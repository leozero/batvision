//! Versions (D102): a movie or episode with several files (a 1080p and a 4K copy, a
//! director's cut). The largest file is the default; a user's other choice is remembered
//! per item and applies on every device (their resume position is per item too).

use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    db::{Db, now},
    error::{ApiError, ApiResult},
    library::media_info::{self, MediaInfo},
    playback::normalize_language,
};

/// One file of a movie or episode.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct Version {
    /// The media file id, to pass to `/api/playback` as `media_file_id`.
    pub id: i64,
    /// A short description, e.g. `4K · HDR · HEVC`; the file name until it is analyzed.
    pub label: String,
    pub file_name: String,
    pub size_bytes: i64,
    pub duration_ms: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    pub hdr: bool,
    /// Languages of the audio tracks (ISO 639-2, as in `PlaybackInfo`), without repeats.
    pub audio_languages: Vec<String>,
    /// The largest file, played unless the user picks another.
    pub default: bool,
    /// The one this user plays: their choice, else the default.
    pub selected: bool,
}

/// The file `user_id` plays for an item: their choice if it is still there, else the
/// largest one. Without a user: the largest one.
pub async fn file_for(
    db: &Db,
    user_id: Option<i64>,
    kind: &str,
    item_id: i64,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(
        "SELECT mf.id FROM media_files mf
         WHERE mf.item_kind = ?1 AND mf.item_id = ?2 AND mf.missing_since IS NULL
         ORDER BY COALESCE(mf.id = (SELECT c.media_file_id FROM version_choices c
             WHERE c.user_id = ?3 AND c.item_kind = ?1 AND c.item_id = ?2), 0) DESC,
           mf.size_bytes DESC, mf.id
         LIMIT 1",
    )
    .bind(kind)
    .bind(item_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// A `media_files` row: id, path, size, duration, probe.
type FileRow = (i64, String, i64, Option<i64>, Option<String>);

/// The versions of an item, largest first.
pub async fn list(db: &Db, user_id: i64, kind: &str, item_id: i64) -> sqlx::Result<Vec<Version>> {
    let rows: Vec<FileRow> = sqlx::query_as(
        "SELECT id, path, size_bytes, duration_ms, probe_json FROM media_files
         WHERE item_kind = ? AND item_id = ? AND missing_since IS NULL
         ORDER BY size_bytes DESC, id",
    )
    .bind(kind)
    .bind(item_id)
    .fetch_all(db)
    .await?;
    let selected = file_for(db, Some(user_id), kind, item_id).await?;
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(i, (id, path, size_bytes, duration_ms, probe))| {
            let info = probe.and_then(|json| media_info::parse(&json).ok());
            describe(
                id,
                &path,
                size_bytes,
                duration_ms,
                info.as_ref(),
                i == 0,
                selected == Some(id),
            )
        })
        .collect())
}

/// Plays `media_file_id` for this user from now on. It must be a present file of the
/// item. Picking the default forgets the choice, so a larger file added later wins.
pub async fn choose(
    db: &Db,
    user_id: i64,
    kind: &str,
    item_id: i64,
    media_file_id: i64,
) -> ApiResult<()> {
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM media_files WHERE id = ? AND item_kind = ? AND item_id = ?
           AND missing_since IS NULL)",
    )
    .bind(media_file_id)
    .bind(kind)
    .bind(item_id)
    .fetch_one(db)
    .await?;
    if !belongs {
        return Err(ApiError::bad_request(
            "unknown_version",
            "this file is not a version of this item",
        ));
    }
    if file_for(db, None, kind, item_id).await? == Some(media_file_id) {
        sqlx::query(
            "DELETE FROM version_choices WHERE user_id = ? AND item_kind = ? AND item_id = ?",
        )
        .bind(user_id)
        .bind(kind)
        .bind(item_id)
        .execute(db)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO version_choices (user_id, item_kind, item_id, media_file_id, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (user_id, item_kind, item_id) DO UPDATE SET
               media_file_id = excluded.media_file_id, updated_at = excluded.updated_at",
        )
        .bind(user_id)
        .bind(kind)
        .bind(item_id)
        .bind(media_file_id)
        .bind(now())
        .execute(db)
        .await?;
    }
    Ok(())
}

fn describe(
    id: i64,
    path: &str,
    size_bytes: i64,
    duration_ms: Option<i64>,
    info: Option<&MediaInfo>,
    default: bool,
    selected: bool,
) -> Version {
    let file_name = path.rsplit('/').next().unwrap_or(path).to_owned();
    let video = info.and_then(|i| i.video.as_ref());
    let mut audio_languages: Vec<String> = Vec::new();
    for track in info.map(|i| i.audio.as_slice()).unwrap_or_default() {
        if let Some(language) = track.language.as_deref().and_then(normalize_language)
            && !audio_languages.contains(&language)
        {
            audio_languages.push(language);
        }
    }
    let label = match video {
        Some(v) => {
            let mut parts = Vec::new();
            if let Some(resolution) = resolution_label(v.width, v.height) {
                parts.push(resolution);
            }
            if v.hdr {
                parts.push("HDR".to_owned());
            }
            parts.push(codec_name(&v.codec));
            parts.join(" · ")
        }
        None => file_name.clone(),
    };
    Version {
        id,
        label,
        file_name,
        size_bytes,
        duration_ms: duration_ms.or_else(|| info.and_then(|i| i.duration_ms)),
        width: video.and_then(|v| v.width),
        height: video.and_then(|v| v.height),
        video_codec: video.map(|v| v.codec.clone()),
        hdr: video.is_some_and(|v| v.hdr),
        audio_languages,
        default,
        selected,
    }
}

/// `4K`, `1080p`… from the width first: a scope movie in 1080p is 1920 × 800. Below
/// 720p, the height (`576p`).
pub fn resolution_label(width: Option<i64>, height: Option<i64>) -> Option<String> {
    let (w, h) = (width.unwrap_or(0), height.unwrap_or(0));
    Some(
        if w >= 3200 || h >= 2000 {
            "4K"
        } else if w >= 2400 || h >= 1400 {
            "1440p"
        } else if w >= 1700 || h >= 1000 {
            "1080p"
        } else if w >= 1200 || h >= 700 {
            "720p"
        } else if h > 0 {
            return Some(format!("{h}p"));
        } else if w > 0 {
            "SD"
        } else {
            return None;
        }
        .to_owned(),
    )
}

fn codec_name(codec: &str) -> String {
    match codec {
        "hevc" => "HEVC".into(),
        "h264" => "H.264".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        "mpeg4" => "MPEG-4".into(),
        "mpeg2video" => "MPEG-2".into(),
        other => other.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::media_info::{AudioTrack, TrackKind, VideoStream};

    #[test]
    fn resolution_from_width_first() {
        assert_eq!(
            resolution_label(Some(3840), Some(1600)).as_deref(),
            Some("4K")
        );
        assert_eq!(
            resolution_label(Some(1920), Some(800)).as_deref(),
            Some("1080p")
        );
        assert_eq!(
            resolution_label(Some(1280), Some(536)).as_deref(),
            Some("720p")
        );
        assert_eq!(
            resolution_label(Some(720), Some(576)).as_deref(),
            Some("576p")
        );
        assert_eq!(resolution_label(None, None), None);
    }

    fn audio(language: &str) -> AudioTrack {
        AudioTrack {
            index: 1,
            codec: "aac".into(),
            language: Some(language.into()),
            title: None,
            channels: Some(2),
            default: false,
            kind: TrackKind::Normal,
        }
    }

    #[test]
    fn label_and_languages() {
        let info = MediaInfo {
            duration_ms: Some(1000),
            container: None,
            size_bytes: None,
            bitrate: None,
            video: Some(VideoStream {
                index: 0,
                codec: "hevc".into(),
                profile: None,
                width: Some(3840),
                height: Some(2160),
                bit_depth: Some(10),
                hdr: true,
            }),
            audio: vec![audio("fre"), audio("eng"), audio("fr")],
            subtitles: vec![],
        };
        let v = describe(
            7,
            "Movies/Heat (1995)/Heat 4K.mkv",
            10,
            None,
            Some(&info),
            true,
            false,
        );
        assert_eq!(v.label, "4K · HDR · HEVC");
        assert_eq!(v.file_name, "Heat 4K.mkv");
        assert_eq!(v.audio_languages, ["fre", "eng"]);
        assert_eq!(v.duration_ms, Some(1000));
        let unknown = describe(
            8,
            "Movies/Heat (1995)/Heat.mkv",
            10,
            None,
            None,
            false,
            true,
        );
        assert_eq!(unknown.label, "Heat.mkv");
    }
}
