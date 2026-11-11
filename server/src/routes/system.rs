use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{AppState, ffmpeg::FfmpegInfo};

#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
}

/// Liveness check used by Docker and the reverse proxy.
#[utoipa::path(get, path = "/api/health", tag = "system",
    responses((status = 200, body = Health)))]
pub async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SystemInfo {
    pub version: &'static str,
    /// `None` while the startup probe is still running.
    pub ffmpeg: Option<FfmpegInfo>,
}

/// Server version and detected ffmpeg capabilities.
///
/// TODO(milestone 1): restrict to admins once authentication exists.
#[utoipa::path(get, path = "/api/system/info", tag = "system",
    responses((status = 200, body = SystemInfo)))]
pub async fn info(State(state): State<AppState>) -> Json<SystemInfo> {
    Json(SystemInfo {
        version: env!("CARGO_PKG_VERSION"),
        ffmpeg: state.ffmpeg.read().await.clone(),
    })
}
