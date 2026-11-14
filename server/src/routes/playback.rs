//! Starting playback, serving media (direct or HLS) and subtitles.

use std::time::{Duration, Instant};

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::{IntoParams, ToSchema};

use crate::{
    AppState,
    auth::CurrentDevice,
    db::now,
    error::{ApiError, ApiResult},
    library::{
        media_info::{self, MediaInfo, TrackKind},
        scanner,
    },
    playback::{
        self, Capabilities, ExternalSubtitle, GeneralPreferences, Method, Quality, QualityOption,
        SeriesPreference, normalize_language,
        signing::Grant,
        subtitles,
        transcode::{TranscodeJob, VideoEncoder, valid_hls_file},
        trickplay,
    },
    userdata::{self, PlayableKind},
};

/// Signed media URLs stay valid long enough for a long movie with pauses.
const URL_LIFETIME_SECS: i64 = 12 * 3600;

#[derive(Deserialize, ToSchema)]
pub struct PlaybackRequest {
    pub kind: PlayableKind,
    pub id: i64,
    pub capabilities: Capabilities,
    /// Audio stream index chosen by the user. Omitted: automatic choice.
    pub audio_index: Option<i64>,
    /// `off`, `s<index>` (embedded) or `x<id>` (external file). Omitted: automatic choice.
    pub subtitle: Option<String>,
    /// Where to start, in ms. Omitted: the resume position.
    pub start_ms: Option<i64>,
    /// The user changed tracks: remember the choice for the rest of the series.
    #[serde(default)]
    pub remember: bool,
    /// Picture quality. Omitted: `max`.
    #[serde(default)]
    pub quality: Quality,
}

#[derive(Serialize, ToSchema)]
pub struct AudioOption {
    pub index: i64,
    pub language: Option<String>,
    pub title: Option<String>,
    pub codec: String,
    pub channels: Option<i64>,
    pub kind: TrackKind,
}

#[derive(Serialize, ToSchema)]
pub struct SubtitleChoice {
    pub id: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub kind: TrackKind,
    /// Image subtitles are burned into the video (triggers a transcode).
    pub text: bool,
    pub external: bool,
}

#[derive(Serialize, ToSchema)]
pub struct PlaybackInfo {
    pub method: Method,
    /// Direct: the file. Otherwise an HLS playlist (play with hls.js).
    pub url: String,
    pub hls: bool,
    /// HLS session, to stop it when leaving the player.
    pub session_id: Option<String>,
    /// HLS: media time at which the stream starts (its time 0). Direct: where to seek.
    pub start_ms: i64,
    pub duration_ms: Option<i64>,
    pub audio_tracks: Vec<AudioOption>,
    pub audio_index: Option<i64>,
    pub subtitles: Vec<SubtitleChoice>,
    pub subtitle: Option<String>,
    /// WebVTT for the selected text subtitle, timed for this stream.
    pub subtitle_url: Option<String>,
    pub next_episode_id: Option<i64>,
    /// The quality level of this stream.
    pub quality: Quality,
    /// What each level gives for this file, best first.
    pub qualities: Vec<QualityOption>,
}

struct MediaFile {
    id: i64,
    path: String,
    info: MediaInfo,
}

async fn load_media_file(state: &AppState, kind: PlayableKind, id: i64) -> ApiResult<MediaFile> {
    let row: Option<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, path, probe_json FROM media_files
         WHERE item_kind = ? AND item_id = ? AND missing_since IS NULL
         ORDER BY size_bytes DESC LIMIT 1",
    )
    .bind(kind.as_str())
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (file_id, path, probe) = row.ok_or(ApiError::NotFound)?;
    let probe = match probe {
        Some(json) => json,
        None => {
            // Not analyzed yet (new file): do it now.
            let full = state.config.media_dir.join(&path);
            let json = scanner::ffprobe(&state.config.ffprobe, &full)
                .await
                .map_err(|e| {
                    ApiError::conflict("unreadable_file", format!("cannot read the file: {e}"))
                })?;
            sqlx::query("UPDATE media_files SET probe_json = ?, probe_error = NULL WHERE id = ?")
                .bind(&json)
                .bind(file_id)
                .execute(&state.db)
                .await?;
            json
        }
    };
    let info = media_info::parse(&probe)?;
    Ok(MediaFile {
        id: file_id,
        path,
        info,
    })
}

fn parse_kind(kind: Option<String>) -> Option<TrackKind> {
    match kind.as_deref() {
        Some("commentary") => Some(TrackKind::Commentary),
        Some("forced") => Some(TrackKind::Forced),
        Some("sdh") => Some(TrackKind::Sdh),
        Some("normal") => Some(TrackKind::Normal),
        _ => None,
    }
}

fn kind_str(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Normal => "normal",
        TrackKind::Commentary => "commentary",
        TrackKind::Forced => "forced",
        TrackKind::Sdh => "sdh",
    }
}

#[utoipa::path(post, path = "/api/playback", tag = "playback", security(("bearer" = [])),
    request_body = PlaybackRequest,
    responses((status = 200, body = PlaybackInfo), (status = 404, body = crate::error::ErrorBody)))]
pub async fn start(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<PlaybackRequest>,
) -> ApiResult<Json<PlaybackInfo>> {
    let user = &current.user;
    let file = load_media_file(&state, req.kind, req.id).await?;
    let info = &file.info;

    let external: Vec<(ExternalSubtitle, Option<String>)> = sqlx::query_as::<_, (i64, Option<String>, bool, bool, String)>(
        "SELECT id, language, forced, sdh, path FROM subtitle_files WHERE media_file_id = ? ORDER BY path",
    )
    .bind(file.id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(id, language, forced, sdh, path)| {
        let kind = if forced {
            TrackKind::Forced
        } else if sdh {
            TrackKind::Sdh
        } else {
            TrackKind::Normal
        };
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        (ExternalSubtitle { id, language, kind }, name)
    })
    .collect();
    let external_only: Vec<ExternalSubtitle> = external.iter().map(|(x, _)| x.clone()).collect();
    let options = playback::subtitle_options(info, &external_only);

    let series_id: Option<i64> = match req.kind {
        PlayableKind::Episode => {
            sqlx::query_scalar("SELECT series_id FROM episodes WHERE id = ?")
                .bind(req.id)
                .fetch_optional(&state.db)
                .await?
        }
        PlayableKind::Movie => None,
    };

    // Tracks: explicit choice, else preferences.
    let explicit = req.audio_index.is_some() || req.subtitle.is_some();
    let (audio_index, subtitle_id) = if explicit {
        let audio = req
            .audio_index
            .filter(|i| info.audio.iter().any(|a| a.index == *i))
            .or_else(|| info.audio.first().map(|a| a.index));
        let subtitle = req
            .subtitle
            .filter(|s| s != "off" && options.iter().any(|o| &o.id == s));
        (audio, subtitle)
    } else {
        let prefs = crate::routes::home::load_preferences(&state, user.id).await?;
        let general = GeneralPreferences {
            audio_language: prefs.audio_language.as_deref().and_then(normalize_language),
            subtitle_language: prefs
                .subtitle_language
                .as_deref()
                .or(Some(user.language.as_str()))
                .and_then(normalize_language),
            subtitle_mode: Some(prefs.subtitle_mode),
        };
        let series_pref = match series_id {
            Some(series_id) => sqlx::query_as::<
                _,
                (
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    bool,
                ),
            >(
                "SELECT audio_language, audio_kind, subtitle_language, subtitle_kind, subtitles_off
                 FROM series_track_preferences WHERE user_id = ? AND series_id = ?",
            )
            .bind(user.id)
            .bind(series_id)
            .fetch_optional(&state.db)
            .await?
            .map(|(al, ak, sl, sk, off)| SeriesPreference {
                audio_language: al,
                audio_kind: parse_kind(ak),
                subtitle_language: sl,
                subtitle_kind: parse_kind(sk),
                subtitles_off: off,
            }),
            None => None,
        };
        playback::select_tracks(info, &options, &general, series_pref.as_ref())
    };

    // Remember a manual choice for the next episodes of the series.
    if req.remember
        && let Some(series_id) = series_id
    {
        let audio = audio_index.and_then(|i| info.audio.iter().find(|a| a.index == i));
        let subtitle = subtitle_id
            .as_ref()
            .and_then(|id| options.iter().find(|o| &o.id == id));
        sqlx::query(
            "INSERT INTO series_track_preferences (user_id, series_id, audio_language, audio_kind,
               subtitle_language, subtitle_kind, subtitles_off, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (user_id, series_id) DO UPDATE SET
               audio_language = excluded.audio_language, audio_kind = excluded.audio_kind,
               subtitle_language = excluded.subtitle_language, subtitle_kind = excluded.subtitle_kind,
               subtitles_off = excluded.subtitles_off, updated_at = excluded.updated_at",
        )
        .bind(user.id)
        .bind(series_id)
        .bind(audio.and_then(|a| a.language.as_deref()).and_then(normalize_language))
        .bind(audio.map(|a| kind_str(a.kind)))
        .bind(subtitle.and_then(|s| s.language.as_deref()).and_then(normalize_language))
        .bind(subtitle.map(|s| kind_str(s.kind)))
        .bind(subtitle.is_none())
        .bind(now())
        .execute(&state.db)
        .await?;
    }

    let selected_subtitle = subtitle_id
        .as_ref()
        .and_then(|id| options.iter().find(|o| &o.id == id));
    let plan = playback::decide(
        info,
        &req.capabilities,
        audio_index,
        selected_subtitle,
        req.quality,
    );
    let qualities =
        playback::quality_options(info, &req.capabilities, audio_index, selected_subtitle);

    let resume: i64 = sqlx::query_scalar(
        "SELECT position_ms FROM user_item_data WHERE user_id = ? AND item_kind = ? AND item_id = ?",
    )
    .bind(user.id)
    .bind(req.kind.as_str())
    .bind(req.id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or(0);
    let start_ms = req.start_ms.unwrap_or(resume).max(0);
    let expires = now() + URL_LIFETIME_SECS;

    let (mut url, hls, session_id) = if plan.method == Method::Direct {
        let token = state.signer.sign(Grant::File { f: file.id }, expires);
        (format!("/api/stream/{token}"), false, None)
    } else {
        let (encoder, gpu_filters) = match state.ffmpeg.read().await.as_ref() {
            Some(probe) if probe.working_encoders.iter().any(|e| e == "h264_nvenc") => {
                (VideoEncoder::Nvenc, probe.gpu_filters)
            }
            _ => (VideoEncoder::Software, false),
        };
        let gpu = info
            .video
            .as_ref()
            .filter(|_| gpu_filters)
            .map(playback::gpu::Source::of);
        let job = TranscodeJob {
            input: state.config.media_dir.join(&file.path),
            plan: plan.clone(),
            video_index: info.video.as_ref().map(|v| v.index),
            start_ms,
            encoder,
            gpu,
            source_width: info.video.as_ref().and_then(|v| v.width),
            source_hevc: info.video.as_ref().is_some_and(|v| v.codec == "hevc"),
        };
        let source_height = info.video.as_ref().and_then(|v| v.height);
        let session = state
            .transcodes
            .start(current.device_id, &job, source_height)
            .map_err(ApiError::Internal)?;
        (
            format!("/api/hls/{session}/index.m3u8"),
            true,
            Some(session),
        )
    };

    // Subtitles are timed against the stream: an HLS stream starts at `start_ms`.
    let offset = if hls { start_ms } else { 0 };
    let subtitle_url = selected_subtitle.filter(|s| s.text).and_then(|s| {
        let grant = if let Some(x) = s.id.strip_prefix('x') {
            Grant::External {
                x: x.parse().ok()?,
                o: offset,
            }
        } else {
            Grant::Subtitle {
                f: file.id,
                s: s.id.strip_prefix('s')?.parse().ok()?,
                o: offset,
            }
        };
        Some(format!(
            "/api/subtitles/{}",
            state.signer.sign(grant, expires)
        ))
    });

    // Apple's players only show subtitles listed inside an HLS stream (D88).
    if let (Some(session), Some(sub_url), true) =
        (&session_id, &subtitle_url, req.capabilities.hls_subtitles)
    {
        let language = selected_subtitle.and_then(|s| s.language.as_deref());
        let (master, subs) = playback::transcode::hls_playlists(
            sub_url,
            language,
            info.duration_ms.unwrap_or(0) - start_ms,
            plan.stream_kbps(info).unwrap_or(8_000),
        );
        state.transcodes.set_playlists(session, master, subs);
        url = format!("/api/hls/{session}/master.m3u8");
    }

    let next_episode_id = match req.kind {
        PlayableKind::Episode => {
            let (season, episode): (i64, i64) =
                sqlx::query_as("SELECT season_number, episode_number FROM episodes WHERE id = ?")
                    .bind(req.id)
                    .fetch_one(&state.db)
                    .await?;
            userdata::neighbours(&state.db, series_id.unwrap_or_default(), season, episode)
                .await?
                .1
        }
        PlayableKind::Movie => None,
    };

    let mut subtitles_list: Vec<SubtitleChoice> = external
        .iter()
        .map(|(x, name)| SubtitleChoice {
            id: format!("x{}", x.id),
            language: x.language.clone(),
            title: name.clone(),
            kind: x.kind,
            text: true,
            external: true,
        })
        .collect();
    subtitles_list.extend(info.subtitles.iter().map(|s| SubtitleChoice {
        id: format!("s{}", s.index),
        language: s.language.as_deref().and_then(normalize_language),
        title: s.title.clone(),
        kind: s.kind,
        text: s.text,
        external: false,
    }));

    Ok(Json(PlaybackInfo {
        method: plan.method,
        url,
        hls,
        session_id,
        start_ms,
        duration_ms: info.duration_ms,
        audio_tracks: info
            .audio
            .iter()
            .map(|a| AudioOption {
                index: a.index,
                language: a.language.as_deref().and_then(normalize_language),
                title: a.title.clone(),
                codec: a.codec.clone(),
                channels: a.channels,
                kind: a.kind,
            })
            .collect(),
        audio_index,
        subtitles: subtitles_list,
        subtitle: subtitle_id,
        subtitle_url,
        next_episode_id,
        quality: req.quality,
        qualities,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct StopRequest {
    pub session_id: String,
}

/// Stops an HLS session (leaving the player). Sessions also stop on their own when idle.
#[utoipa::path(post, path = "/api/playback/stop", tag = "playback", security(("bearer" = [])),
    request_body = StopRequest, responses((status = 204)))]
pub async fn stop(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(req): Json<StopRequest>,
) -> ApiResult<StatusCode> {
    if state.transcodes.owner(&req.session_id) == Some(current.device_id) {
        state.transcodes.stop(&req.session_id);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, IntoParams)]
pub struct TrickplayQuery {
    pub kind: PlayableKind,
    pub id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrickplayStatus {
    Ready,
    /// Being made: ask again in a few seconds.
    Pending,
    /// This file has none.
    Unavailable,
}

/// Timeline thumbnails (D99): `count` pictures of `width` × `height`, one every
/// `interval_ms`, laid out `columns` × `rows` per sheet, left to right then top to bottom.
/// Thumbnail `n` (at `n × interval_ms`) is in sheet `n / (columns × rows)`.
#[derive(Serialize, ToSchema)]
pub struct TrickplayInfo {
    pub status: TrickplayStatus,
    pub interval_ms: i64,
    pub width: i64,
    pub height: i64,
    pub columns: i64,
    pub rows: i64,
    pub count: i64,
    /// Signed URLs of the sheets (JPEG), in order. Empty unless `ready`.
    pub sheets: Vec<String>,
}

/// The timeline thumbnails of a movie or episode. Asking for them makes them (and those
/// of the next episode) when missing.
#[utoipa::path(get, path = "/api/playback/trickplay", tag = "playback", security(("bearer" = [])),
    params(TrickplayQuery), responses((status = 200, body = TrickplayInfo), (status = 404)))]
pub async fn trickplay(
    State(state): State<AppState>,
    _current: CurrentDevice,
    Query(q): Query<TrickplayQuery>,
) -> ApiResult<Json<TrickplayInfo>> {
    let file: i64 = sqlx::query_scalar(
        "SELECT id FROM media_files WHERE item_kind = ? AND item_id = ? AND missing_since IS NULL
         ORDER BY size_bytes DESC LIMIT 1",
    )
    .bind(q.kind.as_str())
    .bind(q.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let status = state.trickplay.status(file).await?;
    // Ready by the time the next episode starts.
    if q.kind == PlayableKind::Episode {
        let next: Option<i64> = sqlx::query_scalar(
            "SELECT mf.id FROM episodes cur
             JOIN episodes nxt ON nxt.series_id = cur.series_id
               AND (nxt.season_number, nxt.episode_number) > (cur.season_number, cur.episode_number)
             JOIN media_files mf ON mf.item_kind = 'episode' AND mf.item_id = nxt.id
               AND mf.missing_since IS NULL
             WHERE cur.id = ?
             ORDER BY nxt.season_number, nxt.episode_number, mf.size_bytes DESC LIMIT 1",
        )
        .bind(q.id)
        .fetch_optional(&state.db)
        .await?;
        if let Some(next) = next {
            state.trickplay.status(next).await?;
        }
    }
    let (status, count) = match status {
        trickplay::Status::Ready { count } => (TrickplayStatus::Ready, count),
        trickplay::Status::Pending => (TrickplayStatus::Pending, 0),
        trickplay::Status::Unavailable => (TrickplayStatus::Unavailable, 0),
    };
    let per_sheet = trickplay::COLUMNS * trickplay::ROWS;
    let token = state
        .signer
        .sign(Grant::Trickplay { t: file }, now() + URL_LIFETIME_SECS);
    Ok(Json(TrickplayInfo {
        status,
        interval_ms: trickplay::INTERVAL_MS,
        width: trickplay::WIDTH,
        height: trickplay::HEIGHT,
        columns: trickplay::COLUMNS,
        rows: trickplay::ROWS,
        count,
        sheets: (0..(count + per_sheet - 1) / per_sheet)
            .map(|n| format!("/api/trickplay/{token}/{n}.jpg"))
            .collect(),
    }))
}

/// A sheet of timeline thumbnails.
#[utoipa::path(get, path = "/api/trickplay/{token}/{sheet}", tag = "playback",
    params(("token" = String, Path), ("sheet" = String, Path, description = "`<n>.jpg`")),
    responses((status = 200, content_type = "image/jpeg"), (status = 404)))]
pub async fn trickplay_sheet(
    State(state): State<AppState>,
    Path((token, sheet)): Path<(String, String)>,
    request: Request,
) -> ApiResult<Response> {
    let Some(Grant::Trickplay { t }) = state.signer.verify(&token, now()) else {
        return Err(ApiError::NotFound);
    };
    let n: u32 = sheet
        .strip_suffix(".jpg")
        .and_then(|n| n.parse().ok())
        .ok_or(ApiError::NotFound)?;
    let response = ServeFile::new(state.trickplay.dir(t).join(format!("{n}.jpg")))
        .oneshot(request)
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("serving file: {e}")))?;
    let mut response = response.map(Body::new);
    // Sheets don't change for a given file version; a new version gets a new token anyway.
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, max-age=86400"),
    );
    Ok(response)
}

/// The original file, with HTTP Range support (seeking).
#[utoipa::path(get, path = "/api/stream/{token}", tag = "playback",
    params(("token" = String, Path)), responses((status = 200), (status = 206), (status = 404)))]
pub async fn stream(
    State(state): State<AppState>,
    Path(token): Path<String>,
    request: Request,
) -> ApiResult<Response> {
    let Some(Grant::File { f }) = state.signer.verify(&token, now()) else {
        return Err(ApiError::NotFound);
    };
    let path: String =
        sqlx::query_scalar("SELECT path FROM media_files WHERE id = ? AND missing_since IS NULL")
            .bind(f)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    let response = ServeFile::new(state.config.media_dir.join(path))
        .oneshot(request)
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("serving file: {e}")))?;
    Ok(response.map(Body::new))
}

/// A subtitle as WebVTT. Extraction is cached; the time shift is applied per request.
#[utoipa::path(get, path = "/api/subtitles/{token}", tag = "playback",
    params(("token" = String, Path)), responses((status = 200, content_type = "text/vtt"), (status = 404)))]
pub async fn subtitle(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Response> {
    let grant = state
        .signer
        .verify(&token, now())
        .ok_or(ApiError::NotFound)?;
    let (cache_name, input, stream, offset) = match grant {
        Grant::Subtitle { f, s, o } => {
            let path: String = sqlx::query_scalar("SELECT path FROM media_files WHERE id = ?")
                .bind(f)
                .fetch_optional(&state.db)
                .await?
                .ok_or(ApiError::NotFound)?;
            (format!("{f}_{s}"), path, Some(s), o)
        }
        Grant::External { x, o } => {
            let path: String = sqlx::query_scalar("SELECT path FROM subtitle_files WHERE id = ?")
                .bind(x)
                .fetch_optional(&state.db)
                .await?
                .ok_or(ApiError::NotFound)?;
            (format!("x{x}"), path, None, o)
        }
        Grant::File { .. } | Grant::Variant { .. } | Grant::Trickplay { .. } => {
            return Err(ApiError::NotFound);
        }
    };
    let input = state.config.media_dir.join(input);
    // The file's date is part of the name: a replaced movie or a corrected .srt must
    // not keep serving the subtitles of the old one.
    let modified = tokio::fs::metadata(&input)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let cache = state
        .config
        .cache_dir
        .join("subtitles")
        .join(format!("{cache_name}_{modified}.vtt"));
    let vtt = match tokio::fs::read_to_string(&cache).await {
        Ok(vtt) => vtt,
        Err(_) => {
            // In a task of its own: reading a big file takes minutes, and it must go on
            // (and be kept) if this request gives up. One at a time per subtitle.
            let ffmpeg = state.config.ffmpeg.clone();
            tokio::spawn(async move {
                let lock = subtitle_lock(&cache);
                let _guard = lock.lock().await;
                if let Ok(vtt) = tokio::fs::read_to_string(&cache).await {
                    return Ok(vtt);
                }
                let vtt = subtitles::to_webvtt(&ffmpeg, &input, stream).await?;
                if let Some(dir) = cache.parent() {
                    tokio::fs::create_dir_all(dir).await?;
                }
                // Write then rename: a concurrent reader never sees a partial file.
                let tmp = cache.with_extension("tmp");
                tokio::fs::write(&tmp, &vtt).await?;
                tokio::fs::rename(&tmp, &cache).await?;
                anyhow::Ok(vtt)
            })
            .await
            .map_err(anyhow::Error::from)?
            .map_err(ApiError::Internal)?
        }
    };
    Ok((
        [
            (header::CONTENT_TYPE, "text/vtt; charset=utf-8"),
            (header::CACHE_CONTROL, "private, max-age=3600"),
        ],
        subtitles::shift_webvtt(&vtt, offset),
    )
        .into_response())
}

/// A file of an HLS session. Waits for ffmpeg to produce it.
#[utoipa::path(get, path = "/api/hls/{session}/{file}", tag = "playback",
    params(("session" = String, Path), ("file" = String, Path)), responses((status = 200), (status = 404)))]
pub async fn hls(
    State(state): State<AppState>,
    Path((session, file)): Path<(String, String)>,
) -> ApiResult<Response> {
    if !valid_hls_file(&file) {
        return Err(ApiError::NotFound);
    }
    let dir = state.transcodes.touch(&session).ok_or(ApiError::NotFound)?;
    if file == "master.m3u8" || file == "subs.m3u8" {
        let playlist = state
            .transcodes
            .playlist(&session, &file)
            .ok_or(ApiError::NotFound)?;
        return Ok((
            [
                (header::CONTENT_TYPE, "application/vnd.apple.mpegurl"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            playlist,
        )
            .into_response());
    }
    let path = dir.join(&file);
    let is_playlist = file == "index.m3u8";
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Ok(bytes) = tokio::fs::read(&path).await {
            // The playlist is only useful once it lists a segment.
            let ready = !is_playlist || bytes.windows(7).any(|w| w == b"#EXTINF");
            if ready {
                let content_type = match file.rsplit_once('.').map(|(_, e)| e) {
                    Some("m3u8") => "application/vnd.apple.mpegurl",
                    _ => "video/mp4",
                };
                let cache = if is_playlist {
                    "no-cache"
                } else {
                    "private, max-age=3600"
                };
                return Ok((
                    [
                        (header::CONTENT_TYPE, content_type),
                        (header::CACHE_CONTROL, cache),
                    ],
                    bytes,
                )
                    .into_response());
            }
        }
        if Instant::now() > deadline
            || (!state.transcodes.is_running(&session) && !state.transcodes.retry_on_cpu(&session))
        {
            return Err(ApiError::NotFound);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        state.transcodes.touch(&session);
    }
}

/// One lock per subtitle being converted, so that two requests for the same one don't
/// both read the whole file.
fn subtitle_lock(cache: &std::path::Path) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    static LOCKS: std::sync::Mutex<
        Option<
            std::collections::HashMap<std::path::PathBuf, std::sync::Arc<tokio::sync::Mutex<()>>>,
        >,
    > = std::sync::Mutex::new(None);
    let mut locks = LOCKS.lock().unwrap();
    let locks = locks.get_or_insert_default();
    // Those nobody holds any more.
    locks.retain(|_, lock| std::sync::Arc::strong_count(lock) > 1);
    locks.entry(cache.to_owned()).or_default().clone()
}
