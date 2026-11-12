//! Loads a device's inputs from the database, and turns expected items into
//! downloadable files (variants) with their state.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    encode::{self, Encoders, VariantSubtitle},
    plan::{
        self, EpisodeRef, ExpectedItem, Inputs, ItemState, ManualEntry, Mode, Profile,
        Subscription, WatchlistEntry,
    },
};
use crate::{
    AppState,
    auth::Platform,
    db::now,
    error::{ApiError, ApiResult},
    library::media_info::{self, TrackKind},
    playback::signing::Grant,
    userdata::PlayableKind,
};

const URL_LIFETIME_SECS: i64 = 12 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct DeviceSettings {
    /// Download watchlist movies automatically.
    pub auto_download: bool,
    pub profile: Profile,
    /// Space downloads may use on the device; `None` = no limit.
    pub quota_bytes: Option<i64>,
    /// Allow downloads over mobile data (lighter versions only).
    pub mobile_data: bool,
    /// Monthly mobile data cap.
    pub mobile_cap_bytes: Option<i64>,
    /// Android: app storage or SD card.
    pub storage: Storage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Storage {
    Internal,
    Sd,
}

pub fn default_settings(platform: Platform) -> DeviceSettings {
    DeviceSettings {
        auto_download: false,
        profile: match platform {
            Platform::Android | Platform::AndroidTv => Profile::Mobile,
            _ => Profile::Max,
        },
        // Desktop: 100 GB (D48). Android picks its own once it knows the free space.
        quota_bytes: match platform {
            Platform::Desktop => Some(100 * 1_000_000_000),
            _ => None,
        },
        mobile_data: false,
        mobile_cap_bytes: None,
        storage: Storage::Internal,
    }
}

/// A device of `user_id` that can download (not a browser). 404 otherwise.
pub async fn download_device(
    state: &AppState,
    user_id: i64,
    device_id: i64,
) -> ApiResult<Platform> {
    let platform: Option<Platform> = sqlx::query_scalar(
        "SELECT platform FROM devices WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
    )
    .bind(device_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;
    match platform {
        Some(Platform::Web) => Err(ApiError::bad_request(
            "not_a_download_device",
            "browsers can't download: use the desktop or Android app",
        )),
        Some(platform) => Ok(platform),
        None => Err(ApiError::NotFound),
    }
}

pub async fn load_settings(
    state: &AppState,
    device_id: i64,
    platform: Platform,
) -> ApiResult<(DeviceSettings, bool)> {
    let row: Option<(DeviceSettings, bool)> = sqlx::query_as::<
        _,
        (bool, Profile, Option<i64>, bool, Option<i64>, Storage, bool),
    >(
        "SELECT auto_download, profile, quota_bytes, mobile_data, mobile_cap_bytes, storage, av1
         FROM device_settings WHERE device_id = ?",
    )
    .bind(device_id)
    .fetch_optional(&state.db)
    .await?
    .map(
        |(auto_download, profile, quota_bytes, mobile_data, mobile_cap_bytes, storage, av1)| {
            (
                DeviceSettings {
                    auto_download,
                    profile,
                    quota_bytes,
                    mobile_data,
                    mobile_cap_bytes,
                    storage,
                },
                av1,
            )
        },
    );
    Ok(row.unwrap_or_else(|| (default_settings(platform), false)))
}

pub async fn save_settings(
    state: &AppState,
    device_id: i64,
    s: &DeviceSettings,
    av1: Option<bool>,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO device_settings (device_id, auto_download, profile, quota_bytes, mobile_data,
           mobile_cap_bytes, storage, av1, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, COALESCE(?8, 0), ?9)
         ON CONFLICT (device_id) DO UPDATE SET auto_download = excluded.auto_download,
           profile = excluded.profile, quota_bytes = excluded.quota_bytes,
           mobile_data = excluded.mobile_data, mobile_cap_bytes = excluded.mobile_cap_bytes,
           storage = excluded.storage, av1 = COALESCE(?8, device_settings.av1),
           updated_at = excluded.updated_at",
    )
    .bind(device_id)
    .bind(s.auto_download)
    .bind(s.profile)
    .bind(s.quota_bytes)
    .bind(s.mobile_data)
    .bind(s.mobile_cap_bytes)
    .bind(s.storage)
    .bind(av1)
    .bind(now())
    .execute(&state.db)
    .await?;
    Ok(())
}

/// series_id, mode, count, season_number, profile, audio_language
type SubscriptionRow = (
    i64,
    String,
    Option<i64>,
    Option<i64>,
    Option<Profile>,
    Option<String>,
);

/// Everything the expected-set computation needs for this device.
pub async fn load_inputs(
    state: &AppState,
    user_id: i64,
    device_id: i64,
    settings: &DeviceSettings,
) -> ApiResult<Inputs> {
    let mut inputs = Inputs {
        now: now(),
        auto_download: settings.auto_download,
        device_profile: Some(settings.profile),
        ..Default::default()
    };

    let subs: Vec<SubscriptionRow> = sqlx::query_as(
        "SELECT series_id, mode, count, season_number, profile, audio_language
         FROM series_subscriptions WHERE device_id = ? ORDER BY created_at, id",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await?;
    for (series_id, mode, count, season, profile, audio_language) in subs {
        let mode = if mode == "season" {
            Mode::Season {
                number: season.unwrap_or(1),
            }
        } else {
            Mode::Rolling {
                count: count.unwrap_or(3).max(1) as usize,
            }
        };
        inputs.subscriptions.push(Subscription {
            series_id,
            mode,
            profile,
            audio_language,
        });
        if let std::collections::hash_map::Entry::Vacant(slot) =
            inputs.series_episodes.entry(series_id)
        {
            let episodes: Vec<(i64, i64)> = sqlx::query_as(
                "SELECT id, season_number FROM episodes WHERE series_id = ? AND missing_since IS NULL
                 ORDER BY season_number = 0, season_number, episode_number",
            )
            .bind(series_id)
            .fetch_all(&state.db)
            .await?;
            slot.insert(
                episodes
                    .into_iter()
                    .map(|(id, season)| EpisodeRef { id, season })
                    .collect(),
            );
        }
    }

    inputs.watchlist = sqlx::query_as::<_, (i64, Option<String>, i64)>(
        "SELECT w.movie_id, w.audio_language, w.added_at FROM watchlist w
         JOIN movies m ON m.id = w.movie_id AND m.missing_since IS NULL WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(movie_id, audio_language, added_at)| WatchlistEntry {
        movie_id,
        audio_language,
        added_at,
    })
    .collect();

    inputs.manual = sqlx::query_as::<_, (String, i64, Option<Profile>, Option<String>)>(
        "SELECT item_kind, item_id, profile, audio_language FROM manual_downloads
         WHERE device_id = ? ORDER BY created_at, id",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(kind, id, profile, audio_language)| ManualEntry {
        kind: if kind == "movie" {
            PlayableKind::Movie
        } else {
            PlayableKind::Episode
        },
        id,
        profile,
        audio_language,
    })
    .collect();

    let states: Vec<(String, i64, bool, i64, Option<i64>)> = sqlx::query_as(
        "SELECT item_kind, item_id, played, position_ms, last_played_at FROM user_item_data WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    inputs.states = states
        .into_iter()
        .map(|(kind, id, played, position_ms, last_played_at)| {
            let kind = if kind == "movie" {
                PlayableKind::Movie
            } else {
                PlayableKind::Episode
            };
            (
                (kind, id),
                ItemState {
                    played,
                    position_ms,
                    last_played_at,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    Ok(inputs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    /// Not asked for yet: the device requests it when it has room and network.
    NotRequested,
    Queued,
    Encoding,
    /// Encoded: download `url`.
    Ready,
    Failed,
    /// The device confirmed it has the file.
    Downloaded,
    /// The file can't be read (never analyzed).
    Unavailable,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SubtitleFile {
    pub url: String,
    pub language: Option<String>,
    pub kind: TrackKind,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlanItem {
    #[serde(flatten)]
    pub item: ExpectedItem,
    /// Movie title, or series title for an episode.
    pub title: String,
    pub episode_title: Option<String>,
    pub series_id: Option<i64>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub poster_path: Option<String>,
    pub duration_ms: Option<i64>,
    pub status: DownloadStatus,
    pub variant_id: Option<i64>,
    /// Encoding progress, 0 to 1.
    pub progress: f64,
    pub size_bytes: Option<i64>,
    pub url: Option<String>,
    pub subtitles: Vec<SubtitleFile>,
    pub error: Option<String>,
}

/// The variant (identity) an expected item maps to.
pub struct VariantKey {
    pub media_file_id: i64,
    pub profile: Profile,
    pub codec: String,
    pub audio_index: Option<i64>,
}

pub fn encoders(state: &AppState) -> Encoders {
    state
        .ffmpeg
        .try_read()
        .ok()
        .and_then(|guard| {
            guard
                .as_ref()
                .map(|i| Encoders::from_names(&i.working_encoders))
        })
        .unwrap_or_default()
}

async fn variant_key(
    state: &AppState,
    item: &ExpectedItem,
    preferred_audio: Option<&str>,
    device_av1: bool,
) -> ApiResult<Option<VariantKey>> {
    let row: Option<(i64, Option<String>)> = sqlx::query_as(
        "SELECT id, probe_json FROM media_files WHERE item_kind = ? AND item_id = ? AND missing_since IS NULL
         ORDER BY size_bytes DESC LIMIT 1",
    )
    .bind(item.kind.as_str())
    .bind(item.id)
    .fetch_optional(&state.db)
    .await?;
    let Some((media_file_id, Some(probe))) = row else {
        return Ok(None);
    };
    let Ok(info) = media_info::parse(&probe) else {
        return Ok(None);
    };
    let Some(video) = info.video.as_ref() else {
        return Ok(None);
    };
    let plan = encode::video_plan(item.profile, &video.codec, device_av1, &encoders(state));
    Ok(Some(VariantKey {
        media_file_id,
        profile: item.profile,
        codec: encode::codec_label(plan, &video.codec),
        audio_index: encode::pick_audio(&info, item.audio_language.as_deref().or(preferred_audio)),
    }))
}

/// The device's expected set, with the download state of each item.
pub async fn plan_for_device(
    state: &AppState,
    user: &crate::auth::User,
    device_id: i64,
) -> ApiResult<(DeviceSettings, Vec<PlanItem>)> {
    let platform = download_device(state, user.id, device_id).await?;
    let (settings, av1) = load_settings(state, device_id, platform).await?;
    let inputs = load_inputs(state, user.id, device_id, &settings).await?;
    let expected = plan::expected_set(&inputs);
    let prefs = crate::routes::home::load_preferences(state, user.id).await?;
    let expires = now() + URL_LIFETIME_SECS;

    // Forget downloads of items the device no longer has to keep.
    let keep: Vec<String> = expected
        .iter()
        .map(|i| format!("{}:{}", i.kind.as_str(), i.id))
        .collect();
    sqlx::query(
        "DELETE FROM device_downloads WHERE device_id = ?
         AND (item_kind || ':' || item_id) NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(device_id)
    .bind(serde_json::to_string(&keep).expect("serializable"))
    .execute(&state.db)
    .await?;
    let downloaded: HashMap<(String, i64), (Profile, Option<i64>)> = sqlx::query_as::<
        _,
        (String, i64, Profile, Option<i64>),
    >(
        "SELECT item_kind, item_id, profile, size_bytes FROM device_downloads WHERE device_id = ?",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(k, id, p, size)| ((k, id), (p, size)))
    .collect();

    let mut items = Vec::with_capacity(expected.len());
    for item in expected {
        let card = crate::routes::home::item_card(state, user, item.kind, item.id).await?;
        let mut out = PlanItem {
            title: card.title,
            episode_title: card.subtitle,
            series_id: card.series_id,
            season_number: card.season_number,
            episode_number: card.episode_number,
            poster_path: card.poster_path,
            duration_ms: card.duration_ms,
            item,
            status: DownloadStatus::Unavailable,
            variant_id: None,
            progress: 0.0,
            size_bytes: None,
            url: None,
            subtitles: vec![],
            error: None,
        };
        if let Some((profile, size)) =
            downloaded.get(&(out.item.kind.as_str().to_owned(), out.item.id))
            && *profile == out.item.profile
        {
            out.status = DownloadStatus::Downloaded;
            out.size_bytes = *size;
            out.progress = 1.0;
            items.push(out);
            continue;
        }
        if let Some(key) =
            variant_key(state, &out.item, prefs.audio_language.as_deref(), av1).await?
        {
            #[derive(sqlx::FromRow)]
            struct V {
                id: i64,
                status: String,
                progress: f64,
                size_bytes: Option<i64>,
                subtitles_json: Option<String>,
                error: Option<String>,
                confirmed: Option<i64>,
            }
            let variant: Option<V> = sqlx::query_as(
                "SELECT v.id, v.status, v.progress, v.size_bytes, v.subtitles_json, v.error,
                   (SELECT confirmed_at FROM variant_requests r WHERE r.variant_id = v.id AND r.device_id = ?5)
                     AS confirmed
                 FROM variants v WHERE v.media_file_id = ?1 AND v.profile = ?2 AND v.codec = ?3
                   AND v.audio_index IS ?4",
            )
            .bind(key.media_file_id)
            .bind(key.profile)
            .bind(&key.codec)
            .bind(key.audio_index)
            .bind(device_id)
            .fetch_optional(&state.db)
            .await?;
            match variant {
                None => out.status = DownloadStatus::NotRequested,
                Some(v) => {
                    out.variant_id = Some(v.id);
                    out.progress = v.progress;
                    out.size_bytes = v.size_bytes;
                    out.error = v.error;
                    out.status = match (v.status.as_str(), v.confirmed) {
                        (_, Some(_)) => DownloadStatus::Downloaded,
                        ("queued", _) => DownloadStatus::Queued,
                        ("encoding", _) => DownloadStatus::Encoding,
                        ("ready", _) => DownloadStatus::Ready,
                        _ => DownloadStatus::Failed,
                    };
                    if out.status == DownloadStatus::Ready {
                        out.url = Some(format!(
                            "/api/downloads/file/{}",
                            state.signer.sign(Grant::Variant { v: v.id, f: 0 }, expires)
                        ));
                        let subs: Vec<VariantSubtitle> = v
                            .subtitles_json
                            .as_deref()
                            .and_then(|j| serde_json::from_str(j).ok())
                            .unwrap_or_default();
                        out.subtitles = subs
                            .into_iter()
                            .enumerate()
                            .map(|(i, s)| SubtitleFile {
                                url: format!(
                                    "/api/downloads/file/{}",
                                    state.signer.sign(
                                        Grant::Variant {
                                            v: v.id,
                                            f: i as i64 + 1
                                        },
                                        expires
                                    )
                                ),
                                language: s.language,
                                kind: s.kind,
                            })
                            .collect();
                    }
                }
            }
        }
        items.push(out);
    }
    Ok((settings, items))
}

/// Queues the variant of an expected item for this device (encoded only on request).
pub async fn request(
    state: &AppState,
    user: &crate::auth::User,
    device_id: i64,
    kind: PlayableKind,
    id: i64,
) -> ApiResult<i64> {
    let platform = download_device(state, user.id, device_id).await?;
    let (settings, av1) = load_settings(state, device_id, platform).await?;
    let inputs = load_inputs(state, user.id, device_id, &settings).await?;
    let item = plan::expected_set(&inputs)
        .into_iter()
        .find(|i| i.kind == kind && i.id == id)
        .ok_or_else(|| {
            ApiError::conflict("not_expected", "this item isn't in the device's downloads")
        })?;
    let prefs = crate::routes::home::load_preferences(state, user.id).await?;
    let key = variant_key(state, &item, prefs.audio_language.as_deref(), av1)
        .await?
        .ok_or_else(|| ApiError::conflict("unreadable_file", "the file hasn't been analyzed"))?;
    let variant_id: i64 = sqlx::query_scalar(
        "INSERT INTO variants (media_file_id, profile, codec, audio_index, status, created_at)
         VALUES (?, ?, ?, ?, 'queued', ?)
         ON CONFLICT (media_file_id, profile, codec, audio_index) DO UPDATE SET
           status = CASE WHEN variants.status = 'failed' THEN 'queued' ELSE variants.status END,
           error = CASE WHEN variants.status = 'failed' THEN NULL ELSE variants.error END
         RETURNING id",
    )
    .bind(key.media_file_id)
    .bind(key.profile)
    .bind(&key.codec)
    .bind(key.audio_index)
    .bind(now())
    .fetch_one(&state.db)
    .await?;
    sqlx::query(
        "INSERT INTO variant_requests (variant_id, device_id, requested_at) VALUES (?, ?, ?)
         ON CONFLICT (variant_id, device_id) DO UPDATE SET confirmed_at = NULL",
    )
    .bind(variant_id)
    .bind(device_id)
    .bind(now())
    .execute(&state.db)
    .await?;
    state.encoder.notify();
    Ok(variant_id)
}

/// The device has the file: once every requesting device has it, the cache copy goes.
pub async fn confirm(state: &AppState, device_id: i64, variant_id: i64) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO device_downloads (device_id, item_kind, item_id, profile, size_bytes, downloaded_at)
         SELECT r.device_id, mf.item_kind, mf.item_id, v.profile, v.size_bytes, ?
         FROM variant_requests r JOIN variants v ON v.id = r.variant_id
         JOIN media_files mf ON mf.id = v.media_file_id
         WHERE r.variant_id = ? AND r.device_id = ?
         ON CONFLICT (device_id, item_kind, item_id) DO UPDATE SET profile = excluded.profile,
           size_bytes = excluded.size_bytes, downloaded_at = excluded.downloaded_at",
    )
    .bind(now())
    .bind(variant_id)
    .bind(device_id)
    .execute(&state.db)
    .await?;
    let updated = sqlx::query(
        "UPDATE variant_requests SET confirmed_at = ? WHERE variant_id = ? AND device_id = ?",
    )
    .bind(now())
    .bind(variant_id)
    .bind(device_id)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    let pending: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM variant_requests WHERE variant_id = ? AND confirmed_at IS NULL)",
    )
    .bind(variant_id)
    .fetch_one(&state.db)
    .await?;
    if !pending {
        state.encoder.delete(variant_id).await?;
    }
    Ok(())
}
