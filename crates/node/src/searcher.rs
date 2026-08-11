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

/// 启动检索服务 HTTP 服务。
pub async fn serve(addr: &str, indexer: Arc<dyn Indexer>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(home))
        .route("/search", get(search))
        .route("/suggest", get(suggest))
        .route("/docs", post(add_doc))
        .route("/robots.txt", get(robots))
        .with_state(indexer);

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
    Json(indexer.suggest(&params.q, params.limit.unwrap_or(10)).await.unwrap_or_default())
}

/// 写入文档：对外端口 7800 也能直接索引内容（与索引节点 7900 的 `/docs` 行为一致）。
async fn add_doc(
    State(indexer): State<Arc<dyn Indexer>>,
    Json(doc): Json<Doc>,
) -> StatusCode {
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
    (
        h,
        "User-agent: *\nAllow: /\n",
    )
}
