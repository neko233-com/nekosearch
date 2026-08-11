# PLAN.md — nekosearch 架构与计划

> 对标 Google 的自建搜索服务器。**默认单机，架构集群，注册中心管理爬虫执行器，支持水平扩容。**

## 1. 目标
提供一个可私有部署、可水平扩展的搜索引擎：
- 开箱即用：一条命令起一个完整可搜的搜索引擎（**傻瓜式部署**）。
- 集群就绪：爬虫、索引、检索均可拆分为独立进程，由注册中心统一调度。
- 易扩展：新增数据源只需实现 `CrawlerExecutor`。

## 2. 总体架构

```
                        ┌──────────────────────────────┐
   种子URL ───────────▶ │   注册中心 Registry (发现+调度) │◀── 心跳/注册
                        └───────────────┬───────────────┘
                                        │ 派发 CrawlTask
                  ┌─────────────────────┼─────────────────────┐
                  ▼                     ▼                     ▼
           Crawler 执行器群      Indexer 索引节点群      Searcher 检索服务
        (http/fs/自定义…)        (倒排索引, 可多副本)     (对外 query API)
                  │                     │
                  │  写入 Doc           │  查询
                  └─────────► Index ◀───┘
```

- **单机（默认）**：一个 `nekosearch --role all` 进程，注册中心/索引/检索/爬虫全部进程内共享内存，零外部依赖。
- **集群**：各角色独立进程，注册中心通过 HTTP 暴露，其它角色以 `HttpRegistryClient` / `HttpIndexerClient` 接入。

## 3. 角色与数据流
| 角色 | 职责 | 默认端口 |
|------|------|----------|
| registry | 节点注册/心跳/注销、抓取任务队列 | 7700 |
| crawler  | 向注册中心领任务，执行抓取，写索引，回灌外链 | — |
| indexer  | 维护倒排索引，提供写入/查询 | 7900 |
| searcher | 对外检索 API（GET /search） | 7800 |
| all      | 上述全部（单机默认） | 7700/7800/7900 |

数据流：种子 URL → 注册中心入队 → 爬虫领取 → 抓取 → 写索引 → 外链回灌为新任务（BFS）→ 检索服务查询索引。

## 4. 注册中心机制（管理爬虫执行器）
注册中心是集群的「大脑」，通过 `Registry` trait 抽象：
- `register` / `heartbeat` / `deregister`：节点生命周期与存活探测（失联 10s 自动剔除）。
- `submit_task` / `claim_task`：抓取任务队列，爬虫按能力领取。
- **水平扩容爬虫**：新增 `crawler` 进程即可直接提升抓取吞吐，无需改动其它组件——注册中心自动感知新节点并分配任务。

## 5. 单机默认 vs 集群
| 维度 | 单机（默认） | 集群 |
|------|--------------|------|
| 启动 | `nekosearch`（或 `--role all`） | 多个进程分别 `--role registry/crawler/indexer/searcher` |
| 节点发现 | 进程内共享 | 注册中心 HTTP |
| 外部依赖 | 无 | 无（注册中心自带） |
| 扩容方式 | — | 加 crawler / indexer / searcher 进程 |

两种方式**共用同一套代码**，仅由 `main` 按形态装配 `Arc<dyn Registry>` / `Arc<dyn Indexer>`，上层逻辑零分支。

## 6. 技术选型
- Rust stable + tokio + axum（HTTP 服务）。
- 注册中心 / 索引：进程内内存实现，零外部服务（etcd/Consul/NATS 不在默认链路）。
- 检索评分：BM25（标准 k1/b）+ jieba 中文分词；生产可替换为 Tantivy / Meilisearch 等。
- 部署：Docker 多阶段构建 + docker-compose 一键起；`deploy.sh`（Linux/macOS）与 `deploy.ps1`（Windows）自动识别 docker/cargo。
- 配置：YAML（`config.yaml`，参考 `config.yaml.example`），CLI/环境变量可覆盖。
- 持久化：默认单机使用 sled（纯 Rust 嵌入式 KV，零外部依赖）保存倒排索引，重启不丢数据。

## 7. 傻瓜式部署
- `./deploy.sh`（Linux/macOS）/ `.\deploy.ps1`（Windows）：检测到 docker 则 `docker compose up -d --build`，否则 `cargo run --release`（单机 all）。
- `docker-compose.yml`：单服务、三端口映射、`restart: unless-stopped`，挂载 `nekosearch-data` 卷持久化索引。
- `config.yaml.example`：所有可配项（角色、地址、种子、深度、data_dir），YAML 格式，经 `config.yaml` 加载。
- 起好后：`curl "http://localhost:7800/search?q=关键词"` 即可检索。

## 8. 里程碑 / 路线图
- [x] **阶段 0（当前骨架）**：Cargo workspace、注册中心 trait+内存实现+HTTP、索引 trait+内存实现+HTTP、检索 API、爬虫执行器 trait + http/fs 示例、CLI 多角色、傻瓜式部署、AGENTS/PLAN。
- [x] **阶段 1**：持久化索引（sled 嵌入式 KV，零外部依赖），进程重启不丢数据；并接入 YAML 配置、新增 `deploy.ps1` 一键安装。
- [x] **阶段 2**：中文分词（jieba，纯 Rust 内置词典）接入 `tokenize`，评分升级为 BM25（k1=1.5/b=0.75，跟踪文档长度）。
- [x] **阶段 3**：索引分片与多副本——新增 `ShardedIndexer`，按 `doc.id` 稳定哈希分片、分片内多副本写入、检索跨分片合并；`indexer_remote` 支持 `分片A|副本B,分片C` 格式。注册中心仍统一管理节点生命周期。
- [ ] **阶段 4**：检索前端 UI、查询词建议、站点地图/robots 合规抓取。
- [ ] **阶段 5**：多注册中心高可用（leader 选举），消除单点。

## 9. 当前骨架状态与约定
- 代码已按可编译标准编写，但本环境无 Rust 工具链，**未执行 `cargo build`**。请在本机运行 `cargo build` 验证。
- **「测试必须只保留一个」约束**：仓库测试/CI 仅以单机全角色（`--role all`）为唯一基线；集群形态为生产扩展，不进入必须通过的单测矩阵（详见 AGENTS.md §7）。
- 新增爬虫数据源见 AGENTS.md §9。
