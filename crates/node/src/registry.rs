//! 注册中心 HTTP 服务（REST）。
//!
//! 仅在节点承担 registry 角色时启动，底层由 `InMemoryRegistry` 支撑。
//! 集群中其它节点的 crawler/indexer/searcher 通过 `HttpRegistryClient` 调用这些端点。

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use nekosearch_core::{
    registry::{InMemoryRegistry, Registry},
    IdRequest, RegisterRequest, Role,
};
use std::collections::HashMap;
use std::time::Duration;

/// 构建注册中心路由（抽出以便测试在临时端口起服务）。
pub(crate) fn router(reg: InMemoryRegistry) -> Router {
    Router::new()
        .route("/ping", get(ping))
        .route("/register", post(register))
        .route("/heartbeat", post(heartbeat))
        .route("/deregister", post(deregister))
        .route("/nodes", get(list_nodes))
        .route("/tasks", post(submit_task))
        .route("/tasks/claim", post(claim_task))
        .with_state(reg)
}

/// 启动注册中心 HTTP 服务。
pub async fn serve(addr: &str, reg: InMemoryRegistry) -> anyhow::Result<()> {
    let app = router(reg);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("registry http listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// 多注册中心 HA 心跳与选主循环。
///
/// 周期性 ping 各对端 `/ping` 学习其 self_id，剔除超时对端，重新计算 leader
/// （{本节点} ∪ {在线对端} 中字典序最小的 self_id 为 leader）。非 leader 的注册中心
/// 会把写请求 307 重定向到 leader，从而实现单写多备的高可用。无 peers 时无害。
pub async fn run_ha_loop(reg: InMemoryRegistry) {
    let client = reqwest::Client::new();
    loop {
        let peers = reg.ha_peers().await;
        for addr in &peers {
            if let Ok(r) = client.get(format!("{addr}/ping")).send().await {
                if let Ok(p) = r.json::<PingResp>().await {
                    reg.observe_peer(addr, &p.id).await;
                }
            }
        }
        reg.drop_stale_peers(10_000).await;
        reg.recompute_leader().await;
        let is_leader = reg.is_leader().await;
        let leader = reg.leader().await;
        tracing::info!(is_leader = is_leader, leader = %leader, "registry HA tick");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// 健康/选举探测响应。
#[derive(serde::Serialize, serde::Deserialize)]
struct PingResp {
    id: String,
    leader: String,
}

/// 健康探测：每个注册中心（含非 leader）都需响应，供对端发现与选主。
async fn ping(State(reg): State<InMemoryRegistry>) -> Json<PingResp> {
    Json(PingResp {
        id: reg.self_id().await,
        leader: reg.leader().await,
    })
}

/// 若本节点不是 leader，返回到 leader 的 307 重定向（供写请求转发）；否则返回 None。
async fn leader_guard(reg: &InMemoryRegistry, path: &str) -> Option<Response> {
    if reg.is_leader().await {
        None
    } else {
        let leader = reg.leader().await;
        Some(Redirect::temporary(&format!("{leader}/{path}")).into_response())
    }
}

/// 领取任务请求体。
#[derive(serde::Deserialize)]
struct ClaimRequest {
    crawler_id: String,
}

async fn register(
    State(reg): State<InMemoryRegistry>,
    Json(req): Json<RegisterRequest>,
) -> Response {
    if let Some(r) = leader_guard(&reg, "register").await {
        return r;
    }
    match reg.register(req).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn heartbeat(State(reg): State<InMemoryRegistry>, Json(req): Json<IdRequest>) -> Response {
    if let Some(r) = leader_guard(&reg, "heartbeat").await {
        return r;
    }
    match reg.heartbeat(&req.id).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn deregister(State(reg): State<InMemoryRegistry>, Json(req): Json<IdRequest>) -> Response {
    if let Some(r) = leader_guard(&reg, "deregister").await {
        return r;
    }
    let _ = reg.deregister(&req.id).await;
    StatusCode::OK.into_response()
}

async fn list_nodes(
    State(reg): State<InMemoryRegistry>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if let Some(r) = leader_guard(&reg, "nodes").await {
        return r;
    }
    let role = params.get("role").and_then(|s| match s.as_str() {
        "registry" => Some(Role::Registry),
        "crawler" => Some(Role::Crawler),
        "indexer" => Some(Role::Indexer),
        "searcher" => Some(Role::Searcher),
        "all" => Some(Role::All),
        _ => None,
    });
    Json(reg.list_nodes(role).await.unwrap_or_default()).into_response()
}

async fn submit_task(
    State(reg): State<InMemoryRegistry>,
    Json(task): Json<nekosearch_core::CrawlTask>,
) -> Response {
    if let Some(r) = leader_guard(&reg, "tasks").await {
        return r;
    }
    match reg.submit_task(task).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn claim_task(
    State(reg): State<InMemoryRegistry>,
    Json(req): Json<ClaimRequest>,
) -> Response {
    if let Some(r) = leader_guard(&reg, "tasks/claim").await {
        return r;
    }
    Json(reg.claim_task(&req.crawler_id).await.unwrap_or(None)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekosearch_core::registry::InMemoryRegistry;
    use nekosearch_core::{NodeInfo, RegisterRequest, Role};
    use tokio::net::TcpListener;

    #[derive(serde::Deserialize)]
    struct Ping {
        id: String,
        leader: String,
    }

    #[tokio::test]
    async fn ping_reports_self_id_and_leader() {
        let reg = InMemoryRegistry::new_with_ha("http://127.0.0.1:7510".into(), Vec::new());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _h = tokio::spawn(async move {
            axum::serve(listener, router(reg)).await.unwrap();
        });
        let base = format!("http://{addr}");
        let ping: Ping = reqwest::Client::new()
            .get(format!("{base}/ping"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(ping.id, "http://127.0.0.1:7510");
        assert_eq!(ping.leader, "http://127.0.0.1:7510");
    }

    #[tokio::test]
    async fn register_then_listed() {
        let reg = InMemoryRegistry::new_with_ha("http://127.0.0.1:7510".into(), Vec::new());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _h = tokio::spawn(async move {
            axum::serve(listener, router(reg)).await.unwrap();
        });
        let base = format!("http://{addr}");
        let client = reqwest::Client::new();
        let st = client
            .post(format!("{base}/register"))
            .json(&RegisterRequest {
                id: "c1".into(),
                role: Role::Crawler,
                addr: "http://127.0.0.1:9999".into(),
            })
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(st, reqwest::StatusCode::OK);
        let nodes: Vec<NodeInfo> = client
            .get(format!("{base}/nodes"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(nodes.iter().any(|n| n.id == "c1"), "注册后 /nodes 应含 c1");
    }

    // 多注册中心 HA：非 leader 把写请求重定向到 leader（单写多备）。
    #[tokio::test]
    async fn non_leader_redirects_writes_to_leader() {
        // 本节点 id=7511，已知对端 7510（字典序更小 => 7510 为 leader）。
        let reg = InMemoryRegistry::new_with_ha(
            "http://127.0.0.1:7511".into(),
            vec!["http://127.0.0.1:7510".into()],
        );
        reg.observe_peer("http://127.0.0.1:7510", "http://127.0.0.1:7510")
            .await;
        reg.recompute_leader().await;
        assert!(
            !reg.is_leader().await,
            "7511 在已知 7510 存在时不应是 leader"
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _h = tokio::spawn(async move {
            axum::serve(listener, router(reg)).await.unwrap();
        });
        let base = format!("http://{addr}");
        // 不跟随重定向，直接检查 307/308 与 Location。
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let resp = client
            .post(format!("{base}/register"))
            .json(&RegisterRequest {
                id: "x".into(),
                role: Role::Registry,
                addr: "http://127.0.0.1:7513".into(),
            })
            .send()
            .await
            .unwrap();
        assert!(
            resp.status().is_redirection(),
            "非 leader 写请求应返回重定向，实际 {}",
            resp.status()
        );
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .expect("应有 Location 头")
            .to_str()
            .unwrap();
        assert!(
            loc.contains("127.0.0.1:7510"),
            "重定向应指向 leader 7510，实际 {loc}"
        );
    }
}
