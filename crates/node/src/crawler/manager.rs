use std::sync::Arc;
use std::time::Duration;

use nekosearch_core::{
    indexer::Indexer,
    registry::{now_millis, Registry},
    CrawlTask, Doc, RegisterRequest, Role,
};

use crate::crawler::executor::CrawlerExecutor;
use crate::crawler::fs_crawler::FsCrawler;
use crate::crawler::http_crawler::HttpCrawler;

/// 爬虫管理器：向注册中心注册，持续领取抓取任务，执行后写入索引，
/// 并把页面外链回灌为新的抓取任务（BFS 扩散）。单机/集群模式共用同一套逻辑，
/// 区别仅在 `registry` / `indexer` 是进程内实现还是远端 HTTP 客户端。
pub struct CrawlerManager {
    registry: Arc<dyn Registry>,
    indexer: Arc<dyn Indexer>,
    seeds: Vec<String>,
    max_depth: u32,
    node_id: String,
}

impl CrawlerManager {
    pub fn new(
        registry: Arc<dyn Registry>,
        indexer: Arc<dyn Indexer>,
        seeds: Vec<String>,
        max_depth: u32,
    ) -> Self {
        Self {
            registry,
            indexer,
            seeds,
            max_depth,
            node_id: format!("crawler-{}", now_millis()),
        }
    }

    /// 主循环：提交种子 -> 注册 -> 不断领取并执行任务。
    ///
    /// 该 future 永不返回（除非底层注册中心持续不可达）；由 `tokio::spawn` 托管。
    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        for (i, url) in self.seeds.iter().enumerate() {
            let task = CrawlTask {
                id: format!("seed-{}", i),
                url: url.clone(),
                depth: 0,
            };
            if let Err(e) = self.registry.submit_task(task).await {
                tracing::warn!("submit seed failed: {e}");
            }
        }

        if let Err(e) = self
            .registry
            .register(RegisterRequest {
                id: self.node_id.clone(),
                role: Role::Crawler,
                addr: "local".to_string(),
            })
            .await
        {
            tracing::warn!("register failed: {e}");
        }

        let http = HttpCrawler::new();
        let fs = FsCrawler::new();

        loop {
            match self.registry.claim_task(&self.node_id).await {
                Ok(Some(task)) => {
                    let exec: &dyn CrawlerExecutor = if task.url.starts_with("file://")
                        || task.url.starts_with("http://")
                        || task.url.starts_with("https://")
                    {
                        &http
                    } else {
                        &fs
                    };

                    match exec.crawl(&task).await {
                        Ok(result) => {
                            let doc = Doc {
                                id: result.task_id.clone(),
                                url: result.url.clone(),
                                title: result.title.clone(),
                                body: result.text.clone(),
                            };
                            if let Err(e) = self.indexer.add(doc).await {
                                tracing::warn!("index failed: {e}");
                            }
                            // BFS 扩散：把外链作为更深一层的任务回灌。
                            if task.depth < self.max_depth {
                                for (i, link) in result.links.into_iter().take(50).enumerate() {
                                    let nt = CrawlTask {
                                        id: format!("{}-l{}", result.task_id, i),
                                        url: link,
                                        depth: task.depth + 1,
                                    };
                                    let _ = self.registry.submit_task(nt).await;
                                }
                            }
                        }
                        Err(e) => tracing::warn!("crawl failed for {}: {e}", task.url),
                    }
                }
                Ok(None) => tokio::time::sleep(Duration::from_millis(500)).await,
                Err(e) => {
                    tracing::warn!("claim failed: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}
