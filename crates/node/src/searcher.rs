//! 检索服务 HTTP 服务。
//!
//! 暴露 `GET /search?q=<关键词>&top_k=<n>`，底层依赖 `Arc<dyn Indexer>`：
//! 单机模式下指向进程内 `InMemoryIndexer`，集群模式下指向远端索引节点的 `HttpIndexerClient`。

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use nekosearch_core::{indexer::Indexer, SearchQuery, SearchResult};
use std::sync::Arc;

/// 启动检索服务 HTTP 服务。
pub async fn serve(addr: &str, indexer: Arc<dyn Indexer>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/search", get(search))
        .with_state(indexer);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("searcher http listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(serde::Deserialize)]
struct SearchParams {
    q: String,
    top_k: Option<usize>,
}

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
