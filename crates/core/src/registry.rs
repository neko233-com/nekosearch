use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::*;
use crate::Result;

/// 注册中心抽象。
///
/// 这是整个集群的“大脑”：负责节点发现（register/heartbeat/list_nodes）
/// 与抓取任务调度（submit_task/claim_task）。
///
/// 单机模式下由 [`InMemoryRegistry`] 以进程内共享内存实现；
/// 集群模式下由 [`crate::registry::HttpRegistryClient`](crate) 通过 HTTP 访问远端注册中心。
/// 上层（爬虫管理器等）只依赖这个 trait，因此两种模式代码完全复用。
#[async_trait]
pub trait Registry: Send + Sync {
    /// 节点上线注册。
    async fn register(&self, req: RegisterRequest) -> Result<()>;
    /// 节点心跳，刷新 `last_heartbeat`。
    async fn heartbeat(&self, id: &str) -> Result<()>;
    /// 节点下线注销。
    async fn deregister(&self, id: &str) -> Result<()>;
    /// 列出节点，可按角色过滤。
    async fn list_nodes(&self, role: Option<Role>) -> Result<Vec<NodeInfo>>;
    /// 提交一个抓取任务到队列。
    async fn submit_task(&self, task: CrawlTask) -> Result<()>;
    /// 爬虫领取一个任务（队列为空时返回 `None`）。
    async fn claim_task(&self, crawler_id: &str) -> Result<Option<CrawlTask>>;
}

/// 当前 Unix 毫秒时间戳。
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct RegistryState {
    nodes: HashMap<String, NodeInfo>,
    queue: VecDeque<CrawlTask>,
}

/// 单机/进程内注册中心实现，基于 `tokio::RwLock` 保护的内存状态。
///
/// 该类型自身是 `Clone`（内部为 `Arc`），可作为 axum 的 `State` 在 HTTP 服务间共享，
/// 也可直接作为进程内 trait 对象交给爬虫管理器使用。
#[derive(Clone)]
pub struct InMemoryRegistry {
    inner: Arc<tokio::sync::RwLock<RegistryState>>,
}

impl InMemoryRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(RegistryState {
                nodes: HashMap::new(),
                queue: VecDeque::new(),
            })),
        }
    }

    /// 剔除超过 `ttl_ms` 未心跳的节点。由注册中心 HTTP 服务周期性调用。
    pub async fn sweep_stale(&self, ttl_ms: i64) {
        let mut s = self.inner.write().await;
        let cutoff = now_millis() - ttl_ms;
        s.nodes.retain(|_, n| n.last_heartbeat >= cutoff);
    }

    /// 当前积压的任务数（用于观测/测试）。
    pub async fn pending_tasks(&self) -> usize {
        self.inner.read().await.queue.len()
    }
}

impl Default for InMemoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Registry for InMemoryRegistry {
    async fn register(&self, req: RegisterRequest) -> Result<()> {
        let info = NodeInfo {
            id: req.id,
            role: req.role,
            addr: req.addr,
            last_heartbeat: now_millis(),
        };
        self.inner.write().await.nodes.insert(info.id.clone(), info);
        Ok(())
    }

    async fn heartbeat(&self, id: &str) -> Result<()> {
        let mut s = self.inner.write().await;
        match s.nodes.get_mut(id) {
            Some(n) => {
                n.last_heartbeat = now_millis();
                Ok(())
            }
            None => Err(crate::Error::Registry(format!("unknown node {id}"))),
        }
    }

    async fn deregister(&self, id: &str) -> Result<()> {
        self.inner.write().await.nodes.remove(id);
        Ok(())
    }

    async fn list_nodes(&self, role: Option<Role>) -> Result<Vec<NodeInfo>> {
        let s = self.inner.read().await;
        Ok(s.nodes
            .values()
            .filter(|n| role.map_or(true, |r| n.role == r))
            .cloned()
            .collect())
    }

    async fn submit_task(&self, task: CrawlTask) -> Result<()> {
        self.inner.write().await.queue.push_back(task);
        Ok(())
    }

    async fn claim_task(&self, _crawler_id: &str) -> Result<Option<CrawlTask>> {
        Ok(self.inner.write().await.queue.pop_front())
    }
}
