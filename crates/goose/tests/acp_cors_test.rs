use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Method, Request},
    response::Response,
};
use goose::acp::server_factory::{AcpServer, AcpServerFactoryConfig};
use goose::acp::transport::create_acp_router;
use goose::agents::GoosePlatform;
use tempfile::TempDir;
use tower::ServiceExt;

fn test_router(dir: &TempDir) -> Router {
    let server = Arc::new(AcpServer::new(AcpServerFactoryConfig {
        builtins: vec![],
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        goose_platform: GoosePlatform::GooseCli,
        additional_source_roots: Vec::new(),
        scheduler: None,
    }));
    create_acp_router(server)
}

async fn send(router: &Router, origin: &str) -> Response {
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/acp")
        .header("Origin", origin)
        .header("Access-Control-Request-Method", "POST")
        .header(
            "Access-Control-Request-Headers",
            "content-type,acp-connection-id",
        )
        .body(Body::empty())
        .unwrap();

    router.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn acp_cors_does_not_allow_arbitrary_web_origins() {
    let dir = tempfile::tempdir().unwrap();
    let router = test_router(&dir);

    let response = send(&router, "https://evil.example").await;
    let allow_origin = response
        .headers()
        .get("access-control-allow-origin")
        .and_then(|value| value.to_str().ok());

    assert_ne!(allow_origin, Some("*"));
    assert_ne!(allow_origin, Some("https://evil.example"));
}

#[tokio::test]
async fn acp_cors_allows_localhost_origins() {
    let dir = tempfile::tempdir().unwrap();
    let router = test_router(&dir);

    let response = send(&router, "http://localhost:3284").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}
