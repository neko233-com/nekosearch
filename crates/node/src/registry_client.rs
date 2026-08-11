//! 集群模式下连接远端注册中心的 HTTP 客户端。
//!
//! 实现 `Registry` trait，使上层（爬虫管理器等）无需感知当前是单机内存还是集群远程。
//! `base` 支持填写多个注册中心地址（逗号或空格分隔），按序故障转移；reqwest 默认跟随
//! 307 重定向，因此非 leader 注册中心对写请求的转发对调用方透明（多注册中心 HA，Phase 5）。

use async_trait::async_trait;
use nekosearch_core::{
    registry::Registry, CrawlTask, IdRequest, NodeInfo, RegisterRequest, Result, Role,
};
use reqwest::Client;

#[derive(Clone)]
pub struct HttpRegistryClient {
    client: Client,
    bases: Vec<String>,
}

impl HttpRegistryClient {
    pub fn new(base: String) -> Self {
        let bases: Vec<String> = base
            .split([',', ' '])
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let bases = if bases.is_empty() { vec![base] } else { bases };
        Self {
            client: Client::new(),
            bases,
        }
    }

    /// 依次尝试各注册中心基址（故障转移）。`build` 针对单个基址构造请求；
    /// reqwest 默认跟随 307，非 leader 的写请求会被透明转发到 leader。
    async fn with_failover<F>(&self, path: &str, build: F) -> Result<reqwest::Response>
    where
        F: Fn(&Client, &str) -> reqwest::RequestBuilder,
    {
        let mut last_err = None;
        for base in &self.bases {
            let url = format!("{base}/{path}");
            match build(&self.client, &url).send().await {
                Ok(r) => return Ok(r),
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        Err(nekosearch_core::Error::Transport(
            last_err.unwrap_or_else(|| "no registry endpoint".to_string()),
        ))
    }
}

#[async_trait]
impl Registry for HttpRegistryClient {
    async fn register(&self, req: RegisterRequest) -> Result<()> {
        self.with_failover("register", |c, url| c.post(url).json(&req))
            .await?;
        Ok(())
    }

    async fn heartbeat(&self, id: &str) -> Result<()> {
        self.with_failover("heartbeat", |c, url| {
            c.post(url).json(&IdRequest { id: id.to_string() })
        })
        .await?;
        Ok(())
    }

    async fn deregister(&self, id: &str) -> Result<()> {
        self.with_failover("deregister", |c, url| {
            c.post(url).json(&IdRequest { id: id.to_string() })
        })
        .await?;
        Ok(())
    }

    async fn list_nodes(&self, role: Option<Role>) -> Result<Vec<NodeInfo>> {
        let name = role.map(|r| {
            serde_json::to_value(r)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "all".to_string())
        });
        let nodes = self
            .with_failover("nodes", |c, url| {
                let mut r = c.get(url);
                if let Some(n) = &name {
                    r = r.query(&[("role", n)]);
                }
                r
            })
            .await?
            .json::<Vec<NodeInfo>>()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(nodes)
    }

    async fn submit_task(&self, task: CrawlTask) -> Result<()> {
        self.with_failover("tasks", |c, url| c.post(url).json(&task))
            .await?;
        Ok(())
    }

    async fn claim_task(&self, crawler_id: &str) -> Result<Option<CrawlTask>> {
        let res = self
            .with_failover("tasks/claim", |c, url| {
                c.post(url)
                    .json(&serde_json::json!({ "crawler_id": crawler_id }))
            })
            .await?;
        let task = res
            .json::<Option<CrawlTask>>()
            .await
            .map_err(|e| nekosearch_core::Error::Transport(e.to_string()))?;
        Ok(task)
    }
}
