//! 集群模式下连接远端索引节点的 HTTP 客户端。
//!
//! 实现 `Indexer` trait，使爬虫与检索服务无需感知当前是单机内存还是集群远程。

use async_trait::async_trait;
use nekosearch_core::{
    indexer::Indexer,
    Doc, Result, SearchQuery, SearchResult,
};
use reqwest::Client;

#[derive(Clone)]
pub struct HttpIndexerClient {
    client: Client,
    base: String,
}

impl HttpIndexerClient {
    pub fn new(base: String) -> Self {
        Self {
            client: Client::new(),
            base,
        }
    }
}

#[async_trait]
impl Indexer for HttpIndexerClient {
    async fn add(&self, doc: Doc) -> Result<()> {
        self.client
            .post(format!("{}/docs", self.base))
            .json(&doc)
            .send()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(())
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let res = self
            .client
            .post(format!("{}/search", self.base))
            .json(query)
            .send()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        let results = res
            .json::<Vec<SearchResult>>()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(results)
    }
}
