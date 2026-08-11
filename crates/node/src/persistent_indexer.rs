//! 基于 sled 的持久化索引实现。
//!
//! 默认单机模式使用本实现：文档与倒排记录写入嵌入式 KV 存储（sled），
//! 进程重启后索引不丢。评分逻辑与 `InMemoryIndexer` 保持一致（简化 TF-IDF），
//! 因此单机/集群检索结果可对齐。

use async_trait::async_trait;
use nekosearch_core::indexer::{tokenize, Indexer};
use nekosearch_core::{Doc, Result, SearchQuery, SearchResult};
use std::collections::HashMap;

/// sled 键前缀，避免不同树之间的键冲突。
const DOC_PREFIX: &str = "doc:";
const POST_PREFIX: &str = "post:";
const META_DOC_COUNT: &str = "meta:doc_count";

#[derive(serde::Serialize, serde::Deserialize)]
struct PostingEntry {
    doc_id: String,
    tf: f32,
}

/// 基于 sled 的持久化索引。sled 是纯 Rust 嵌入式 KV，跨平台零外部依赖，契合「傻瓜式部署」。
#[derive(Clone)]
pub struct SledIndexer {
    db: sled::Db,
}

impl SledIndexer {
    /// 打开（或创建）位于 `path` 的索引数据库。
    pub fn open_or_create(path: &str) -> anyhow::Result<Self> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    fn doc_key(id: &str) -> String {
        format!("{DOC_PREFIX}{id}")
    }

    fn post_key(term: &str) -> String {
        format!("{POST_PREFIX}{term}")
    }

    fn get_postings(&self, term: &str) -> Result<Vec<PostingEntry>> {
        match self.db.get(Self::post_key(term)) {
            Ok(Some(bytes)) => Ok(serde_json::from_slice(&bytes)?),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(nekosearch_core::Error::Index(e.to_string())),
        }
    }

    fn set_postings(&self, term: &str, entries: &[PostingEntry]) -> Result<()> {
        let bytes = serde_json::to_vec(entries)?;
        self.db
            .insert(Self::post_key(term), bytes)
            .map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;
        Ok(())
    }

    fn remove_posting(&self, term: &str, doc_id: &str) -> Result<()> {
        let mut entries = self.get_postings(term)?;
        entries.retain(|e| e.doc_id != doc_id);
        self.set_postings(term, &entries)?;
        Ok(())
    }

    fn append_posting(&self, term: &str, doc_id: &str, tf: f32) -> Result<()> {
        let mut entries = self.get_postings(term)?;
        entries.push(PostingEntry {
            doc_id: doc_id.to_string(),
            tf,
        });
        self.set_postings(term, &entries)?;
        Ok(())
    }

    fn doc_count(&self) -> u64 {
        match self.db.get(META_DOC_COUNT) {
            Ok(Some(bytes)) => {
                let s = std::str::from_utf8(&bytes).unwrap_or("0");
                s.parse::<u64>().unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn inc_doc_count(&self) -> Result<()> {
        let n = self.doc_count() + 1;
        self.db
            .insert(META_DOC_COUNT, n.to_string().as_bytes())
            .map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl Indexer for SledIndexer {
    async fn add(&self, doc: Doc) -> Result<()> {
        let id = doc.id.clone();

        // 重新索引：先清理旧词项上的倒排记录。
        let old = self
            .db
            .get(Self::doc_key(&id))
            .map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;
        if let Some(old_bytes) = old {
            if let Ok(old) = serde_json::from_slice::<Doc>(&old_bytes) {
                for t in tokenize(&old.title).into_iter().chain(tokenize(&old.body)) {
                    self.remove_posting(&t, &id)?;
                }
            }
        }

        let doc_json = serde_json::to_vec(&doc)?;
        self.db
            .insert(Self::doc_key(&id), doc_json)
            .map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;

        let mut tfs: HashMap<String, f32> = HashMap::new();
        for t in tokenize(&doc.title).into_iter().chain(tokenize(&doc.body)) {
            *tfs.entry(t).or_insert(0.0) += 1.0;
        }
        for (t, tf) in tfs {
            self.append_posting(&t, &id, tf)?;
        }

        self.inc_doc_count()?;
        // 尽快落盘；sled 也会在后台周期性 flush。
        self.db
            .flush()
            .map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;
        Ok(())
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let n = self.doc_count().max(1) as f32;
        let mut scores: HashMap<String, f32> = HashMap::new();

        for term in tokenize(&query.q) {
            let entries = self.get_postings(&term)?;
            let df = entries.len() as f32;
            // idf = ln((N - df + 0.5) / (df + 0.5) + 1)，未出现的词视为 0。
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0);
            for e in entries {
                let norm = (1.0 + e.tf).ln();
                *scores.entry(e.doc_id).or_insert(0.0) += norm * idf;
            }
        }

        let mut out: Vec<SearchResult> = scores
            .into_iter()
            .filter_map(|(doc_id, score)| {
                self.db
                    .get(Self::doc_key(&doc_id))
                    .ok()
                    .flatten()
                    .and_then(|bytes| serde_json::from_slice::<Doc>(&bytes).ok())
                    .map(|d| SearchResult { doc: d, score })
            })
            .collect();

        out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(query.top_k.max(1));
        Ok(out)
    }
}
