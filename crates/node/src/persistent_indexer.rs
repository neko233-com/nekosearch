//! 基于 sled 的持久化索引实现。
//!
//! 默认单机模式使用本实现：文档与倒排记录写入嵌入式 KV 存储（sled），
//! 进程重启后索引不丢。评分采用 BM25（与 `InMemoryIndexer` 一致），支持中文（jieba 分词）。

use async_trait::async_trait;
use nekosearch_core::indexer::{bm25, tokenize, Indexer};
use nekosearch_core::{Doc, Result, SearchQuery, SearchResult};
use std::collections::HashMap;

/// sled 键前缀，避免不同树之间的键冲突。
const DOC_PREFIX: &str = "doc:";
const POST_PREFIX: &str = "post:";
const META_DOC_COUNT: &str = "meta:doc_count";
const META_TOTAL_LEN: &str = "meta:total_len";
const LEN_PREFIX: &str = "len:";

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

    fn len_key(id: &str) -> String {
        format!("{LEN_PREFIX}{id}")
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

    fn get_u64(&self, key: &str) -> u64 {
        match self.db.get(key) {
            Ok(Some(bytes)) => std::str::from_utf8(&bytes)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
            _ => 0,
        }
    }

    fn set_u64(&self, key: &str, v: u64) -> Result<()> {
        self.db
            .insert(key, v.to_string().as_bytes())
            .map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;
        Ok(())
    }

    fn doc_count(&self) -> u64 {
        self.get_u64(META_DOC_COUNT)
    }

    fn total_len(&self) -> u64 {
        self.get_u64(META_TOTAL_LEN)
    }

    fn get_doc_len(&self, id: &str) -> u64 {
        self.get_u64(&Self::len_key(id))
    }

    fn inc_doc_count(&self) -> Result<()> {
        self.set_u64(META_DOC_COUNT, self.doc_count() + 1)
    }
}

#[async_trait]
impl Indexer for SledIndexer {
    async fn add(&self, doc: Doc) -> Result<()> {
        let id = doc.id.clone();

        // 重新索引：先清理旧词项上的倒排记录与长度统计。
        let old = self
            .db
            .get(Self::doc_key(&id))
            .map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;
        // 仅当文档此前不存在时才计入 doc_count，避免重索引（如演示数据重复写入）使计数膨胀。
        let is_new = old.is_none();
        if let Some(old_bytes) = old {
            if let Ok(old) = serde_json::from_slice::<Doc>(&old_bytes) {
                for t in tokenize(&old.title).into_iter().chain(tokenize(&old.body)) {
                    self.remove_posting(&t, &id)?;
                }
            }
            let old_len = self.get_doc_len(&id);
            if old_len > 0 {
                self.set_u64(META_TOTAL_LEN, self.total_len().saturating_sub(old_len))?;
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

        let len = (tokenize(&doc.title).len() + tokenize(&doc.body).len()) as u64;
        self.set_u64(&Self::len_key(&id), len)?;
        self.set_u64(META_TOTAL_LEN, self.total_len() + len)?;
        if is_new {
            self.inc_doc_count()?;
        }

        // 尽快落盘；sled 也会在后台周期性 flush。
        self.db
            .flush()
            .map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;
        Ok(())
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let n = self.doc_count().max(1) as usize;
        let total = self.total_len();
        let avgdl = if n > 0 { total as f64 / n as f64 } else { 0.0 };
        let mut scores: HashMap<String, f32> = HashMap::new();

        for term in tokenize(&query.q) {
            let entries = self.get_postings(&term)?;
            let df = entries.len();
            for e in entries {
                let dl = self.get_doc_len(&e.doc_id) as usize;
                let sc = bm25(e.tf, dl, avgdl, n, df) as f32;
                *scores.entry(e.doc_id).or_insert(0.0) += sc;
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

        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.truncate(query.top_k.max(1));
        Ok(out)
    }

    async fn suggest(&self, prefix: &str, limit: usize) -> Result<Vec<String>> {
        let p = prefix.to_lowercase();
        let mut terms: Vec<(String, usize)> = Vec::new();
        for item in self.db.scan_prefix(POST_PREFIX) {
            let (k, v) = item.map_err(|e| nekosearch_core::Error::Index(e.to_string()))?;
            let kstr = std::str::from_utf8(&k).unwrap_or("");
            if let Some(term) = kstr.strip_prefix(POST_PREFIX) {
                if term.starts_with(&p) {
                    let df = serde_json::from_slice::<Vec<PostingEntry>>(&v)
                        .map(|v| v.len())
                        .unwrap_or(0);
                    terms.push((term.to_string(), df));
                }
            }
        }
        terms.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(terms
            .into_iter()
            .take(limit.max(1))
            .map(|(t, _)| t)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekosearch_core::{Doc, SearchQuery};

    fn unique_tmp() -> String {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("nekosearch-sled-test-{n}"))
            .to_string_lossy()
            .to_string()
    }

    #[tokio::test]
    async fn sled_add_search_and_persist_across_reopen() {
        let dir = unique_tmp();
        let idx = SledIndexer::open_or_create(&dir).unwrap();
        idx.add(Doc {
            id: "s".into(),
            url: "https://rust-lang.org".into(),
            title: "Rust".into(),
            body: "rust programming language".into(),
        })
        .await
        .unwrap();
        let r = idx
            .search(&SearchQuery {
                q: "rust".into(),
                top_k: 5,
            })
            .await
            .unwrap();
        assert!(r.iter().any(|x| x.doc.id == "s"), "写入后应可检索");

        // 关闭后重新打开，索引应仍在（持久化）。
        drop(idx);
        let idx2 = SledIndexer::open_or_create(&dir).unwrap();
        let r2 = idx2
            .search(&SearchQuery {
                q: "rust".into(),
                top_k: 5,
            })
            .await
            .unwrap();
        assert!(r2.iter().any(|x| x.doc.id == "s"), "重启后索引应仍在");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
