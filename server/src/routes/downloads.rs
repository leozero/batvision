//! Downloads: what each device should hold, watchlist, series subscriptions, the
//! encode queue, and serving encoded files.

use axum::{
    Json,
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::ToSchema;

use crate::{
    AppState,
    auth::{Admin, CurrentDevice, Platform},
    db::now,
    downloads::{
        encode::VariantSubtitle,
        plan::Profile,
        service::{self, DeviceSettings, PlanItem},
    },
    error::{ApiError, ApiResult},
    playback::{normalize_language, signing::Grant},
    userdata::{self, OfflineProgress, PlayableKind},
};

fn valid_language(code: &Option<String>) -> ApiResult<Option<String>> {
    match code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        None => Ok(None),
        Some(c) => normalize_language(c)
            .map(Some)
            .ok_or_else(|| ApiError::bad_request("invalid_language", "unknown audio language")),
    }
}

// ---- The device itself ----------------------------------------------------------------

#[derive(Deserialize, ToSchema, Default)]
pub struct PlanRequest {
    /// The device decodes AV1 in hardware (then lighter versions use AV1).
    pub av1: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct DownloadPlan {
    pub settings: DeviceSettings,
    /// In priority order: keep the longest prefix that fits the quota, delete the rest.
    pub items: Vec<PlanItem>,
}

/// What this device should have on disk. Call on start, when back online, periodically.
#[utoipa::path(post, path = "/api/downloads/plan", tag = "downloads", security(("bearer" = [])),
    request_body = PlanRequest, responses((status = 200, body = DownloadPlan)))]
pub async fn plan(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<PlanRequest>,
) -> ApiResult<Json<DownloadPlan>> {
    let platform = service::download_device(&state, current.user.id, current.device_id).await?;
    if let Some(av1) = req.av1 {
        let (settings, _) = service::load_settings(&state, current.device_id, platform).await?;
        service::save_settings(&state, current.device_id, &settings, Some(av1)).await?;
    }
    let (settings, items) =
        service::plan_for_device(&state, &current.user, current.device_id).await?;
    Ok(Json(DownloadPlan { settings, items }))
}

#[derive(Deserialize, ToSchema)]
pub struct ItemRef {
    pub kind: PlayableKind,
    pub id: i64,
}

#[derive(Serialize, ToSchema)]
pub struct Requested {
    pub variant_id: i64,
}

/// Asks the server to prepare an expected item (encoding starts only now).
#[utoipa::path(post, path = "/api/downloads/request", tag = "downloads", security(("bearer" = [])),
    request_body = ItemRef, responses((status = 200, body = Requested), (status = 409, body = crate::error::ErrorBody)))]
pub async fn request(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(item): Json<ItemRef>,
) -> ApiResult<Json<Requested>> {
    let variant_id =
        service::request(&state, &current.user, current.device_id, item.kind, item.id).await?;
    Ok(Json(Requested { variant_id }))
}

#[derive(Deserialize, ToSchema)]
pub struct ConfirmRequest {
    pub variant_id: i64,
}

/// The device finished downloading: the server can drop its cache copy.
#[utoipa::path(post, path = "/api/downloads/confirm", tag = "downloads", security(("bearer" = [])),
    request_body = ConfirmRequest, responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
pub async fn confirm(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<ConfirmRequest>,
) -> ApiResult<StatusCode> {
    service::confirm(&state, current.device_id, req.variant_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// An encoded file (video or subtitle), with Range support to resume downloads.
#[utoipa::path(get, path = "/api/downloads/file/{token}", tag = "downloads",
    params(("token" = String, Path)), responses((status = 200), (status = 206), (status = 404)))]
pub async fn file(
    State(state): State<AppState>,
    Path(token): Path<String>,
    request: Request,
) -> ApiResult<axum::response::Response> {
    let Some(Grant::Variant { v, f }) = state.signer.verify(&token, now()) else {
        return Err(ApiError::NotFound);
    };
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT status, subtitles_json FROM variants WHERE id = ?")
            .bind(v)
            .fetch_optional(&state.db)
            .await?;
    let Some((status, subtitles)) = row.filter(|(s, _)| s == "ready") else {
        return Err(ApiError::NotFound);
    };
    let _ = status;
    let dir = state.encoder.dir(v);
    let path = if f == 0 {
        dir.join("video.mp4")
    } else {
        let subs: Vec<VariantSubtitle> = subtitles
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();
        let sub = subs.get((f - 1) as usize).ok_or(ApiError::NotFound)?;
        dir.join(&sub.file)
    };
    let response = ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("serving file: {e}")))?;
    Ok(response.map(Body::new))
}

/// Progress made offline, uploaded in one go. The most recent watch wins.
#[utoipa::path(post, path = "/api/progress/batch", tag = "downloads", security(("bearer" = [])),
    request_body = Vec<OfflineProgress>, responses((status = 200, body = BatchResult)))]
pub async fn progress_batch(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(records): Json<Vec<OfflineProgress>>,
) -> ApiResult<Json<BatchResult>> {
    if records.len() > 1000 {
        return Err(ApiError::bad_request(
            "too_many_records",
            "send at most 1000 records at once",
        ));
    }
    let applied = userdata::apply_offline(&state.db, current.user.id, &records).await?;
    Ok(Json(BatchResult { applied }))
}

#[derive(Serialize, ToSchema)]
pub struct BatchResult {
    pub applied: usize,
}

// ---- Control from any signed-in client (web) -------------------------------------------

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct SubscriptionInfo {
    pub id: i64,
    pub series_id: i64,
    pub series_title: String,
    pub mode: String,
    pub count: Option<i64>,
    pub season_number: Option<i64>,
    pub profile: Option<Profile>,
    pub audio_language: Option<String>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct ManualInfo {
    pub id: i64,
    pub kind: String,
    pub item_id: i64,
    pub title: String,
    pub profile: Option<Profile>,
    pub audio_language: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct DeviceDownloads {
    pub id: i64,
    pub name: String,
    pub platform: Platform,
    pub last_seen_at: i64,
    pub settings: DeviceSettings,
    pub items: Vec<PlanItem>,
    pub subscriptions: Vec<SubscriptionInfo>,
    pub manual: Vec<ManualInfo>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct WatchlistItem {
    pub movie_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub poster_path: Option<String>,
    pub audio_language: Option<String>,
    pub added_at: i64,
}

#[derive(Serialize, ToSchema)]
pub struct DownloadsOverview {
    /// The user's devices able to download (apps, not browsers).
    pub devices: Vec<DeviceDownloads>,
    pub watchlist: Vec<WatchlistItem>,
}

async fn watchlist_items(
    state: &AppState,
    user: &crate::auth::User,
) -> ApiResult<Vec<WatchlistItem>> {
    Ok(sqlx::query_as(
        "SELECT w.movie_id,
           COALESCE(
             (SELECT title FROM translations WHERE item_kind = 'movie' AND item_id = m.id AND language = ?1),
             (SELECT title FROM translations WHERE item_kind = 'movie' AND item_id = m.id AND language = ?2),
             m.parsed_title) AS title,
           COALESCE(CAST(substr(m.release_date, 1, 4) AS INTEGER), m.parsed_year) AS year,
           m.poster_path, w.audio_language, w.added_at
         FROM watchlist w JOIN movies m ON m.id = w.movie_id
         WHERE w.user_id = ?3 AND m.missing_since IS NULL ORDER BY w.added_at",
    )
    .bind(&user.language)
    .bind(&state.config.default_language)
    .bind(user.id)
    .fetch_all(&state.db)
    .await?)
}

#[utoipa::path(get, path = "/api/downloads/overview", tag = "downloads", security(("bearer" = [])),
    responses((status = 200, body = DownloadsOverview)))]
pub async fn overview(
    State(state): State<AppState>,
    current: CurrentDevice,
) -> ApiResult<Json<DownloadsOverview>> {
    let user = &current.user;
    let devices: Vec<(i64, String, Platform, i64)> = sqlx::query_as(
        "SELECT id, name, platform, last_seen_at FROM devices
         WHERE user_id = ? AND revoked_at IS NULL AND platform != 'web' ORDER BY last_seen_at DESC",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::new();
    for (id, name, platform, last_seen_at) in devices {
        let (settings, items) = service::plan_for_device(&state, user, id).await?;
        let subscriptions = sqlx::query_as(
            "SELECT s.id, s.series_id,
               COALESCE(
                 (SELECT title FROM translations WHERE item_kind = 'series' AND item_id = se.id AND language = ?1),
                 (SELECT title FROM translations WHERE item_kind = 'series' AND item_id = se.id AND language = ?2),
                 se.parsed_title) AS series_title,
               s.mode, s.count, s.season_number, s.profile, s.audio_language
             FROM series_subscriptions s JOIN series se ON se.id = s.series_id
             WHERE s.device_id = ?3 ORDER BY s.created_at",
        )
        .bind(&user.language)
        .bind(&state.config.default_language)
        .bind(id)
        .fetch_all(&state.db)
        .await?;
        let manual = sqlx::query_as(
            "SELECT d.id, d.item_kind AS kind, d.item_id,
               COALESCE(
                 (SELECT title FROM translations t WHERE t.item_kind = d.item_kind AND t.item_id = d.item_id
                    AND t.language = ?1),
                 (SELECT parsed_title FROM movies WHERE d.item_kind = 'movie' AND id = d.item_id),
                 'Episode') AS title,
               d.profile, d.audio_language
             FROM manual_downloads d WHERE d.device_id = ?2 ORDER BY d.created_at",
        )
        .bind(&user.language)
        .bind(id)
        .fetch_all(&state.db)
        .await?;
        out.push(DeviceDownloads {
            id,
            name,
            platform,
            last_seen_at,
            settings,
            items,
            subscriptions,
            manual,
        });
    }
    Ok(Json(DownloadsOverview {
        devices: out,
        watchlist: watchlist_items(&state, user).await?,
    }))
}

#[utoipa::path(put, path = "/api/downloads/devices/{id}/settings", tag = "downloads",
    security(("bearer" = [])), params(("id" = i64, Path)), request_body = DeviceSettings,
    responses((status = 200, body = DeviceSettings), (status = 404, body = crate::error::ErrorBody)))]
pub async fn put_settings(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(id): Path<i64>,
    Json(settings): Json<DeviceSettings>,
) -> ApiResult<Json<DeviceSettings>> {
    let platform = service::download_device(&state, current.user.id, id).await?;
    if settings.quota_bytes.is_some_and(|q| q < 0)
        || settings.mobile_cap_bytes.is_some_and(|q| q < 0)
    {
        return Err(ApiError::bad_request(
            "invalid_quota",
            "quotas can't be negative",
        ));
    }
    service::save_settings(&state, id, &settings, None).await?;
    Ok(Json(service::load_settings(&state, id, platform).await?.0))
}

// Watchlist

#[utoipa::path(get, path = "/api/watchlist", tag = "downloads", security(("bearer" = [])),
    responses((status = 200, body = Vec<WatchlistItem>)))]
pub async fn get_watchlist(
    State(state): State<AppState>,
    current: CurrentDevice,
) -> ApiResult<Json<Vec<WatchlistItem>>> {
    Ok(Json(watchlist_items(&state, &current.user).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct WatchlistAdd {
    /// ISO 639-2 audio language to download; omitted = the preferred one.
    pub audio_language: Option<String>,
}

#[utoipa::path(put, path = "/api/watchlist/{movie_id}", tag = "downloads", security(("bearer" = [])),
    params(("movie_id" = i64, Path)), request_body = WatchlistAdd,
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
pub async fn add_to_watchlist(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(movie_id): Path<i64>,
    Json(req): Json<WatchlistAdd>,
) -> ApiResult<StatusCode> {
    let audio = valid_language(&req.audio_language)?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM movies WHERE id = ? AND missing_since IS NULL)",
    )
    .bind(movie_id)
    .fetch_one(&state.db)
    .await?;
    if !exists {
        return Err(ApiError::NotFound);
    }
    sqlx::query(
        "INSERT INTO watchlist (user_id, movie_id, audio_language, added_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (user_id, movie_id) DO UPDATE SET audio_language = excluded.audio_language",
    )
    .bind(current.user.id)
    .bind(movie_id)
    .bind(audio)
    .bind(now())
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/watchlist/{movie_id}", tag = "downloads", security(("bearer" = [])),
    params(("movie_id" = i64, Path)), responses((status = 204)))]
pub async fn remove_from_watchlist(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(movie_id): Path<i64>,
) -> ApiResult<StatusCode> {
    sqlx::query("DELETE FROM watchlist WHERE user_id = ? AND movie_id = ?")
        .bind(current.user.id)
        .bind(movie_id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// Series subscriptions

#[derive(Debug, Clone, Copy, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionMode {
    Rolling,
    Season,
}

#[derive(Deserialize, ToSchema)]
pub struct SubscribeRequest {
    pub device_id: i64,
    pub series_id: i64,
    pub mode: SubscriptionMode,
    /// rolling: number of unwatched episodes to keep (1 to 20).
    pub count: Option<i64>,
    /// season: the season to download.
    pub season_number: Option<i64>,
    /// Omitted: the device's profile.
    pub profile: Option<Profile>,
    pub audio_language: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Created {
    pub id: i64,
}

#[utoipa::path(post, path = "/api/downloads/subscriptions", tag = "downloads", security(("bearer" = [])),
    request_body = SubscribeRequest,
    responses((status = 201, body = Created), (status = 400, body = crate::error::ErrorBody)))]
pub async fn subscribe(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<SubscribeRequest>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    service::download_device(&state, current.user.id, req.device_id).await?;
    let audio = valid_language(&req.audio_language)?;
    let (count, season) = match req.mode {
        SubscriptionMode::Rolling => {
            let count = req.count.unwrap_or(3);
            if !(1..=20).contains(&count) {
                return Err(ApiError::bad_request(
                    "invalid_count",
                    "count must be between 1 and 20",
                ));
            }
            (Some(count), None)
        }
        SubscriptionMode::Season => {
            let season = req.season_number.ok_or_else(|| {
                ApiError::bad_request("missing_season", "season_number is required")
            })?;
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM episodes WHERE series_id = ? AND season_number = ? AND missing_since IS NULL)",
            )
            .bind(req.series_id)
            .bind(season)
            .fetch_one(&state.db)
            .await?;
            if !exists {
                return Err(ApiError::NotFound);
            }
            (None, Some(season))
        }
    };
    let mode = if req.mode == SubscriptionMode::Rolling {
        "rolling"
    } else {
        "season"
    };
    // One rolling subscription per series and device: a new one replaces the old.
    if req.mode == SubscriptionMode::Rolling {
        sqlx::query("DELETE FROM series_subscriptions WHERE device_id = ? AND series_id = ? AND mode = 'rolling'")
            .bind(req.device_id)
            .bind(req.series_id)
            .execute(&state.db)
            .await?;
    }
    let result = sqlx::query_scalar(
        "INSERT INTO series_subscriptions (user_id, device_id, series_id, mode, count, season_number,
           profile, audio_language, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (device_id, series_id, mode, season_number) DO UPDATE SET
           count = excluded.count, profile = excluded.profile, audio_language = excluded.audio_language
         RETURNING id",
    )
    .bind(current.user.id)
    .bind(req.device_id)
    .bind(req.series_id)
    .bind(mode)
    .bind(count)
    .bind(season)
    .bind(req.profile)
    .bind(audio)
    .bind(now())
    .fetch_one(&state.db)
    .await;
    match result {
        Ok(id) => Ok((StatusCode::CREATED, Json(Created { id }))),
        Err(sqlx::Error::Database(e)) if e.is_foreign_key_violation() => Err(ApiError::NotFound),
        Err(e) => Err(e.into()),
    }
}

#[utoipa::path(delete, path = "/api/downloads/subscriptions/{id}", tag = "downloads",
    security(("bearer" = [])), params(("id" = i64, Path)), responses((status = 204), (status = 404)))]
pub async fn unsubscribe(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let deleted = sqlx::query("DELETE FROM series_subscriptions WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(current.user.id)
        .execute(&state.db)
        .await?;
    if deleted.rows_affected() == 1 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ManualRequest {
    pub device_id: i64,
    pub kind: PlayableKind,
    pub id: i64,
    pub profile: Option<Profile>,
    pub audio_language: Option<String>,
}

#[utoipa::path(post, path = "/api/downloads/manual", tag = "downloads", security(("bearer" = [])),
    request_body = ManualRequest, responses((status = 201, body = Created), (status = 404)))]
pub async fn add_manual(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<ManualRequest>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    service::download_device(&state, current.user.id, req.device_id).await?;
    let audio = valid_language(&req.audio_language)?;
    let exists: bool = match req.kind {
        PlayableKind::Movie => sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM movies WHERE id = ? AND missing_since IS NULL)",
        ),
        PlayableKind::Episode => sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM episodes WHERE id = ? AND missing_since IS NULL)",
        ),
    }
    .bind(req.id)
    .fetch_one(&state.db)
    .await?;
    if !exists {
        return Err(ApiError::NotFound);
    }
    let id = sqlx::query_scalar(
        "INSERT INTO manual_downloads (user_id, device_id, item_kind, item_id, profile, audio_language, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (device_id, item_kind, item_id) DO UPDATE SET profile = excluded.profile,
           audio_language = excluded.audio_language
         RETURNING id",
    )
    .bind(current.user.id)
    .bind(req.device_id)
    .bind(req.kind.as_str())
    .bind(req.id)
    .bind(req.profile)
    .bind(audio)
    .bind(now())
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

#[utoipa::path(delete, path = "/api/downloads/manual/{id}", tag = "downloads", security(("bearer" = [])),
    params(("id" = i64, Path)), responses((status = 204), (status = 404)))]
pub async fn remove_manual(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let deleted = sqlx::query("DELETE FROM manual_downloads WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(current.user.id)
        .execute(&state.db)
        .await?;
    if deleted.rows_affected() == 1 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

// ---- Admin -----------------------------------------------------------------------------

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct EncodeInfo {
    pub id: i64,
    pub item_kind: String,
    pub item_id: i64,
    pub file: String,
    pub profile: Profile,
    pub codec: String,
    pub status: String,
    pub progress: f64,
    pub size_bytes: Option<i64>,
    pub error: Option<String>,
    pub created_at: i64,
    /// Devices that asked and haven't confirmed the download yet.
    pub waiting_devices: i64,
}

#[derive(Serialize, ToSchema)]
pub struct EncodeQueue {
    pub variants: Vec<EncodeInfo>,
    pub cache_bytes: i64,
    pub budget_bytes: i64,
}

#[utoipa::path(get, path = "/api/admin/encodes", tag = "admin", security(("bearer" = [])),
    responses((status = 200, body = EncodeQueue)))]
pub async fn encodes(State(state): State<AppState>, _: Admin) -> ApiResult<Json<EncodeQueue>> {
    let variants = sqlx::query_as(
        "SELECT v.id, mf.item_kind, mf.item_id, mf.path AS file, v.profile, v.codec, v.status, v.progress,
           v.size_bytes, v.error, v.created_at,
           (SELECT COUNT(*) FROM variant_requests r WHERE r.variant_id = v.id AND r.confirmed_at IS NULL)
             AS waiting_devices
         FROM variants v JOIN media_files mf ON mf.id = v.media_file_id
         ORDER BY CASE v.status WHEN 'encoding' THEN 0 WHEN 'queued' THEN 1 WHEN 'failed' THEN 2 ELSE 3 END,
                  v.created_at",
    )
    .fetch_all(&state.db)
    .await?;
    let cache_bytes = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes), 0) FROM variants WHERE status = 'ready'",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(EncodeQueue {
        variants,
        cache_bytes,
        budget_bytes: state.encoder.budget_bytes(),
    }))
}

/// Deletes an encoded version (or cancels a queued one). Devices can request it again.
#[utoipa::path(delete, path = "/api/admin/encodes/{id}", tag = "admin", security(("bearer" = [])),
    params(("id" = i64, Path)), responses((status = 204)))]
pub async fn delete_encode(
    State(state): State<AppState>,
    _: Admin,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    state.encoder.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
