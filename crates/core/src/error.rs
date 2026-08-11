use thiserror::Error;

/// nekosearch 的统一错误类型。
///
/// 所有 crate 内部的 `Result` 均为 `nekosearch_core::Result<T>` 的别名。
/// 跨网络（HTTP 注册中心 / 索引客户端）的错误统一归类为 [`Error::Transport`]，
/// 这样上层无需关心当前是单机内存模式还是集群远程模式。
#[derive(Debug, Error)]
pub enum Error {
    #[error("registry error: {0}")]
    Registry(String),

    #[error("crawler error: {0}")]
    Crawler(String),

    #[error("index error: {0}")]
    Index(String),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// nekosearch 的统一结果别名。
pub type Result<T> = std::result::Result<T, Error>;
