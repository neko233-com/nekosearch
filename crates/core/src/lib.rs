//! `nekosearch-core` 是 nekosearch 的共享内核：
//! 定义节点角色、注册中心协议、索引/检索协议、抓取的通用数据结构与错误类型。
//!
//! 设计目标：单机与集群共用同一套抽象，默认单机（`--role all`），
//! 集群时各角色以独立进程运行，通过 HTTP 注册中心互相发现与协作。

pub mod error;
pub mod indexer;
pub mod protocol;
pub mod registry;

pub use error::{Error, Result};
pub use indexer::*;
pub use protocol::*;
pub use registry::*;
