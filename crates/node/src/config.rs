//! nekosearch 配置（YAML）。
//!
//! 配置以 `config.yaml` 为主，命令行参数（以及 `clap` 兼容的环境变量）优先级高于配置文件。
//! 解析失败或文件缺失时回退到 [`Config::default`]。

use anyhow::Context;
use nekosearch_core::Role;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 全部可配置项。字段缺失时使用 [`Config::default`] 的对应值。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// 节点角色：`all`(默认, 单机全角色) / registry / crawler / indexer / searcher
    pub role: String,
    /// 注册中心 HTTP 监听地址（本节点承担 registry 角色时生效）。
    pub registry_addr: String,
    /// 索引节点 HTTP 监听地址（本节点承担 indexer 角色时生效）。
    pub indexer_addr: String,
    /// 检索服务 HTTP 监听地址（本节点承担 searcher 角色时生效）。
    pub searcher_addr: String,
    /// 远端注册中心基址（集群模式下 crawler/indexer/searcher 连接用）。
    pub registry_remote: String,
    /// 远端索引节点基址（集群模式下 crawler/searcher 写入/查询用）。
    pub indexer_remote: String,
    /// 爬虫种子 URL 列表。
    pub seeds: Vec<String>,
    /// 最大爬取深度（BFS）。
    pub max_depth: u32,
    /// 持久化索引数据目录（sled）。
    pub data_dir: String,
    /// 启动时写入内置演示文档到索引，便于一键验证搜索（不依赖外网爬取）。
    pub seed_demo: bool,
    /// 多注册中心高可用：对端注册中心基址列表（http://host:port）。为空则为单机注册中心。
    pub peers: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            role: "all".to_string(),
            registry_addr: "0.0.0.0:7700".to_string(),
            indexer_addr: "0.0.0.0:7900".to_string(),
            searcher_addr: "0.0.0.0:7800".to_string(),
            registry_remote: "http://127.0.0.1:7700".to_string(),
            indexer_remote: "http://127.0.0.1:7900".to_string(),
            seeds: Vec::new(),
            max_depth: 2,
            data_dir: "./data".to_string(),
            seed_demo: false,
            peers: Vec::new(),
        }
    }
}

impl Config {
    /// 加载配置文件；文件不存在或为空时返回 [`Config::default`]。
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let p = Path::new(path);
        if !p.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(p)
            .with_context(|| format!("读取配置文件失败: {path}"))?;
        let text = raw.trim();
        if text.is_empty() {
            return Ok(Self::default());
        }
        let cfg: Config = serde_yaml::from_str(text)
            .with_context(|| format!("解析配置文件失败（应为合法 YAML）: {path}"))?;
        Ok(cfg)
    }
}

/// 将配置中的角色字符串解析为协议 [`Role`]。
pub fn parse_role(s: &str) -> anyhow::Result<Role> {
    match s.trim().to_lowercase().as_str() {
        "all" => Ok(Role::All),
        "registry" => Ok(Role::Registry),
        "crawler" => Ok(Role::Crawler),
        "indexer" => Ok(Role::Indexer),
        "searcher" => Ok(Role::Searcher),
        other => anyhow::bail!(
            "未知角色: {other}（应为 all/registry/crawler/indexer/searcher）"
        ),
    }
}
