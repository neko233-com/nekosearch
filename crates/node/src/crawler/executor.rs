use async_trait::async_trait;
use nekosearch_core::{CrawlResult, CrawlTask};

/// 爬虫执行器抽象。
///
/// 新增一种抓取源（如 S3、数据库、RSS）只需实现本 trait 并在 `CrawlerManager` 注册，
/// 注册中心即可统一调度，天然支持水平扩容——这是 nekosearch 接入新数据源的扩展点。
#[async_trait]
pub trait CrawlerExecutor: Send + Sync {
    /// 执行器名称，用于标识。
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    /// 执行一次抓取，返回结构化结果。
    async fn crawl(&self, task: &CrawlTask) -> anyhow::Result<CrawlResult>;
}
