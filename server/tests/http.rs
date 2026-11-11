use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use batvision_server::{AppState, app, config::Config};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

fn config(web_dir: Option<std::path::PathBuf>) -> Config {
    let mut config = Config::from_lookup(|_| None).unwrap();
    config.web_dir = web_dir;
    config
}

async fn get(router: axum::Router, uri: &str) -> (StatusCode, String) {
    let res = router
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn health_returns_ok_and_version() {
    let (status, body) = get(app(AppState::new(config(None))), "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn system_info_reports_pending_probe_as_null() {
    let (status, body) = get(app(AppState::new(config(None))), "/api/system/info").await;
    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_str(&body).unwrap();
    assert!(json["ffmpeg"].is_null());
}

#[tokio::test]
async fn openapi_lists_system_routes() {
    let (status, body) = get(app(AppState::new(config(None))), "/api/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_str(&body).unwrap();
    assert!(json["paths"]["/api/health"].is_object());
    assert!(json["paths"]["/api/system/info"].is_object());
}

#[tokio::test]
async fn unknown_api_route_is_404_even_with_web_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<html>spa</html>").unwrap();
    let (status, _) = get(
        app(AppState::new(config(Some(dir.path().into())))),
        "/api/does-not-exist",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn spa_routes_fall_back_to_index() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<html>spa</html>").unwrap();
    let (status, body) = get(
        app(AppState::new(config(Some(dir.path().into())))),
        "/series/42",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("spa"));
}
