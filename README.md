# nekosearch

> 对标 Google 的自建搜索服务器。**默认单机部署，架构天生集群，注册中心统一管理爬虫执行器，支持水平扩容。**

用 Rust 编写，单二进制 `nekosearch` 通过 `--role` 切换形态：不传参数即为**单机全角色**，一个进程跑完抓取→索引→检索；也可拆分为 `registry` / `crawler` / `indexer` / `searcher` 多个进程组成集群。注册中心内置、零外部服务依赖。

## 一分钟跑起来（傻瓜式）

```bash
# Linux / macOS
./deploy.sh

# Windows (PowerShell)
.\deploy.ps1
```

`deploy.sh` / `deploy.ps1` 会自动检测：有 docker 就 `docker compose up -d --build`，没有就用 `cargo run --release`（需本机装了 Rust）。

起来后检索：

```bash
curl "http://localhost:7800/search?q=rust&top_k=10"
```

查看注册中心里的节点：

```bash
curl "http://localhost:7700/nodes"
```

### 网页界面（类 Google）
起好后直接用浏览器打开检索服务的地址即可使用类 Google 的搜索页：

```
http://localhost:7800/
```

首页是居中的搜索框，输入关键词回车后进入结果页（标题链接 + 绿色 URL + 摘要查询词高亮），纯前端调用 `/search` JSON 接口渲染，UI 随二进制内嵌、无需额外部署静态文件。对外提供搜索时，把这一个端口（默认 7800）用反向代理暴露出去即可。

## 手动方式

### 用 Docker
```bash
docker compose up -d --build
```

### 用 Cargo（单机全角色）
```bash
cargo run -- --role all --seeds https://www.rust-lang.org/ --max-depth 2
```

### 集群模式（拆角色，水平扩容爬虫）
开多个终端，分别跑：
```bash
nekosearch --role registry                         # 注册中心 :7700
nekosearch --role indexer                          # 索引节点 :7900
nekosearch --role searcher                         # 检索服务 :7800
nekosearch --role crawler --seeds https://example.com/   # 爬虫，可开 N 个
```
新增 `crawler` 进程即可直接提升抓取吞吐——注册中心会自动发现并分配任务。

## 配置项（YAML / CLI / 环境变量）
配置以 `config.yaml` 为主（参考 `config.yaml.example`）。CLI 参数与兼容的环境变量优先级更高。常用项：
- `role` / `--role` / `NEKO_ROLE`：角色，`all`(默认)/registry/crawler/indexer/searcher
- `registry_addr` / `indexer_addr` / `searcher_addr`：本节点各服务监听地址
- `registry_remote` / `indexer_remote`：集群模式下连接远端注册中心/索引的基址（`indexer_remote` 支持分片与多副本，格式见 `config.yaml.example`）
- `seeds` / `--seeds` / `SEEDS`：种子 URL（YAML 用列表，CLI/环境变量逗号分隔）
- `max_depth` / `MAX_DEPTH`：最大爬取深度
- `data_dir` / `DATA_DIR`：持久化索引目录（sled），默认 `./data`

## 架构一览
```
种子URL → 注册中心(入队) → 爬虫领取 → 抓取 → 写索引 → 外链回灌(BFS) → 检索服务查询索引
```
- **注册中心**：节点发现 + 抓取任务调度，失联自动剔除。
- **爬虫执行器**：实现 `CrawlerExecutor` trait；当前内置 http / fs 两个示例，新增数据源只需加一个实现。
- **索引**：sled 持久化倒排索引 + BM25 评分 + jieba 中文分词，单机/集群共用 `Indexer` trait；集群下 `indexer_remote` 可配置分片与多副本。

详见 [PLAN.md](./PLAN.md)（架构与路线图）与 [AGENTS.md](./AGENTS.md)（开发规范）。

## 开发
```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```
> 注意：测试/CI 基线只保留**单机全角色（--role all）**这一种形态（见 AGENTS.md §7）。

## 许可证
TODO（默认拟采用 MIT/Apache-2.0，待定）。
