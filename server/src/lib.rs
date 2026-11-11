pub mod config;
pub mod ffmpeg;
pub mod routes;

use std::sync::Arc;

use axum::{Json, Router, http::StatusCode, routing::get};
use tokio::sync::RwLock;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use utoipa::OpenApi;

use crate::{config::Config, ffmpeg::FfmpegInfo};

#[derive(OpenApi)]
#[openapi(
    info(title = "batvision", license(name = "AGPL-3.0-only")),
    paths(routes::system::health, routes::system::info),
    tags((name = "system", description = "Server status"))
)]
pub struct ApiDoc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    /// Filled in by the background ffmpeg probe.
    pub ffmpeg: Arc<RwLock<Option<FfmpegInfo>>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            ffmpeg: Arc::new(RwLock::new(None)),
        }
    }
}

pub fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(routes::system::health))
        .route("/system/info", get(routes::system::info))
        .route("/openapi.json", get(|| async { Json(ApiDoc::openapi()) }))
        // Without this, unknown /api paths would inherit the SPA fallback and return HTML.
        .fallback(|| async { StatusCode::NOT_FOUND });

    let mut router = Router::new().nest("/api", api);

    // Serve the web client as a SPA: unknown paths fall back to index.html
    // so client-side routes survive a page reload.
    if let Some(web_dir) = &state.config.web_dir {
        let index = web_dir.join("index.html");
        router = router.fallback_service(ServeDir::new(web_dir).fallback(ServeFile::new(index)));
    }

    router.layer(TraceLayer::new_for_http()).with_state(state)
}
