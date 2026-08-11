use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::protocol::*;
use crate::Result;

/// 索引/检索抽象。
///
/// 单机模式由 [`InMemoryIndexer`] 提供进程内倒排索引；
/// 集群模式由 [`crate::indexer::HttpIndexerClient`](crate) 通过 HTTP 写入/查询远端索引节点。
/// 爬虫只调用 `add`，检索服务只调用 `search`，因此两种模式可无缝切换。
#[async_trait]
pub trait Indexer: Send + Sync {
    /// 索引一个文档。
    async fn add(&self, doc: Doc) -> Result<()>;
    /// 执行检索，返回按相关性降序排列的结果（最多 `top_k` 条）。
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
}

/// 极简英文/数字分词：转小写、按非字母数字切分、过滤单字符碎片。
///
/// 注意：这是骨架级分词器，中文需要后续接入专用分词（如 jieba）才能有好的召回。
/// 该函数在 `add` 与 `search` 中保持一致即可保证索引/查询对齐。
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(|t| t.to_string())
        .collect()
}

struct IndexState {
    docs: HashMap<String, Doc>,
    /// term -> (doc_id -> 词频 tf)
    postings: HashMap<String, HashMap<String, f32>>,
    doc_count: usize,
}

/// 进程内倒排索引，基于 `tokio::RwLock` 保护的内存状态。
///
/// 评分采用简化版的 TF-IDF：对每个查询词累加 `(1+tf).ln() * idf`，
/// 仅用于演示检索链路，生产环境应替换为更成熟的检索库（Tantivy / Meilisearch 等）。
#[derive(Clone)]
pub struct InMemoryIndexer {
    inner: Arc<tokio::sync::RwLock<IndexState>>,
}

impl InMemoryIndexer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(IndexState {
                docs: HashMap::new(),
                postings: HashMap::new(),
                doc_count: 0,
            })),
        }
    }
}

impl Default for InMemoryIndexer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Indexer for InMemoryIndexer {
    async fn add(&self, doc: Doc) -> Result<()> {
        let mut s = self.inner.write().await;
        let id = doc.id.clone();

        // 若是重新索引，先清理旧词项上的倒排记录。
        if let Some(old) = s.docs.get(&id) {
            for t in tokenize(&old.title).into_iter().chain(tokenize(&old.body)) {
                if let Some(m) = s.postings.get_mut(&t) {
                    m.remove(&id);
                }
            }
        }

        s.docs.insert(id.clone(), doc.clone());
        s.doc_count += 1;

        let mut tfs: HashMap<String, f32> = HashMap::new();
        for t in tokenize(&doc.title).into_iter().chain(tokenize(&doc.body)) {
            *tfs.entry(t).or_insert(0.0) += 1.0;
        }
        for (t, tf) in tfs {
            s.postings.entry(t).or_default().insert(id.clone(), tf);
        }
        Ok(())
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let s = self.inner.read().await;
        let n = s.doc_count.max(1) as f32;
        let mut scores: HashMap<String, f32> = HashMap::new();

        for term in tokenize(&query.q) {
            let df = s.postings.get(&term).map(|m| m.len()).unwrap_or(0) as f32;
            // idf = ln((N - df + 0.5) / (df + 0.5) + 1)，对未出现的词视为 0。
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0);
            if let Some(m) = s.postings.get(&term) {
                for (doc_id, tf) in m {
                    let norm = (1.0 + *tf).ln();
                    *scores.entry(doc_id.clone()).or_insert(0.0) += norm * idf;
                }
            }
        }

        let mut out: Vec<SearchResult> = scores
            .into_iter()
            .filter_map(|(doc_id, score)| {
                s.docs.get(&doc_id).map(|d| SearchResult {
                    doc: d.clone(),
                    score,
                })
            })
            .collect();

        out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(query.top_k.max(1));
        Ok(out)
    }
}
