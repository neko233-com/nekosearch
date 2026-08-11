//! 爬虫执行器模块。
//!
//! - `executor`：`CrawlerExecutor` trait，所有抓取实现必须遵循（便于通过注册中心管理、水平扩容）。
//! - `http_crawler` / `fs_crawler`：两个示例执行器。
//! - `manager`：`CrawlerManager`，向注册中心注册并持续领取任务、执行抓取、写入索引、回灌外链继续 BFS。

pub mod executor;
pub mod fs_crawler;
pub mod http_crawler;
pub mod manager;
