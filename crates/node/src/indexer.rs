//! 索引节点 HTTP 服务（REST）。
//!
//! 仅在节点承担 indexer 角色时启动，底层由 `InMemoryIndexer` 支撑。
//! 集群中爬虫通过 `HttpIndexerClient` 写入文档，检索服务通过它查询。

use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use nekosearch_core::{
    indexer::{InMemoryIndexer, Indexer},
    Doc, SearchQuery, SearchResult,
};

/// 启动索引节点 HTTP 服务。
pub async fn serve(addr: &str, idx: InMemoryIndexer) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/docs", post(add_doc))
        .route("/search", post(search))
        .with_state(idx);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("indexer http listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn add_doc(State(idx): State<InMemoryIndexer>, Json(doc): Json<Doc>) -> axum::http::StatusCode {
    match idx.add(doc).await {
        Ok(()) => axum::http::StatusCode::OK,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn search(
    State(idx): State<InMemoryIndexer>,
    Json(q): Json<SearchQuery>,
) -> Json<Vec<SearchResult>> {
    Json(idx.search(&q).await.unwrap_or_default())
}
