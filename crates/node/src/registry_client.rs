//! 集群模式下连接远端注册中心的 HTTP 客户端。
//!
//! 实现 `Registry` trait，使上层（爬虫管理器等）无需感知当前是单机内存还是集群远程。
//! 所有网络错误统一转为 `Error::Transport`。

use async_trait::async_trait;
use nekosearch_core::{
    registry::Registry,
    CrawlTask, IdRequest, NodeInfo, RegisterRequest, Result, Role,
};
use reqwest::Client;

#[derive(Clone)]
pub struct HttpRegistryClient {
    client: Client,
    base: String,
}

impl HttpRegistryClient {
    pub fn new(base: String) -> Self {
        Self {
            client: Client::new(),
            base,
        }
    }
}

#[async_trait]
impl Registry for HttpRegistryClient {
    async fn register(&self, req: RegisterRequest) -> Result<()> {
        self.client
            .post(format!("{}/register", self.base))
            .json(&req)
            .send()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(())
    }

    async fn heartbeat(&self, id: &str) -> Result<()> {
        self.client
            .post(format!("{}/heartbeat", self.base))
            .json(&IdRequest { id: id.to_string() })
            .send()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(())
    }

    async fn deregister(&self, id: &str) -> Result<()> {
        self.client
            .post(format!("{}/deregister", self.base))
            .json(&IdRequest { id: id.to_string() })
            .send()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(())
    }

    async fn list_nodes(&self, role: Option<Role>) -> Result<Vec<NodeInfo>> {
        let mut req = self.client.get(format!("{}/nodes", self.base));
        if let Some(r) = role {
            let name = serde_json::to_value(r)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "all".to_string());
            req = req.query(&[("role", name)]);
        }
        let nodes = req
            .send()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?
            .json::<Vec<NodeInfo>>()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(nodes)
    }

    async fn submit_task(&self, task: CrawlTask) -> Result<()> {
        self.client
            .post(format!("{}/tasks", self.base))
            .json(&task)
            .send()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(())
    }

    async fn claim_task(&self, crawler_id: &str) -> Result<Option<CrawlTask>> {
        let res = self
            .client
            .post(format!("{}/tasks/claim", self.base))
            .json(&serde_json::json!({ "crawler_id": crawler_id }))
            .send()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        let task = res
            .json::<Option<CrawlTask>>()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(task)
    }
}
