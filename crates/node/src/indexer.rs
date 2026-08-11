//! 索引节点 HTTP 服务（REST）。
//!
//! 仅在节点承担 indexer 角色时启动。底层由任意 [`Indexer`] 实现支撑：
//! 单机默认是 `SledIndexer`（持久化），集群中爬虫通过 `HttpIndexerClient` 写入远端索引节点。
//! 这样上层（爬虫、检索）只依赖 `Arc<dyn Indexer>`，与具体实现解耦。

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use nekosearch_core::{
    indexer::Indexer,
    Doc, SearchQuery, SearchResult,
};
use std::sync::Arc;

/// 启动索引节点 HTTP 服务。
pub async fn serve(addr: &str, idx: Arc<dyn Indexer>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/docs", post(add_doc))
        .route("/search", post(search))
        .route("/suggest", get(suggest))
        .with_state(idx);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("indexer http listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn add_doc(
    State(idx): State<Arc<dyn Indexer>>,
    Json(doc): Json<Doc>,
) -> axum::http::StatusCode {
    match idx.add(doc).await {
        Ok(()) => axum::http::StatusCode::OK,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn search(
    State(idx): State<Arc<dyn Indexer>>,
    Json(q): Json<SearchQuery>,
) -> Json<Vec<SearchResult>> {
    Json(idx.search(&q).await.unwrap_or_default())
}

#[derive(serde::Deserialize)]
struct SuggestParams {
    q: String,
    limit: Option<usize>,
}

/// JSON 查询词自动补全接口。
async fn suggest(
    State(idx): State<Arc<dyn Indexer>>,
    Query(params): Query<SuggestParams>,
) -> Json<Vec<String>> {
    Json(idx.suggest(&params.q, params.limit.unwrap_or(10)).await.unwrap_or_default())
}
