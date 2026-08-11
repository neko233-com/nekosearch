use async_trait::async_trait;
use nekosearch_core::{CrawlResult, CrawlTask};

use crate::crawler::executor::CrawlerExecutor;

/// 文件系统爬虫执行器（示例），抓取本地文本文件。
///
/// 任务 `url` 形如 `file:///path/to/file.txt` 或 `/abs/path.txt`。
pub struct FsCrawler;

impl FsCrawler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CrawlerExecutor for FsCrawler {
    fn name(&self) -> &'static str {
        "fs"
    }

    async fn crawl(&self, task: &CrawlTask) -> anyhow::Result<CrawlResult> {
        let path = task.url.trim_start_matches("file://");
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let title = std::path::Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&task.url)
            .to_string();
        Ok(CrawlResult {
            task_id: task.id.clone(),
            url: task.url.clone(),
            title,
            text,
            links: Vec::new(),
        })
    }
}
