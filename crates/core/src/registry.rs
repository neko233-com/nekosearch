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

/// 多注册中心高可用状态（零外部依赖，基于心跳租约的 leader 选举）。
struct HaState {
    /// 本节点对外可达的标识（即 advertise URL，例如 http://127.0.0.1:7510）。
    self_id: String,
    /// 配置的对端注册中心地址列表。
    peers: Vec<String>,
    /// 对端视图：peer_addr -> (peer 的 self_id, 最近一次心跳可见的 Unix 毫秒)。
    peer_view: HashMap<String, (String, i64)>,
    /// 当前 leader 的 self_id（启动时假定为自己，运行期由选主刷新）。
    leader: String,
}

/// 单机/进程内注册中心实现，基于 `tokio::RwLock` 保护的内存状态。
///
/// 该类型自身是 `Clone`（内部为 `Arc`），可作为 axum 的 `State` 在 HTTP 服务间共享，
/// 也可直接作为进程内 trait 对象交给爬虫管理器使用。多注册中心高可用（Phase 5）通过
/// [`InMemoryRegistry::new_with_ha`] 注入 `self_id`/`peers`，并由 node 侧心跳循环驱动选主。
#[derive(Clone)]
pub struct InMemoryRegistry {
    inner: Arc<tokio::sync::RwLock<RegistryState>>,
    ha: Arc<tokio::sync::RwLock<HaState>>,
}

impl InMemoryRegistry {
    pub fn new() -> Self {
        Self::new_with_ha("local".to_string(), Vec::new())
    }

    /// 构造带 HA 信息的注册中心。`self_id` 为本节点可达标识，`peers` 为对端地址。
    /// `peers` 为空时本节点即为唯一 leader（单机/无 HA 场景，行为与 `new()` 一致）。
    pub fn new_with_ha(self_id: String, peers: Vec<String>) -> Self {
        let leader = self_id.clone();
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(RegistryState {
                nodes: HashMap::new(),
                queue: VecDeque::new(),
            })),
            ha: Arc::new(tokio::sync::RwLock::new(HaState {
                self_id,
                peers,
                peer_view: HashMap::new(),
                leader,
            })),
        }
    }

    /// 本节点的 HA 标识。
    pub async fn self_id(&self) -> String {
        self.ha.read().await.self_id.clone()
    }

    /// 对端地址列表（供 HA 心跳循环使用）。
    pub async fn ha_peers(&self) -> Vec<String> {
        self.ha.read().await.peers.clone()
    }

    /// 记录某对端在线及其 self_id（HA 心跳循环调用）。
    pub async fn observe_peer(&self, addr: &str, id: &str) {
        let mut h = self.ha.write().await;
        h.peer_view
            .insert(addr.to_string(), (id.to_string(), now_millis()));
    }

    /// 剔除超过 `ttl_ms` 未心跳的对端。
    pub async fn drop_stale_peers(&self, ttl_ms: i64) {
        let mut h = self.ha.write().await;
        let cutoff = now_millis() - ttl_ms;
        h.peer_view.retain(|_, (_, seen)| *seen >= cutoff);
    }

    /// 重新计算 leader：在 {本节点} ∪ {在线对端 id} 中取字典序最小者（稳定、确定性）。
    pub async fn recompute_leader(&self) {
        let mut h = self.ha.write().await;
        let mut candidate = h.self_id.clone();
        for (id, _) in h.peer_view.values() {
            if id < &candidate {
                candidate = id.clone();
            }
        }
        h.leader = candidate;
    }

    /// 本节点是否为当前 leader。
    pub async fn is_leader(&self) -> bool {
        let h = self.ha.read().await;
        h.leader == h.self_id
    }

    /// 当前 leader 的标识（即可直接用于重定向的地址）。
    pub async fn leader(&self) -> String {
        self.ha.read().await.leader.clone()
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
            .filter(|n| role.is_none_or(|r| n.role == r))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_list_nodes() {
        let reg = InMemoryRegistry::new();
        reg.register(RegisterRequest {
            id: "node1".into(),
            role: Role::Crawler,
            addr: "http://127.0.0.1:8001".into(),
        })
        .await
        .unwrap();

        let nodes = reg.list_nodes(None).await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "node1");
    }

    #[tokio::test]
    async fn list_nodes_filter_by_role() {
        let reg = InMemoryRegistry::new();
        reg.register(RegisterRequest {
            id: "c1".into(),
            role: Role::Crawler,
            addr: "http://127.0.0.1:8001".into(),
        })
        .await
        .unwrap();
        reg.register(RegisterRequest {
            id: "s1".into(),
            role: Role::Searcher,
            addr: "http://127.0.0.1:8002".into(),
        })
        .await
        .unwrap();

        let crawlers = reg.list_nodes(Some(Role::Crawler)).await.unwrap();
        assert_eq!(crawlers.len(), 1);
        assert_eq!(crawlers[0].id, "c1");
    }

    #[tokio::test]
    async fn deregister_removes_node() {
        let reg = InMemoryRegistry::new();
        reg.register(RegisterRequest {
            id: "node1".into(),
            role: Role::Indexer,
            addr: "http://127.0.0.1:8001".into(),
        })
        .await
        .unwrap();
        reg.deregister("node1").await.unwrap();

        let nodes = reg.list_nodes(None).await.unwrap();
        assert!(nodes.is_empty());
    }

    #[tokio::test]
    async fn heartbeat_updates_timestamp() {
        let reg = InMemoryRegistry::new();
        reg.register(RegisterRequest {
            id: "node1".into(),
            role: Role::All,
            addr: "http://127.0.0.1:8001".into(),
        })
        .await
        .unwrap();

        let before = reg.list_nodes(None).await.unwrap()[0].last_heartbeat;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        reg.heartbeat("node1").await.unwrap();
        let after = reg.list_nodes(None).await.unwrap()[0].last_heartbeat;
        assert!(after > before);
    }

    #[tokio::test]
    async fn heartbeat_unknown_node_errors() {
        let reg = InMemoryRegistry::new();
        let result = reg.heartbeat("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn submit_and_claim_task_fifo() {
        let reg = InMemoryRegistry::new();
        reg.submit_task(CrawlTask {
            id: "t1".into(),
            url: "https://a.com".into(),
            depth: 0,
        })
        .await
        .unwrap();
        reg.submit_task(CrawlTask {
            id: "t2".into(),
            url: "https://b.com".into(),
            depth: 0,
        })
        .await
        .unwrap();

        let first = reg.claim_task("crawler1").await.unwrap();
        assert_eq!(first.unwrap().id, "t1");
        let second = reg.claim_task("crawler1").await.unwrap();
        assert_eq!(second.unwrap().id, "t2");
        let empty = reg.claim_task("crawler1").await.unwrap();
        assert!(empty.is_none());
    }

    #[tokio::test]
    async fn pending_tasks_count() {
        let reg = InMemoryRegistry::new();
        assert_eq!(reg.pending_tasks().await, 0);
        reg.submit_task(CrawlTask {
            id: "t1".into(),
            url: "https://a.com".into(),
            depth: 0,
        })
        .await
        .unwrap();
        assert_eq!(reg.pending_tasks().await, 1);
    }

    #[tokio::test]
    async fn ha_single_node_is_leader() {
        let reg = InMemoryRegistry::new();
        assert!(reg.is_leader().await);
        assert_eq!(reg.leader().await, "local");
    }

    #[tokio::test]
    async fn ha_leader_election_min_id_wins() {
        let reg = InMemoryRegistry::new_with_ha("node-b".into(), vec![]);
        reg.observe_peer("http://node-a:7510", "node-a").await;
        reg.recompute_leader().await;
        // node-a < node-b 字典序，所以 node-a 应该是 leader
        assert_eq!(reg.leader().await, "node-a");
        assert!(!reg.is_leader().await);
    }
}
