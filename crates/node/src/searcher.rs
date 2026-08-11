//! 检索服务 HTTP 服务（对外网页 + JSON API）。
//!
//! - `GET /`            搜索网页（首页 + 结果页，纯前端调用 `/search`）。
//! - `GET /search`      JSON 检索接口：`?q=<关键词>&top_k=<n>`，返回 `Vec<SearchResult>`。
//! - `GET /robots.txt`  爬虫合规声明。
//!
//! 底层依赖 `Arc<dyn Indexer>`：单机模式下指向进程内 `InMemoryIndexer`/`SledIndexer`，
//! 集群模式下指向远端索引节点的 `ShardedIndexer`。

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use nekosearch_core::{indexer::Indexer, Doc, SearchQuery, SearchResult};
use std::sync::Arc;

/// 内嵌的搜索网页（编译期包含，单二进制即可对外提供 UI，无需额外部署静态文件）。
const INDEX_HTML: &str = include_str!("../static/index.html");

/// 构建检索服务路由（抽出以便测试在临时端口起服务）。
pub(crate) fn router(indexer: Arc<dyn Indexer>) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/search", get(search))
        .route("/suggest", get(suggest))
        .route("/docs", post(add_doc))
        .route("/robots.txt", get(robots))
        .with_state(indexer)
}

/// 启动检索服务 HTTP 服务。
pub async fn serve(addr: &str, indexer: Arc<dyn Indexer>) -> anyhow::Result<()> {
    let app = router(indexer);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("searcher http listening on {addr} (web UI at /)");
    axum::serve(listener, app).await?;
    Ok(())
}

/// 搜索网页首页。
async fn home() -> Html<&'static str> {
    Html(INDEX_HTML)
}

#[derive(serde::Deserialize)]
struct SearchParams {
    q: String,
    top_k: Option<usize>,
}

#[derive(serde::Deserialize)]
struct SuggestParams {
    q: String,
    limit: Option<usize>,
}

/// JSON 检索接口。
async fn search(
    State(indexer): State<Arc<dyn Indexer>>,
    Query(params): Query<SearchParams>,
) -> Json<Vec<SearchResult>> {
    let q = SearchQuery {
        q: params.q.clone(),
        top_k: params.top_k.unwrap_or(10),
    };
    Json(indexer.search(&q).await.unwrap_or_default())
}

/// JSON 查询词自动补全接口。
async fn suggest(
    State(indexer): State<Arc<dyn Indexer>>,
    Query(params): Query<SuggestParams>,
) -> Json<Vec<String>> {
    Json(
        indexer
            .suggest(&params.q, params.limit.unwrap_or(10))
            .await
            .unwrap_or_default(),
    )
}

/// 写入文档：对外端口 7512 也能直接索引内容（与索引节点 7511 的 `/docs` 行为一致）。
async fn add_doc(State(indexer): State<Arc<dyn Indexer>>, Json(doc): Json<Doc>) -> StatusCode {
    match indexer.add(doc).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// 爬虫合规声明。允许全部抓取，遵循 robots 协议。
async fn robots() -> impl IntoResponse {
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    (h, "User-agent: *\nAllow: /\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekosearch_core::indexer::InMemoryIndexer;
    use std::sync::Arc;
    use tokio::net::TcpListener;

    /// 在临时端口起一个检索服务，写入全部演示文档，返回 base URL 与任务句柄。
    async fn spawn_searcher() -> (String, Vec<tokio::task::JoinHandle<()>>) {
        let idx = Arc::new(InMemoryIndexer::new());
        for d in crate::demo::demo_docs() {
            idx.add(d).await.unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, router(idx)).await.unwrap();
        });
        (format!("http://{addr}"), vec![handle])
    }

    #[tokio::test]
    async fn search_returns_language_official_sites() {
        let (base, _h) = spawn_searcher().await;
        let client = reqwest::Client::new();
        let cases = [
            ("rust", "rust-lang.org"),
            ("go", "go.dev"),
            ("python", "python.org"),
            ("java", "java.com"),
            ("cppreference", "cppreference.com"),
            ("kotlin", "kotlinlang.org"),
            ("swift", "swift.org"),
            ("node", "nodejs.org"),
        ];
        for (q, expect) in cases {
            let resp: Vec<SearchResult> = client
                .get(format!("{base}/search"))
                .query(&[("q", q), ("top_k", "5")])
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert!(!resp.is_empty(), "query '{q}' 返回空结果");
            let urls: Vec<&str> = resp.iter().map(|r| r.doc.url.as_str()).collect();
            assert!(
                urls.iter().any(|u| u.contains(expect)),
                "query '{q}' 命中 {urls:?} 不含期望域名 {expect}"
            );
        }
    }

    #[tokio::test]
    async fn suggest_returns_prefix_candidates() {
        let (base, _h) = spawn_searcher().await;
        let client = reqwest::Client::new();
        let sug: Vec<String> = client
            .get(format!("{base}/suggest"))
            .query(&[("q", "ru"), ("limit", "8")])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(
            sug.iter().any(|s| s == "rust"),
            "suggest 'ru' 应含 'rust'，实际 {sug:?}"
        );
    }

    #[tokio::test]
    async fn post_docs_then_searchable() {
        let idx = Arc::new(InMemoryIndexer::new());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _h = tokio::spawn(async move {
            axum::serve(listener, router(idx)).await.unwrap();
        });
        let base = format!("http://{addr}");
        let client = reqwest::Client::new();
        let doc = Doc {
            id: "example".into(),
            url: "https://example.com".into(),
            title: "Example Domain".into(),
            body: "example site used for testing search indexing".into(),
        };
        let status = client
            .post(format!("{base}/docs"))
            .json(&doc)
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, reqwest::StatusCode::OK);
        let resp: Vec<SearchResult> = client
            .get(format!("{base}/search"))
            .query(&[("q", "example"), ("top_k", "5")])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(
            resp.iter().any(|r| r.doc.id == "example"),
            "POST /docs 写入的文档应可被搜索到"
        );
    }
}
