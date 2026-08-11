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
    /// 查询词自动补全：返回以 `prefix` 开头的候选词（按文档频率降序），最多 `limit` 条。
    async fn suggest(&self, prefix: &str, limit: usize) -> Result<Vec<String>>;
}

/// 分词：中文走 jieba 词级切分，英文/数字按非字母数字切分并过滤单字符碎片。
///
/// 该函数在 `add` 与 `search` 中保持一致即可保证索引/查询对齐。
/// jieba 为纯 Rust 实现、内置词典、零外部依赖，契合「傻瓜式部署」。
pub fn tokenize(text: &str) -> Vec<String> {
    use jieba_rs::Jieba;
    use std::sync::OnceLock;

    static JIEBA: OnceLock<Jieba> = OnceLock::new();
    let jieba = JIEBA.get_or_init(Jieba::new);

    let mut out = Vec::new();
    for raw in jieba.cut(text, false) {
        let s = raw.word.trim();
        if s.is_empty() {
            continue;
        }
        let has_cjk = s.chars().any(|c| {
            let code = c as u32;
            (0x3400..=0x4DBF).contains(&code)
                || (0x4E00..=0x9FFF).contains(&code)
                || (0xF900..=0xFAFF).contains(&code)
        });
        if has_cjk {
            // 中文：保留词级切分结果（含单字），统一小写。
            out.push(s.to_lowercase());
        } else {
            // 英文/数字：按非字母数字切分，过滤单字符碎片。
            for t in s
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|t| t.len() > 1)
            {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// BM25 单条（词, 文档）评分贡献。
///
/// - `tf`：该词在文档中的词频。
/// - `dl`：文档长度（token 数）。
/// - `avgdl`：全库平均文档长度。
/// - `n`：文档总数，`df`：包含该词的文档数。
/// - `k1`/`b`：标准 BM25 参数（1.5 / 0.75）。
pub fn bm25(tf: f32, dl: usize, avgdl: f64, n: usize, df: usize) -> f64 {
    if df == 0 || avgdl <= 0.0 {
        return 0.0;
    }
    let k1 = 1.5_f64;
    let b = 0.75_f64;
    let idf = ((n as f64 - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln();
    let denom = tf as f64 + k1 * (1.0 - b + b * (dl as f64) / avgdl);
    idf * ((tf as f64) * (k1 + 1.0)) / denom
}

struct IndexState {
    docs: HashMap<String, Doc>,
    /// term -> (doc_id -> 词频 tf)
    postings: HashMap<String, HashMap<String, f32>>,
    /// doc_id -> 文档长度（token 数），用于 BM25。
    doc_len: HashMap<String, usize>,
    /// 所有文档 token 数之和，用于计算平均长度。
    total_len: usize,
    doc_count: usize,
}

/// 进程内倒排索引，基于 `tokio::RwLock` 保护的内存状态。
///
/// 评分采用 BM25（标准 k1/b 参数），支持中文（jieba 分词）。
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
                doc_len: HashMap::new(),
                total_len: 0,
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
        // 仅当文档此前不存在时才计入 doc_count，避免重索引（如演示数据重复写入）使计数膨胀。
        let is_new = s.docs.get(&id).is_none();

        // 若是重新索引，先清理旧词项上的倒排记录与长度统计。
        if let Some(old) = s.docs.get(&id) {
            for t in tokenize(&old.title).into_iter().chain(tokenize(&old.body)) {
                if let Some(m) = s.postings.get_mut(&t) {
                    m.remove(&id);
                }
            }
            if let Some(old_len) = s.doc_len.remove(&id) {
                s.total_len = s.total_len.saturating_sub(old_len);
            }
        }

        s.docs.insert(id.clone(), doc.clone());
        if is_new {
            s.doc_count += 1;
        }

        let len = tokenize(&doc.title).len() + tokenize(&doc.body).len();
        s.doc_len.insert(id.clone(), len);
        s.total_len += len;

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
        let n = s.doc_count;
        let avgdl = if n > 0 {
            s.total_len as f64 / n as f64
        } else {
            0.0
        };
        let mut scores: HashMap<String, f32> = HashMap::new();

        for term in tokenize(&query.q) {
            let df = s.postings.get(&term).map(|m| m.len()).unwrap_or(0);
            if let Some(m) = s.postings.get(&term) {
                for (doc_id, tf) in m {
                    let dl = s.doc_len.get(doc_id).copied().unwrap_or(0);
                    let sc = bm25(*tf, dl, avgdl, n, df) as f32;
                    *scores.entry(doc_id.clone()).or_insert(0.0) += sc;
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

    async fn suggest(&self, prefix: &str, limit: usize) -> Result<Vec<String>> {
        let s = self.inner.read().await;
        let p = prefix.to_lowercase();
        let mut terms: Vec<(String, usize)> = s
            .postings
            .iter()
            .filter(|(t, _)| t.starts_with(&p))
            .map(|(t, m)| (t.clone(), m.len()))
            .collect();
        // 按文档频率降序，频率相同按词字典序，保证结果稳定。
        terms.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(terms
            .into_iter()
            .take(limit.max(1))
            .map(|(t, _)| t)
            .collect())
    }
}
