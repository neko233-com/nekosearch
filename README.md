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
curl "http://localhost:7512/search?q=rust&top_k=10"
```

> **单机开箱即搜**：`--role all` 且未配置任何爬取种子时（默认情况），启动会自动写入一组**各编程语言官方网站**的内置演示数据（Rust / Go / Python / JS / TS / Java / C++ / Kotlin / Swift / Ruby / PHP / Node.js …），无需外网即可直接搜索：
>
> ```bash
> cargo run -- --role all
> curl "http://localhost:7512/search?q=rust&top_k=5"      # 返回 rust-lang.org
> curl "http://localhost:7512/search?q=python&top_k=5"    # 返回 python.org
> ```
>
> 想显式控制，可加 `--seed-demo`（强制写入）或配置 `seeds`（去爬真实网页，不再注入演示数据）。
>
> 一键验证（构建 → 起服务 → 查询多语言官网 → 断言命中 → 校验自动补全）：
> `bash scripts/verify.sh` ／ `.\scripts\verify.ps1`
>
> 验证脚本会断言 `rust→rust-lang.org`、`go→go.dev`、`python→python.org`、`java→java.com`、`c++→cppreference.com`，并校验 `/suggest` 自动补全。

也可直接往对外端口写入内容（索引节点 7511 的 `/docs` 行为一致）：

```bash
curl -X POST "http://localhost:7512/docs" \
  -H 'content-type: application/json' \
  -d '{"id":"doc1","url":"https://example.com","title":"示例标题","body":"示例正文，包含要被检索的关键词"}'
```

查询词自动补全（输入时下拉候选）：

```bash
curl "http://localhost:7512/suggest?q=ne&limit=8"
```

查看注册中心里的节点：

```bash
curl "http://localhost:7510/nodes"
```

### 网页界面
起好后直接用浏览器打开检索服务的地址即可使用搜索页：

```
http://localhost:7512/
```

首页是带 `@neko233` 品牌与 GitHub 入口的暗色开发者风格搜索页，下方有一排编程语言快捷搜索（Rust / Go / Python …）。输入关键词回车后进入结果页（标题链接 + 绿色 URL + 摘要查询词高亮），纯前端调用 `/search` JSON 接口渲染，UI 随二进制内嵌、无需额外部署静态文件。对外提供搜索时，把这一个端口（默认 7512）用反向代理暴露出去即可。

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
nekosearch --role registry                         # 注册中心 :7510
nekosearch --role indexer                          # 索引节点 :7511
nekosearch --role searcher                         # 检索服务 :7512
nekosearch --role crawler --seeds https://example.com/   # 爬虫，可开 N 个
```
新增 `crawler` 进程即可直接提升抓取吞吐——注册中心会自动发现并分配任务。

### 多注册中心高可用（Phase 5）
注册中心支持多实例部署，消除单点：
- 每个注册中心通过 `peers`（或 `--peers`）知道对端地址，互相每秒心跳（`GET /ping`）。
- 选主规则：`{本节点} ∪ {在线对端}` 中**字典序最小 id** 为 leader（确定性、无外部协调服务）。
- 非 leader 注册中心对写请求（register/heartbeat/deregister/nodes/tasks）返回 **307 重定向**到 leader；`HttpRegistryClient` 默认跟随重定向，调用方无感。
- crawler/indexer/searcher 的 `registry_remote` 填**多个**注册中心地址（逗号分隔），任一宕机自动故障转移到其它实例。

```bash
# 注册中心 A（leader 候选）
nekosearch --role registry --registry-addr 0.0.0.0:7510 \
  --peers http://127.0.0.1:7513
# 注册中心 B
nekosearch --role registry --registry-addr 0.0.0.0:7513 \
  --peers http://127.0.0.1:7510
# 其它角色指向两个注册中心，任一宕机自动切换
nekosearch --role crawler --registry-remote "http://127.0.0.1:7510,http://127.0.0.1:7513" --seeds https://example.com/
```

## 配置项（YAML / CLI / 环境变量）
配置以 `config.yaml` 为主（参考 `config.yaml.example`）。CLI 参数与兼容的环境变量优先级更高。常用项：
- `role` / `--role` / `NEKO_ROLE`：角色，`all`(默认)/registry/crawler/indexer/searcher
- `registry_addr` / `indexer_addr` / `searcher_addr`：本节点各服务监听地址
- `registry_remote` / `indexer_remote`：集群模式下连接远端注册中心/索引的基址（`indexer_remote` 支持分片与多副本，格式见 `config.yaml.example`）
- `seeds` / `--seeds` / `SEEDS`：种子 URL（YAML 用列表，CLI/环境变量逗号分隔）
- `max_depth` / `MAX_DEPTH`：最大爬取深度
- `data_dir` / `DATA_DIR`：持久化索引目录（sled），默认 `./data`
- `seed_demo` / `--seed-demo` / `NEKO_SEED_DEMO`：启动时写入内置演示文档，便于一键验证搜索（无需外网爬取）
- `peers` / `--peers` / `PEERS`：多注册中心高可用时填写对端注册中心基址（逗号分隔），为空即单机注册中心

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
