mod common;

use axum::http::StatusCode;
use common::{TestApp, test_config};

#[tokio::test]
async fn health_returns_ok_and_version() {
    let app = TestApp::new().await;
    let (status, json) = app.get("/api/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn system_info_is_admin_only() {
    let app = TestApp::new().await;
    let (status, _) = app.get("/api/system/info", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (token, _) = app.ready_admin().await;
    let (status, json) = app.get("/api/system/info", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["ffmpeg"].is_null(), "probe not run in tests");
}

#[tokio::test]
async fn openapi_lists_routes_and_bearer_scheme() {
    let app = TestApp::new().await;
    let (status, json) = app.get("/api/openapi.json", None).await;
    assert_eq!(status, StatusCode::OK);
    for path in [
        "/api/health",
        "/api/auth/login",
        "/api/me",
        "/api/admin/users",
    ] {
        assert!(json["paths"][path].is_object(), "missing {path}");
    }
    assert_eq!(
        json["components"]["securitySchemes"]["bearer"]["scheme"],
        "bearer"
    );
}

#[tokio::test]
async fn unknown_api_route_is_404_even_with_web_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<html>spa</html>").unwrap();
    let mut config = test_config();
    config.web_dir = Some(dir.path().into());
    let app = TestApp::with_config(config).await;
    let (status, _) = app.get("/api/does-not-exist", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn spa_routes_fall_back_to_index() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<html>spa</html>").unwrap();
    let mut config = test_config();
    config.web_dir = Some(dir.path().into());
    let app = TestApp::with_config(config).await;
    let (status, body) = app.get("/series/42", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.as_str().unwrap().contains("spa"));
}

#[tokio::test]
async fn every_openapi_ref_resolves() {
    fn collect(value: &serde_json::Value, refs: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, v) in map {
                    match (key.as_str(), v) {
                        ("$ref", serde_json::Value::String(r)) => refs.push(r.clone()),
                        _ => collect(v, refs),
                    }
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| collect(v, refs)),
            _ => {}
        }
    }
    let app = TestApp::new().await;
    let (_, spec) = app.get("/api/openapi.json", None).await;
    let mut refs = Vec::new();
    collect(&spec, &mut refs);
    assert!(!refs.is_empty());
    for r in refs {
        let name = r
            .strip_prefix("#/components/schemas/")
            .expect("local schema ref");
        assert!(
            spec["components"]["schemas"][name].is_object(),
            "unresolved $ref {r}"
        );
    }
}
