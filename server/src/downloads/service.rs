//! Loads a device's inputs from the database, and turns expected items into
//! downloadable files (variants) with their state.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    encode::{self, Encoders, VariantSubtitle},
    plan::{
        self, EpisodeRef, ExpectedItem, Inputs, ItemState, ManualEntry, Mode, Profile,
        Subscription, WatchlistEntry, WatchlistTarget,
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
    /// What may download over mobile data (D68).
    pub mobile_data: MobileData,
    /// Monthly mobile data cap, counted by the device.
    pub mobile_cap_bytes: Option<i64>,
    /// Android: app storage or SD card.
    pub storage: Storage,
    /// Download only while the device charges (D69).
    pub charging_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum MobileData {
    /// Wi-Fi only.
    Off,
    /// Lighter versions (`mobile`, `saver`) over mobile data; `max` waits for Wi-Fi.
    Light,
    /// Every quality over mobile data.
    All,
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
            Platform::Android | Platform::AndroidTv | Platform::Ios => Profile::Mobile,
            _ => Profile::Max,
        },
        // Desktop: 100 GB (D48); Android and iOS: 20 GB (D70, D86).
        quota_bytes: match platform {
            Platform::Desktop => Some(100 * 1_000_000_000),
            Platform::Android | Platform::Ios => Some(20 * 1_000_000_000),
            _ => None,
        },
        mobile_data: MobileData::Off,
        mobile_cap_bytes: None,
        storage: Storage::Internal,
        charging_only: false,
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
    #[derive(sqlx::FromRow)]
    struct Row {
        #[sqlx(flatten)]
        settings: DeviceSettings,
        av1: bool,
    }
    let row: Option<(DeviceSettings, bool)> = sqlx::query_as::<_, Row>(
        "SELECT auto_download, profile, quota_bytes, mobile_data, mobile_cap_bytes, storage,
           charging_only, av1
         FROM device_settings WHERE device_id = ?",
    )
    .bind(device_id)
    .fetch_optional(&state.db)
    .await?
    .map(|r| (r.settings, r.av1));
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
           mobile_cap_bytes, storage, av1, updated_at, charging_only)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, COALESCE(?8, 0), ?9, ?10)
         ON CONFLICT (device_id) DO UPDATE SET auto_download = excluded.auto_download,
           profile = excluded.profile, quota_bytes = excluded.quota_bytes,
           mobile_data = excluded.mobile_data, mobile_cap_bytes = excluded.mobile_cap_bytes,
           storage = excluded.storage, av1 = COALESCE(?8, device_settings.av1),
           updated_at = excluded.updated_at, charging_only = excluded.charging_only",
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
    .bind(s.charging_only)
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

/// Episodes of a series in viewing order (specials last), loaded once.
async fn load_episodes(state: &AppState, inputs: &mut Inputs, series_id: i64) -> ApiResult<()> {
    if inputs.series_episodes.contains_key(&series_id) {
        return Ok(());
    }
    let episodes: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT id, season_number FROM episodes WHERE series_id = ? AND missing_since IS NULL
         ORDER BY season_number = 0, season_number, episode_number",
    )
    .bind(series_id)
    .fetch_all(&state.db)
    .await?;
    inputs.series_episodes.insert(
        series_id,
        episodes
            .into_iter()
            .map(|(id, season)| EpisodeRef { id, season })
            .collect(),
    );
    Ok(())
}

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
        load_episodes(state, &mut inputs, series_id).await?;
    }

    // Watchlist movies, then series with the series' audio choice (D20, D73).
    let movies = sqlx::query_as::<_, (i64, Option<String>, i64)>(
        "SELECT w.movie_id, w.audio_language, w.added_at FROM watchlist w
         JOIN movies m ON m.id = w.movie_id AND m.missing_since IS NULL WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let series = sqlx::query_as::<_, (i64, Option<String>, i64)>(
        "SELECT w.series_id,
           (SELECT p.audio_language FROM series_track_preferences p
            WHERE p.user_id = w.user_id AND p.series_id = w.series_id),
           w.added_at
         FROM watchlist_series w JOIN series s ON s.id = w.series_id AND s.missing_since IS NULL
         WHERE w.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    inputs.watchlist = movies
        .into_iter()
        .map(|(id, audio_language, added_at)| WatchlistEntry {
            target: WatchlistTarget::Movie(id),
            audio_language,
            added_at,
        })
        .collect();
    for (series_id, audio_language, added_at) in series {
        load_episodes(state, &mut inputs, series_id).await?;
        inputs.watchlist.push(WatchlistEntry {
            target: WatchlistTarget::Series(series_id),
            audio_language,
            added_at,
        });
    }
    let count: i64 = sqlx::query_scalar("SELECT watchlist_episodes FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    inputs.watchlist_episodes = count.clamp(1, 10) as usize;

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
        .and_then(|guard| guard.as_ref().map(Encoders::from_info))
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
    // Requests for items it no longer expects (removed from the watchlist, subscription
    // stopped from the web): withdrawn, and their encodes stop if nobody else waits.
    let stale = sqlx::query(
        "DELETE FROM variant_requests WHERE device_id = ? AND confirmed_at IS NULL
         AND variant_id IN (SELECT v.id FROM variants v JOIN media_files mf ON mf.id = v.media_file_id
             WHERE (mf.item_kind || ':' || mf.item_id) NOT IN (SELECT value FROM json_each(?)))",
    )
    .bind(device_id)
    .bind(serde_json::to_string(&keep).expect("serializable"))
    .execute(&state.db)
    .await?;
    if stale.rows_affected() > 0 {
        drop_unwanted_encodes(state).await?;
    }
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
                created_at: i64,
                confirmed: Option<i64>,
            }
            let variant: Option<V> = sqlx::query_as(
                "SELECT v.id, v.status, v.progress, v.size_bytes, v.subtitles_json, v.error, v.created_at,
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
                            state.signer.sign(
                                Grant::Variant {
                                    v: v.id,
                                    f: 0,
                                    c: v.created_at,
                                },
                                expires,
                            )
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
                                            f: i as i64 + 1,
                                            c: v.created_at,
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
         ON CONFLICT (media_file_id, profile, codec, COALESCE(audio_index, -1)) DO UPDATE SET
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
    // The version it asked for before (another profile, codec or audio track since):
    // withdrawn, or that request would keep its encode in the cache for ever.
    let replaced = sqlx::query(
        "DELETE FROM variant_requests WHERE device_id = ?1 AND confirmed_at IS NULL
           AND variant_id != ?2 AND variant_id IN (
             SELECT v.id FROM variants v JOIN media_files mf ON mf.id = v.media_file_id
             WHERE mf.item_kind = ?3 AND mf.item_id = ?4)",
    )
    .bind(device_id)
    .bind(variant_id)
    .bind(kind.as_str())
    .bind(id)
    .execute(&state.db)
    .await?;
    if replaced.rows_affected() > 0 {
        drop_unwanted_encodes(state).await?;
    }
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

/// Devices signed out or revoked never ask for their plan again: their pending requests
/// would keep encodes running and ready versions in the cache for ever.
pub async fn release_revoked_devices(state: &AppState) -> ApiResult<()> {
    sqlx::query(
        "DELETE FROM variant_requests WHERE confirmed_at IS NULL
           AND device_id IN (SELECT id FROM devices WHERE revoked_at IS NOT NULL)",
    )
    .execute(&state.db)
    .await?;
    drop_unwanted_encodes(state).await?;
    state.encoder.enforce_budget().await?;
    Ok(())
}

/// Queued or running encodes no device asks for any more: stopped and deleted, so a
/// cancelled download doesn't keep the GPU busy.
async fn drop_unwanted_encodes(state: &AppState) -> ApiResult<()> {
    let orphans: Vec<i64> = sqlx::query_scalar(
        "SELECT v.id FROM variants v WHERE v.status IN ('queued', 'encoding')
           AND NOT EXISTS (SELECT 1 FROM variant_requests r WHERE r.variant_id = v.id)",
    )
    .fetch_all(&state.db)
    .await?;
    for variant in orphans {
        state.encoder.abort(variant);
        state.encoder.delete(variant).await?;
    }
    Ok(())
}

/// The device drops an item (D74): removes each source that asks for it on this
/// device (manual download, subscription, watchlist entry) until none does, then
/// withdraws the device's pending request. An encode nobody else waits for stops.
pub async fn cancel(
    state: &AppState,
    user: &crate::auth::User,
    device_id: i64,
    kind: PlayableKind,
    id: i64,
) -> ApiResult<Vec<plan::Source>> {
    let platform = download_device(state, user.id, device_id).await?;
    let (settings, _) = load_settings(state, device_id, platform).await?;
    let mut removed = Vec::new();
    // Each pass removes one source; an item has at most four (manual, rolling, season,
    // watchlist), so the bound is never reached unless a source can't be removed.
    for _ in 0..4 {
        let inputs = load_inputs(state, user.id, device_id, &settings).await?;
        let Some(item) = plan::expected_set(&inputs)
            .into_iter()
            .find(|i| i.kind == kind && i.id == id)
        else {
            break;
        };
        remove_source(state, user.id, device_id, kind, id, item.source).await?;
        removed.push(item.source);
    }

    sqlx::query(
        "DELETE FROM variant_requests WHERE device_id = ? AND confirmed_at IS NULL
         AND variant_id IN (SELECT v.id FROM variants v JOIN media_files mf ON mf.id = v.media_file_id
                            WHERE mf.item_kind = ? AND mf.item_id = ?)",
    )
    .bind(device_id)
    .bind(kind.as_str())
    .bind(id)
    .execute(&state.db)
    .await?;
    drop_unwanted_encodes(state).await?;
    sqlx::query(
        "DELETE FROM device_downloads WHERE device_id = ? AND item_kind = ? AND item_id = ?",
    )
    .bind(device_id)
    .bind(kind.as_str())
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(removed)
}

async fn remove_source(
    state: &AppState,
    user_id: i64,
    device_id: i64,
    kind: PlayableKind,
    id: i64,
    source: plan::Source,
) -> ApiResult<()> {
    use plan::Source;
    let episode: Option<(i64, i64)> = if kind == PlayableKind::Episode {
        sqlx::query_as("SELECT series_id, season_number FROM episodes WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
    } else {
        None
    };
    let query = match (source, episode) {
        (Source::Manual, _) => sqlx::query(
            "DELETE FROM manual_downloads WHERE device_id = ? AND item_kind = ? AND item_id = ?",
        )
        .bind(device_id)
        .bind(kind.as_str())
        .bind(id),
        (Source::Rolling, Some((series_id, _))) => sqlx::query(
            "DELETE FROM series_subscriptions WHERE device_id = ? AND series_id = ? AND mode = 'rolling'",
        )
        .bind(device_id)
        .bind(series_id),
        (Source::Season, Some((series_id, season))) => sqlx::query(
            "DELETE FROM series_subscriptions WHERE device_id = ? AND series_id = ?
             AND mode = 'season' AND season_number = ?",
        )
        .bind(device_id)
        .bind(series_id)
        .bind(season),
        (Source::Watchlist, Some((series_id, _))) => {
            sqlx::query("DELETE FROM watchlist_series WHERE user_id = ? AND series_id = ?")
                .bind(user_id)
                .bind(series_id)
        }
        (Source::Watchlist, None) => {
            sqlx::query("DELETE FROM watchlist WHERE user_id = ? AND movie_id = ?")
                .bind(user_id)
                .bind(id)
        }
        (Source::Rolling | Source::Season, None) => return Ok(()),
    };
    query.execute(&state.db).await?;
    Ok(())
}
