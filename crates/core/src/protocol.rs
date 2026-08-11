use serde::{Deserialize, Serialize};

/// 节点角色。
///
/// - `All`：单机默认形态，一个进程承载注册中心 + 爬虫 + 索引 + 检索全部角色。
/// - 其余为集群形态下的独立角色，可水平扩容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Registry,
    Crawler,
    Indexer,
    Searcher,
    All,
}

/// 已注册的节点信息，由注册中心维护。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub role: Role,
    /// 该节点对外暴露的基址，例如 `http://10.0.0.5:7511`。
    pub addr: String,
    /// 最近一次心跳时间（Unix 毫秒），用于剔除失联节点。
    pub last_heartbeat: i64,
}

/// 注册请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub id: String,
    pub role: Role,
    pub addr: String,
}

/// 仅携带节点 id 的请求（心跳 / 注销）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdRequest {
    pub id: String,
}

/// 一个抓取任务。`depth` 用于限制 BFS 爬取深度，避免无限扩散。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlTask {
    pub id: String,
    pub url: String,
    pub depth: u32,
}

/// 一次抓取的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    pub task_id: String,
    pub url: String,
    pub title: String,
    /// 已去除 HTML 标签的正文文本。
    pub text: String,
    /// 从页面中抽取出的外链，供注册中心继续派发为新任务（BFS）。
    pub links: Vec<String>,
}

/// 被索引的文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    pub id: String,
    pub url: String,
    pub title: String,
    pub body: String,
}

/// 检索请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub top_k: usize,
}

/// 单条检索结果，附带相关性得分。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub doc: Doc,
    pub score: f32,
}
