//! 集群模式下的分片索引客户端。
//!
//! 按 `doc.id` 的稳定哈希把文档路由到某个分片；每个分片可有多个副本（分片之间用 `,` 分隔，
//! 同一分片的副本用 `|` 分隔）。`add` 写入该分片全部副本，`search` 跨所有分片检索并合并 top_k。
//! 爬虫/检索服务对分片无感知，仍只依赖 `Arc<dyn Indexer>`。

use async_trait::async_trait;
use nekosearch_core::{
    indexer::Indexer,
    Doc, Result, SearchQuery, SearchResult,
};
use reqwest::Client;

/// 按 `indexer_remote` 解析出的分片拓扑：外层 Vec 是分片，内层 Vec 是副本基址。
pub struct ShardedIndexer {
    client: Client,
    shards: Vec<Vec<String>>,
}

impl ShardedIndexer {
    /// 解析分片拓扑。
    ///
    /// 格式：`分片0副本A|分片0副本B,分片1副本A`。
    /// - 分片之间用 `,` 分隔；
    /// - 同一分片的多个副本用 `|` 分隔。
    pub fn new(indexer_remote: &str) -> anyhow::Result<Self> {
        let shards: Vec<Vec<String>> = indexer_remote
            .split(',')
            .map(|shard| {
                shard
                    .split('|')
                    .map(|r| r.trim().trim_end_matches('/').to_string())
                    .filter(|r| !r.is_empty())
                    .collect::<Vec<String>>()
            })
            .filter(|v| !v.is_empty())
            .collect();
        if shards.is_empty() {
            anyhow::bail!("indexer_remote 为空或格式非法，无法构造分片索引客户端");
        }
        Ok(Self {
            client: Client::new(),
            shards,
        })
    }

    /// 稳定哈希：把文档固定到某个分片。
    fn shard_for(&self, id: &str) -> usize {
        let mut h: u64 = 1469598103934665603; // FNV-1a offset basis
        for b in id.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(1099511628211);
        }
        (h as usize) % self.shards.len()
    }
}

#[async_trait]
impl Indexer for ShardedIndexer {
    async fn add(&self, doc: Doc) -> Result<()> {
        let shard = self.shard_for(&doc.id);
        let mut last_err: Option<String> = None;
        for replica in &self.shards[shard] {
            let url = format!("{replica}/docs");
            match self.client.post(&url).json(&doc).send().await {
                Ok(resp) if resp.status().is_success() => {}
                Ok(resp) => {
                    last_err = Some(format!("replica {replica} 返回状态 {}", resp.status()));
                }
                Err(e) => {
                    last_err = Some(format!("replica {replica} 请求失败: {e}"));
                }
            }
        }
        match last_err {
            Some(e) => Err(nekosearch_core::Error::Index(e)),
            None => Ok(()),
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let mut all: Vec<SearchResult> = Vec::new();
        for shard in &self.shards {
            // 每个分片查询第一个（主）副本。
            let replica = &shard[0];
            let url = format!("{replica}/search");
            match self.client.post(&url).json(query).send().await {
                Ok(resp) => {
                    if let Ok(mut res) = resp.json::<Vec<SearchResult>>().await {
                        all.append(&mut res);
                    }
                }
                Err(e) => {
                    tracing::warn!("分片 {replica} 检索失败: {e}");
                }
            }
        }
        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(query.top_k.max(1));
        Ok(all)
    }

    async fn suggest(&self, prefix: &str, limit: usize) -> Result<Vec<String>> {
        let mut merged: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for shard in &self.shards {
            // 每个分片查询第一个（主）副本的 /suggest。
            let replica = &shard[0];
            let url = format!("{replica}/suggest");
            let lim = limit.max(1).to_string();
            match self
                .client
                .get(&url)
                .query(&[("q", prefix), ("limit", lim.as_str())])
                .send()
                .await
            {
                Ok(resp) => {
                    if let Ok(list) = resp.json::<Vec<String>>().await {
                        for t in list {
                            *merged.entry(t).or_insert(0) += 1;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("分片 {replica} suggest 失败: {e}");
                }
            }
        }
        let mut out: Vec<String> = merged.into_keys().collect();
        out.sort();
        out.truncate(limit.max(1));
        Ok(out)
    }
}
