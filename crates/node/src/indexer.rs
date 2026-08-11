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
use nekosearch_core::{indexer::Indexer, Doc, SearchQuery, SearchResult};
use std::sync::Arc;

/// 构建索引节点路由（抽出以便测试在临时端口起服务）。
pub(crate) fn router(idx: Arc<dyn Indexer>) -> Router {
    Router::new()
        .route("/docs", post(add_doc))
        .route("/search", post(search))
        .route("/suggest", get(suggest))
        .with_state(idx)
}

/// 启动索引节点 HTTP 服务。
pub async fn serve(addr: &str, idx: Arc<dyn Indexer>) -> anyhow::Result<()> {
    let app = router(idx);
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
    Json(
        idx.suggest(&params.q, params.limit.unwrap_or(10))
            .await
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekosearch_core::indexer::InMemoryIndexer;
    use nekosearch_core::{Doc, SearchQuery, SearchResult};
    use std::sync::Arc;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn docs_search_suggest_over_http() {
        let idx = Arc::new(InMemoryIndexer::new());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _h = tokio::spawn(async move {
            axum::serve(listener, router(idx)).await.unwrap();
        });
        let base = format!("http://{addr}");
        let client = reqwest::Client::new();

        // 写入
        let doc = Doc {
            id: "rust".into(),
            url: "https://rust-lang.org".into(),
            title: "Rust".into(),
            body: "rust systems programming language memory safety".into(),
        };
        let st = client
            .post(format!("{base}/docs"))
            .json(&doc)
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(st, reqwest::StatusCode::OK);

        // 检索
        let resp: Vec<SearchResult> = client
            .post(format!("{base}/search"))
            .json(&SearchQuery {
                q: "rust".into(),
                top_k: 5,
            })
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(
            resp.iter().any(|r| r.doc.id == "rust"),
            "索引节点检索应命中 rust"
        );

        // 自动补全
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
            "索引节点 /suggest 应含 rust"
        );
    }
}
