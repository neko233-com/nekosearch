# AGENTS.md — nekosearch 开发规范

> 面向参与本仓库开发的智能体与人类协作者的硬性约定。改代码前先读这一份。

## 1. 项目定位
nekosearch 是一个**对标 Google 的自建搜索服务器**。设计上**默认单机部署**（`--role all`，一个进程承载全部角色），但**架构从第一天起就是集群**：各角色可拆分为独立进程，由内置注册中心统一发现与调度，支持水平扩容。

## 2. 技术栈与工具链
- 语言：**Rust（stable，edition 2021）**，禁止引入 nightly-only 特性。
- 异步：tokio + axum（HTTP）。
- 序列化：serde / serde_json。
- CLI：clap；配置以 **YAML** 为主（见 `config.yaml.example`），clap 兼容环境变量作为兜底。
- 外部服务依赖：**默认零依赖**。单机模式不依赖 etcd / Consul / NATS 等任何外部服务；集群模式也仅依赖本进程内的注册中心 HTTP 服务。新增外部依赖须在本文件登记并经评审。
- 中文分词：引入 `jieba-rs`（纯 Rust、内置词典、零外部依赖），属允许的库依赖，不破坏「零外部服务」红线。
- 本地需安装 Rust 工具链（`rustup`）。本仓库含 `rust-toolchain.toml` 固定 stable。

## 3. 目录结构
```
crates/core   # 共享内核：协议/数据结构、Registry trait、Indexer trait、错误类型
crates/node   # 单二进制 nekosearch：注册中心HTTP、索引HTTP、检索HTTP、爬虫执行器与管理器、CLI、YAML配置、持久化索引
crates/node/static  # 内嵌的检索网页（index.html），由 searcher 通过 include_str! 编译期打包，单二进制即可对外提供 @neko233 品牌暗色风 UI
crates/node/demo.rs  # --seed-demo 启动参数写入的内置演示文档，便于一键验证搜索
```

## 4. 架构红线（不可违反）
1. **注册中心优先（registry-first）**：任何节点发现、任务派发都经过 `Registry` trait，不得硬编码对端地址或端口。
2. **无硬编码节点**：爬虫/检索/索引在集群模式下通过注册中心或 `--*-remote` 配置互相连接，禁止在代码里写死其它节点 IP。
3. **爬虫执行器必须实现 `CrawlerExecutor` trait**：新增数据源（S3、DB、RSS 等）只新增一个实现并在 `CrawlerManager` 注册，不改动调度逻辑。
4. **抽象统一**：上层只依赖 `Arc<dyn Registry>` 与 `Arc<dyn Indexer>`。单机 = 内存实现，集群 = HTTP 客户端，二者对上层透明。
5. **默认单机**：不传 `--role` 时为 `all`（单机全角色）。集群化是显式选择。

## 5. 提交 / 分支 / PR
- 主分支 `main`，禁止直接 push 到 `main`；功能走 `feat/xxx`、修复走 `fix/xxx`，PR 合并。
- 提交信息风格（conventional commits）：`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`。
- 示例：`feat(crawler): add rss executor implementing CrawlerExecutor`。

## 6. 代码质量门槛（合并前必须过）
- `cargo fmt --all -- --check` 通过。
- `cargo clippy --all-targets -- -D warnings` 零警告。
- `cargo build` 通过；`cargo test` 全绿。
- 新增公开 API 必须配套文档注释（`///`）。

## 7. 测试基线（重要）
**测试 / CI 只保留「单机全角色（--role all）」这一种部署形态作为唯一基线。**
集群多进程是生产扩展路径，不在常规测试矩阵内；集群相关的验证以手动/集成脚本为主，不得让集群形态进入必须通过的单元测试集合。即「测试必须只保留一个」= 只保留单机这一条基线。

## 8. 本地运行 / 调试
```bash
# 单机全角色（默认）
cargo run -- --role all --seeds https://www.rust-lang.org/

# 一键验证搜索是否可用：--seed-demo 写入内置演示文档，无需外网
cargo run -- --role all --seed-demo
curl "http://localhost:7512/search?q=nekosearch&top_k=5"

# 检索（JSON API）
curl "http://localhost:7512/search?q=rust&top_k=10"

# 写入内容（对外端口 7512 与索引端口 7511 的 /docs 一致）
curl -X POST "http://localhost:7512/docs" -H 'content-type: application/json' \
  -d '{"id":"d1","url":"https://example.com","title":"标题","body":"正文关键词"}'

# 查询词自动补全
curl "http://localhost:7512/suggest?q=ne&limit=8"

# 网页界面（@neko233 品牌暗色风，浏览器打开）
#   http://localhost:7512/

# 查看注册中心节点
curl "http://localhost:7510/nodes"
```

多注册中心高可用（Phase 5）：注册中心实例通过 `--peers` 互知对端，心跳选主（字典序最小 id 为 leader），非 leader 对写请求 307 重定向到 leader；集群角色的 `registry_remote` 填多个注册中心地址（逗号分隔）即具备故障转移。详见 `README.md` 集群模式章节。

## 9. 新增一个爬虫执行器（扩展点）
1. 在 `crates/node/src/crawler/` 新建 `xxx_crawler.rs`，实现 `CrawlerExecutor` trait。
2. 在 `crawler/mod.rs` 声明模块，在 `crawler/manager.rs` 的 `run` 中按 `task.url` 模式选择并 `Box`/引用该执行器。
3. 跑通单机基线后提 PR，附该数据源的冒烟说明。

## 10. 傻瓜式部署
见 `README.md` 与仓库根目录的 `deploy.sh`（Linux/macOS）/ `deploy.ps1`（Windows PowerShell）/ `docker-compose.yml`：一行 `./deploy.sh` 或 `.\deploy.ps1` 或 `docker compose up -d` 即可起一个单机搜索引擎。
