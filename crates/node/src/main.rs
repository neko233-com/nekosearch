//! nekosearch 节点进程。
//!
//! 单个二进制 `nekosearch` 通过 `--role` 切换形态：
//! - `all`（默认）：单机全角色，一个进程承载注册中心 + 爬虫 + 索引 + 检索，进程内共享内存。
//! - `registry` / `crawler` / `indexer` / `searcher`：集群形态，各自独立进程，
//!   通过 HTTP 注册中心互相发现与协作，可水平扩容。
//!
//! 关键抽象：无论单机还是集群，爬虫管理器只依赖 `Arc<dyn Registry>` 与 `Arc<dyn Indexer>`，
//! 由 `main` 在启动时按形态装配（内存/持久化实现 vs HTTP 客户端），上层逻辑完全一致。
//!
//! 配置以 `config.yaml` 为主（见 `config.yaml.example`），命令行参数与环境变量优先。

mod config;
mod crawler;
mod indexer;
mod indexer_client;
mod persistent_indexer;
mod registry;
mod registry_client;
mod searcher;

use clap::{Parser, ValueEnum};
use nekosearch_core::{indexer::Indexer, registry::{InMemoryRegistry, Registry}, Role};
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

impl RoleArg {
    /// 角色对应的字符串表示（用于覆盖 YAML 配置中的 `role` 字段）。
    fn as_str(self) -> &'static str {
        match self {
            RoleArg::All => "all",
            RoleArg::Registry => "registry",
            RoleArg::Crawler => "crawler",
            RoleArg::Indexer => "indexer",
            RoleArg::Searcher => "searcher",
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
    /// 配置文件路径（YAML）。命令行参数优先级高于本文件。
    #[arg(long, default_value = "config.yaml")]
    config: String,

    /// 节点角色。`all` = 单机全角色；其余为集群独立角色。
    #[arg(long, value_enum, env = "NEKO_ROLE")]
    role: Option<RoleArg>,

    /// 注册中心 HTTP 监听地址（本节点承担 registry 角色时生效）。
    #[arg(long, env = "REGISTRY_ADDR")]
    registry_addr: Option<String>,

    /// 索引节点 HTTP 监听地址（本节点承担 indexer 角色时生效）。
    #[arg(long, env = "INDEXER_ADDR")]
    indexer_addr: Option<String>,

    /// 检索服务 HTTP 监听地址（本节点承担 searcher 角色时生效）。
    #[arg(long, env = "SEARCHER_ADDR")]
    searcher_addr: Option<String>,

    /// 远端注册中心基址（集群模式下 crawler/indexer/searcher 连接用）。
    #[arg(long, env = "REGISTRY_REMOTE")]
    registry_remote: Option<String>,

    /// 远端索引节点基址（集群模式下 crawler/searcher 写入/查询用）。
    #[arg(long, env = "INDEXER_REMOTE")]
    indexer_remote: Option<String>,

    /// 爬虫种子 URL，逗号分隔。
    #[arg(long, env = "SEEDS", value_delimiter = ',')]
    seeds: Option<Vec<String>>,

    /// 最大爬取深度（BFS）。
    #[arg(long, env = "MAX_DEPTH")]
    max_depth: Option<u32>,

    /// 持久化索引数据目录（sled）。
    #[arg(long, env = "DATA_DIR")]
    data_dir: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();

    // 加载 YAML 配置，命令行参数（及 clap 环境变量）覆盖配置文件。
    let mut cfg = config::Config::load(&cli.config).unwrap_or_default();
    if let Some(r) = cli.role {
        cfg.role = r.as_str().to_string();
    }
    if let Some(a) = cli.registry_addr {
        cfg.registry_addr = a;
    }
    if let Some(a) = cli.indexer_addr {
        cfg.indexer_addr = a;
    }
    if let Some(a) = cli.searcher_addr {
        cfg.searcher_addr = a;
    }
    if let Some(a) = cli.registry_remote {
        cfg.registry_remote = a;
    }
    if let Some(a) = cli.indexer_remote {
        cfg.indexer_remote = a;
    }
    if let Some(s) = cli.seeds {
        cfg.seeds = s;
    }
    if let Some(d) = cli.max_depth {
        cfg.max_depth = d;
    }
    if let Some(p) = cli.data_dir {
        cfg.data_dir = p;
    }

    let role = config::parse_role(&cfg.role)?;

    let run_registry = matches!(role, Role::All | Role::Registry);
    let run_indexer = matches!(role, Role::All | Role::Indexer);
    let run_crawler = matches!(role, Role::All | Role::Crawler);
    let run_searcher = matches!(role, Role::All | Role::Searcher);

    // 本节点若承担 registry 角色，则启动进程内实现；否则使用远端 HTTP 客户端。
    let inmem_registry = if run_registry {
        Some(InMemoryRegistry::new())
    } else {
        None
    };
    // 本节点若承担 indexer 角色，默认使用持久化（sled）实现，重启不丢索引；
    // 否则使用远端 HTTP 客户端。
    let sled_indexer = if run_indexer {
        Some(persistent_indexer::SledIndexer::open_or_create(&cfg.data_dir)?)
    } else {
        None
    };

    let registry: Arc<dyn Registry> = match &inmem_registry {
        Some(r) => Arc::new(r.clone()),
        None => Arc::new(registry_client::HttpRegistryClient::new(cfg.registry_remote.clone())),
    };
    let indexer: Arc<dyn Indexer> = match &sled_indexer {
        Some(i) => Arc::new(i.clone()),
        None => Arc::new(indexer_client::HttpIndexerClient::new(cfg.indexer_remote.clone())),
    };

    if let Some(reg) = &inmem_registry {
        let reg_clone = reg.clone();
        let addr = cfg.registry_addr.clone();
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

    if let Some(idx) = &sled_indexer {
        let idx_clone = idx.clone();
        let addr = cfg.indexer_addr.clone();
        tokio::spawn(async move {
            if let Err(e) = indexer::serve(&addr, Arc::new(idx_clone)).await {
                tracing::error!("indexer server stopped: {e}");
            }
        });
    }

    if run_searcher {
        let idx = indexer.clone();
        let addr = cfg.searcher_addr.clone();
        tokio::spawn(async move {
            if let Err(e) = searcher::serve(&addr, idx).await {
                tracing::error!("searcher server stopped: {e}");
            }
        });
    }

    if run_crawler {
        let seeds: Vec<String> = cfg.seeds.iter().filter(|s| !s.is_empty()).cloned().collect();
        let manager = Arc::new(crawler::manager::CrawlerManager::new(
            registry.clone(),
            indexer.clone(),
            seeds,
            cfg.max_depth,
        ));
        tokio::spawn(manager.run());
    }

    tracing::info!(
        role = ?role,
        data_dir = %cfg.data_dir,
        "nekosearch started (single-node default; cluster roles enabled by --role)"
    );

    // 等待退出信号。
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    Ok(())
}
