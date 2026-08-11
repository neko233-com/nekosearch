//! 演示数据集：开启 `--seed-demo` 时写入索引，便于一键验证搜索是否可用。
//!
//! 无需外网爬取即可让 `/search` 返回结果，覆盖中英文关键词，方便验证分词与 BM25 排序。

use nekosearch_core::Doc;

/// 返回一组内置演示文档（id 稳定，重复写入会原地更新，不会重复堆积）。
pub fn demo_docs() -> Vec<Doc> {
    vec![
        Doc {
            id: "demo-nekosearch".into(),
            url: "https://github.com/neko233-com/nekosearch".into(),
            title: "nekosearch · 自托管搜索引擎".into(),
            body: "nekosearch 是一个对标 Google 的自建搜索服务器，默认单机部署，架构天生支持集群与水平扩容。由 @neko233 开发，使用 Rust 编写。".into(),
        },
        Doc {
            id: "demo-rust".into(),
            url: "https://www.rust-lang.org/".into(),
            title: "Rust 编程语言".into(),
            body: "Rust 是一门系统级编程语言，注重内存安全与并发安全，无垃圾回收，性能可与 C++ 媲美。nekosearch 正是用 Rust 实现的搜索引擎。".into(),
        },
        Doc {
            id: "demo-bm25".into(),
            url: "https://en.wikipedia.org/wiki/Okapi_BM25".into(),
            title: "BM25 排序算法".into(),
            body: "BM25 是搜索引擎中常用的相关度排序函数，基于概率检索模型。它考虑了词频、逆文档频率和文档长度归一化。nekosearch 使用 BM25（k1=1.5, b=0.75）对结果打分。".into(),
        },
        Doc {
            id: "demo-jieba".into(),
            url: "https://github.com/messense/jieba-rs".into(),
            title: "jieba 中文分词".into(),
            body: "jieba-rs 是 Rust 实现的中文分词库，内置词典、零外部依赖。nekosearch 用它做中文词级切分，让中文搜索也能正确命中。".into(),
        },
        Doc {
            id: "demo-cluster".into(),
            url: "https://github.com/neko233-com/nekosearch#cluster".into(),
            title: "nekosearch 的集群与分片".into(),
            body: "nekosearch 默认单机，但架构从第一天起就是集群。索引按 doc.id 做 FNV-1a 哈希分片，支持多副本，爬虫执行器可水平扩容，由注册中心统一管理。".into(),
        },
        Doc {
            id: "demo-crawler".into(),
            url: "https://github.com/neko233-com/nekosearch#crawler".into(),
            title: "爬虫执行器".into(),
            body: "nekosearch 的爬虫执行器向注册中心注册，持续领取抓取任务，抓取网页后写入索引，并把外链回灌为新任务做 BFS 扩散。".into(),
        },
        Doc {
            id: "demo-registry".into(),
            url: "https://github.com/neko233-com/nekosearch#registry".into(),
            title: "注册中心".into(),
            body: "注册中心（registry）管理各个爬虫执行器和节点，是集群的协调者。多注册中心模式下通过 leader 选举实现高可用。".into(),
        },
        Doc {
            id: "demo-suggest".into(),
            url: "https://github.com/neko233-com/nekosearch#suggest".into(),
            title: "搜索建议 suggest".into(),
            body: "nekosearch 支持查询词自动补全，输入前缀即可从索引词表中返回候选词，帮助快速完成搜索。".into(),
        },
        Doc {
            id: "demo-deploy".into(),
            url: "https://github.com/neko233-com/nekosearch#deploy".into(),
            title: "傻瓜式部署".into(),
            body: "nekosearch 提供 deploy.sh 与 deploy.ps1 一键安装脚本，配置使用 YAML，默认单机即可运行，也可通过 Docker 部署。".into(),
        },
        Doc {
            id: "demo-selfhost".into(),
            url: "https://github.com/neko233-com/nekosearch#self-hosted".into(),
            title: "自托管搜索".into(),
            body: "把搜索掌握在自己手里：nekosearch 让你在自己的服务器上运行一个私有的、可集群的搜索引擎，数据完全自控。".into(),
        },
    ]
}
