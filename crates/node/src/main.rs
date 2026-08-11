//! nekosearch 节点进程。
//!
//! 单个二进制 `nekosearch` 通过 `--role` 切换形态：
//! - `all`（默认）：单机全角色，一个进程承载注册中心 + 爬虫 + 索引 + 检索，进程内共享内存。
//! - `registry` / `crawler` / `indexer` / `searcher`：集群形态，各自独立进程，
//!   通过 HTTP 注册中心互相发现与协作，可水平扩容。
//!
//! 关键抽象：无论单机还是集群，爬虫管理器只依赖 `Arc<dyn Registry>` 与 `Arc<dyn Indexer>`，
//! 由 `main` 在启动时按形态装配（内存实现 vs HTTP 客户端），上层逻辑完全一致。

mod crawler;
mod indexer;
mod indexer_client;
mod registry;
mod registry_client;
mod searcher;

use clap::{Parser, ValueEnum};
use nekosearch_core::{
    indexer::{InMemoryIndexer, Indexer},
    registry::{InMemoryRegistry, Registry},
    Role,
};
use std::sync::Arc;

/// 命令行选定的角色。映射到底层 [`Role`] 协议枚举。
#[derive(Clone, Copy, ValueEnum)]
enum RoleArg {
    All,
    Registry,
    Crawler,
    Indexer,
    Searcher,
}

impl From<RoleArg> for Role {
    fn from(r: RoleArg) -> Role {
        match r {
            RoleArg::All => Role::All,
            RoleArg::Registry => Role::Registry,
            RoleArg::Crawler => Role::Crawler,
            RoleArg::Indexer => Role::Indexer,
            RoleArg::Searcher => Role::Searcher,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "nekosearch",
    version,
    about = "Self-hosted search server — single-node by default, cluster-ready"
)]
struct Cli {
    /// 节点角色。`all` = 单机全角色；其余为集群独立角色。
    #[arg(long, value_enum, default_value_t = RoleArg::All, env = "NEKO_ROLE")]
    role: RoleArg,

    /// 注册中心 HTTP 监听地址（本节点承担 registry 角色时生效）。
    #[arg(long, env = "REGISTRY_ADDR", default_value = "0.0.0.0:7700")]
    registry_addr: String,

    /// 索引节点 HTTP 监听地址（本节点承担 indexer 角色时生效）。
    #[arg(long, env = "INDEXER_ADDR", default_value = "0.0.0.0:7900")]
    indexer_addr: String,

    /// 检索服务 HTTP 监听地址（本节点承担 searcher 角色时生效）。
    #[arg(long, env = "SEARCHER_ADDR", default_value = "0.0.0.0:7800")]
    searcher_addr: String,

    /// 远端注册中心基址（集群模式下 crawler/indexer/searcher 连接用）。
    #[arg(long, env = "REGISTRY_REMOTE", default_value = "http://127.0.0.1:7700")]
    registry_remote: String,

    /// 远端索引节点基址（集群模式下 crawler/searcher 写入/查询用）。
    #[arg(long, env = "INDEXER_REMOTE", default_value = "http://127.0.0.1:7900")]
    indexer_remote: String,

    /// 爬虫种子 URL，逗号分隔。
    #[arg(long, env = "SEEDS", value_delimiter = ',', default_value = "")]
    seeds: Vec<String>,

    /// 最大爬取深度（BFS）。
    #[arg(long, env = "MAX_DEPTH", default_value_t = 2)]
    max_depth: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();

    let run_registry = matches!(cli.role, RoleArg::All | RoleArg::Registry);
    let run_indexer = matches!(cli.role, RoleArg::All | RoleArg::Indexer);
    let run_crawler = matches!(cli.role, RoleArg::All | RoleArg::Crawler);
    let run_searcher = matches!(cli.role, RoleArg::All | RoleArg::Searcher);

    // 本节点若承担 registry/indexer 角色，则启动进程内实现；否则使用远端 HTTP 客户端。
    let inmem_registry = if run_registry {
        Some(InMemoryRegistry::new())
    } else {
        None
    };
    let inmem_indexer = if run_indexer {
        Some(InMemoryIndexer::new())
    } else {
        None
    };

    let registry: Arc<dyn Registry> = match &inmem_registry {
        Some(r) => Arc::new(r.clone()),
        None => Arc::new(registry_client::HttpRegistryClient::new(cli.registry_remote.clone())),
    };
    let indexer: Arc<dyn Indexer> = match &inmem_indexer {
        Some(i) => Arc::new(i.clone()),
        None => Arc::new(indexer_client::HttpIndexerClient::new(cli.indexer_remote.clone())),
    };

    if let Some(reg) = &inmem_registry {
        let reg_clone = reg.clone();
        let addr = cli.registry_addr.clone();
        tokio::spawn(async move {
            if let Err(e) = registry::serve(&addr, reg_clone).await {
                tracing::error!("registry server stopped: {e}");
            }
        });
        // 周期性剔除失联节点（10s 未心跳即视为下线）。
        let sweep = reg.clone();
        tokio::spawn(async move {
            loop {
                sweep.sweep_stale(10_000).await;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    }

    if let Some(idx) = &inmem_indexer {
        let idx_clone = idx.clone();
        let addr = cli.indexer_addr.clone();
        tokio::spawn(async move {
            if let Err(e) = indexer::serve(&addr, idx_clone).await {
                tracing::error!("indexer server stopped: {e}");
            }
        });
    }

    if run_searcher {
        let idx = indexer.clone();
        let addr = cli.searcher_addr.clone();
        tokio::spawn(async move {
            if let Err(e) = searcher::serve(&addr, idx).await {
                tracing::error!("searcher server stopped: {e}");
            }
        });
    }

    if run_crawler {
        let seeds: Vec<String> = cli.seeds.iter().filter(|s| !s.is_empty()).cloned().collect();
        let manager = Arc::new(crawler::manager::CrawlerManager::new(
            registry.clone(),
            indexer.clone(),
            seeds,
            cli.max_depth,
        ));
        tokio::spawn(manager.run());
    }

    tracing::info!(
        role = ?cli.role,
        "nekosearch started (single-node default; cluster roles enabled by --role)"
    );

    // 等待退出信号。
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    Ok(())
}
