//! 注册中心 HTTP 服务（REST）。
//!
//! 仅在节点承担 registry 角色时启动，底层由 `InMemoryRegistry` 支撑。
//! 集群中其它节点的 crawler/indexer/searcher 通过 `HttpRegistryClient` 调用这些端点。

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use nekosearch_core::{
    registry::{InMemoryRegistry, Registry},
    IdRequest, NodeInfo, RegisterRequest, Role,
};
use std::collections::HashMap;

/// 启动注册中心 HTTP 服务。
pub async fn serve(addr: &str, reg: InMemoryRegistry) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/register", post(register))
        .route("/heartbeat", post(heartbeat))
        .route("/deregister", post(deregister))
        .route("/nodes", get(list_nodes))
        .route("/tasks", post(submit_task))
        .route("/tasks/claim", post(claim_task))
        .with_state(reg);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("registry http listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// 领取任务请求体。
#[derive(serde::Deserialize)]
struct ClaimRequest {
    crawler_id: String,
}

async fn register(
    State(reg): State<InMemoryRegistry>,
    Json(req): Json<RegisterRequest>,
) -> StatusCode {
    match reg.register(req).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn heartbeat(State(reg): State<InMemoryRegistry>, Json(req): Json<IdRequest>) -> StatusCode {
    match reg.heartbeat(&req.id).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn deregister(
    State(reg): State<InMemoryRegistry>,
    Json(req): Json<IdRequest>,
) -> StatusCode {
    reg.deregister(&req.id).await;
    StatusCode::OK
}

async fn list_nodes(
    State(reg): State<InMemoryRegistry>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<NodeInfo>> {
    let role = params.get("role").and_then(|s| match s.as_str() {
        "registry" => Some(Role::Registry),
        "crawler" => Some(Role::Crawler),
        "indexer" => Some(Role::Indexer),
        "searcher" => Some(Role::Searcher),
        "all" => Some(Role::All),
        _ => None,
    });
    Json(reg.list_nodes(role).await.unwrap_or_default())
}

async fn submit_task(
    State(reg): State<InMemoryRegistry>,
    Json(task): Json<nekosearch_core::CrawlTask>,
) -> StatusCode {
    match reg.submit_task(task).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn claim_task(
    State(reg): State<InMemoryRegistry>,
    Json(req): Json<ClaimRequest>,
) -> Json<Option<nekosearch_core::CrawlTask>> {
    Json(reg.claim_task(&req.crawler_id).await.unwrap_or(None))
}
