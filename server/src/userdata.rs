//! Per-user playback state: progress, played flags, "next up".

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    db::{Db, now},
    error::{ApiError, ApiResult},
};

/// Below this, a stopped item isn't worth resuming (trailer, accidental click).
pub const MIN_RESUME_MS: i64 = 60_000;
/// Watched past this share of its duration, an item counts as played.
pub const PLAYED_RATIO: f64 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlayableKind {
    Movie,
    Episode,
}

impl PlayableKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Episode => "episode",
        }
    }
}

/// What a progress report means for the stored state. Pure, for testing.
#[derive(Debug, PartialEq, Eq)]
pub enum ProgressOutcome {
    /// Store this resume position.
    Resume(i64),
    /// Too early to resume later: nothing to resume.
    NotStarted,
    /// Watched to the end (or close enough).
    Played,
}

pub fn progress_outcome(position_ms: i64, duration_ms: Option<i64>) -> ProgressOutcome {
    if let Some(duration) = duration_ms.filter(|d| *d > 0)
        && position_ms as f64 >= duration as f64 * PLAYED_RATIO
    {
        return ProgressOutcome::Played;
    }
    if position_ms < MIN_RESUME_MS {
        ProgressOutcome::NotStarted
    } else {
        ProgressOutcome::Resume(position_ms)
    }
}

async fn duration_of(db: &Db, kind: PlayableKind, id: i64) -> ApiResult<Option<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT duration_ms FROM media_files WHERE item_kind = ? AND item_id = ?
         AND missing_since IS NULL ORDER BY size_bytes DESC LIMIT 1",
    )
    .bind(kind.as_str())
    .bind(id)
    .fetch_optional(db)
    .await?
    .flatten())
}

async fn ensure_exists(db: &Db, kind: PlayableKind, id: i64) -> ApiResult<()> {
    let exists: bool = match kind {
        PlayableKind::Movie => sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM movies WHERE id = ? AND missing_since IS NULL)",
        ),
        PlayableKind::Episode => sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM episodes WHERE id = ? AND missing_since IS NULL)",
        ),
    }
    .bind(id)
    .fetch_one(db)
    .await?;
    if exists {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

/// Records a playback position. Returns whether the item is now played.
pub async fn record_progress(
    db: &Db,
    user_id: i64,
    kind: PlayableKind,
    id: i64,
    position_ms: i64,
) -> ApiResult<bool> {
    ensure_exists(db, kind, id).await?;
    let duration = duration_of(db, kind, id).await?;
    let now = now();
    match progress_outcome(position_ms.max(0), duration) {
        ProgressOutcome::Played => {
            mark(db, user_id, kind.as_str(), id, true, now).await?;
            Ok(true)
        }
        outcome => {
            let position = match outcome {
                ProgressOutcome::Resume(p) => p,
                _ => 0,
            };
            // Rewatching a played item keeps it played: only the position moves.
            sqlx::query(
                "INSERT INTO user_item_data (user_id, item_kind, item_id, position_ms, last_played_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT (user_id, item_kind, item_id) DO UPDATE SET
                   position_ms = excluded.position_ms,
                   last_played_at = excluded.last_played_at,
                   updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(kind.as_str())
            .bind(id)
            .bind(position)
            .bind(now)
            .execute(db)
            .await?;
            Ok(false)
        }
    }
}

async fn mark(db: &Db, user_id: i64, kind: &str, id: i64, played: bool, now: i64) -> ApiResult<()> {
    if played {
        sqlx::query(
            "INSERT INTO user_item_data
               (user_id, item_kind, item_id, position_ms, played, play_count, last_played_at, updated_at)
             VALUES (?1, ?2, ?3, 0, 1, 1, ?4, ?4)
             ON CONFLICT (user_id, item_kind, item_id) DO UPDATE SET
               play_count = user_item_data.play_count + (CASE WHEN user_item_data.played = 0
                   OR user_item_data.position_ms > 0 THEN 1 ELSE 0 END),
               position_ms = 0, played = 1,
               last_played_at = excluded.last_played_at, updated_at = excluded.updated_at",
        )
    } else {
        sqlx::query(
            "INSERT INTO user_item_data (user_id, item_kind, item_id, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (user_id, item_kind, item_id) DO UPDATE SET
               position_ms = 0, played = 0, updated_at = excluded.updated_at",
        )
    }
    .bind(user_id)
    .bind(kind)
    .bind(id)
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkableKind {
    Movie,
    Episode,
    Season,
    Series,
}

/// Marks an item (or every episode of a season/series) played or unplayed.
pub async fn set_played(
    db: &Db,
    user_id: i64,
    kind: MarkableKind,
    id: i64,
    played: bool,
) -> ApiResult<()> {
    let now = now();
    match kind {
        MarkableKind::Movie => {
            ensure_exists(db, PlayableKind::Movie, id).await?;
            mark(db, user_id, "movie", id, played, now).await
        }
        MarkableKind::Episode => {
            ensure_exists(db, PlayableKind::Episode, id).await?;
            mark(db, user_id, "episode", id, played, now).await
        }
        MarkableKind::Season | MarkableKind::Series => {
            let episodes: Vec<i64> = match kind {
                MarkableKind::Season => sqlx::query_scalar(
                    "SELECT id FROM episodes WHERE season_id = ? AND missing_since IS NULL",
                ),
                _ => sqlx::query_scalar(
                    "SELECT id FROM episodes WHERE series_id = ? AND missing_since IS NULL",
                ),
            }
            .bind(id)
            .fetch_all(db)
            .await?;
            if episodes.is_empty() {
                return Err(ApiError::NotFound);
            }
            for episode in episodes {
                mark(db, user_id, "episode", episode, played, now).await?;
            }
            Ok(())
        }
    }
}

/// Previous and next episode of a series. Specials (season 0) come after the
/// regular seasons, so they never interrupt the main story.
pub async fn neighbours(
    db: &Db,
    series_id: i64,
    season: i64,
    episode: i64,
) -> ApiResult<(Option<i64>, Option<i64>)> {
    let key = (i64::from(season == 0), season, episode);
    let previous = sqlx::query_scalar(
        "SELECT id FROM episodes WHERE series_id = ?1 AND missing_since IS NULL
           AND (season_number = 0, season_number, episode_number) < (?2, ?3, ?4)
         ORDER BY season_number = 0 DESC, season_number DESC, episode_number DESC LIMIT 1",
    )
    .bind(series_id)
    .bind(key.0)
    .bind(key.1)
    .bind(key.2)
    .fetch_optional(db)
    .await?;
    let next = sqlx::query_scalar(
        "SELECT id FROM episodes WHERE series_id = ?1 AND missing_since IS NULL
           AND (season_number = 0, season_number, episode_number) > (?2, ?3, ?4)
         ORDER BY season_number = 0, season_number, episode_number LIMIT 1",
    )
    .bind(series_id)
    .bind(key.0)
    .bind(key.1)
    .bind(key.2)
    .fetch_optional(db)
    .await?;
    Ok((previous, next))
}

#[derive(sqlx::FromRow)]
struct LastWatched {
    id: i64,
    season_number: i64,
    episode_number: i64,
    played: bool,
    position_ms: i64,
}

async fn last_watched(db: &Db, user_id: i64, series_id: i64) -> ApiResult<Option<LastWatched>> {
    Ok(sqlx::query_as(
        "SELECT e.id, e.season_number, e.episode_number, d.played, d.position_ms
         FROM user_item_data d JOIN episodes e ON d.item_kind = 'episode' AND e.id = d.item_id
         WHERE d.user_id = ? AND e.series_id = ? AND e.missing_since IS NULL
           AND d.last_played_at IS NOT NULL
         ORDER BY d.last_played_at DESC, e.season_number DESC, e.episode_number DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(series_id)
    .fetch_optional(db)
    .await?)
}

/// First unwatched episode after (season, episode), in viewing order.
async fn next_unplayed_after(
    db: &Db,
    user_id: i64,
    series_id: i64,
    after: Option<(i64, i64)>,
) -> ApiResult<Option<i64>> {
    let (s0, season, episode) = match after {
        Some((s, e)) => (i64::from(s == 0), s, e),
        None => (-1, -1, -1),
    };
    Ok(sqlx::query_scalar(
        "SELECT e.id FROM episodes e WHERE e.series_id = ?1 AND e.missing_since IS NULL
           AND (e.season_number = 0, e.season_number, e.episode_number) > (?2, ?3, ?4)
           AND NOT EXISTS (SELECT 1 FROM user_item_data d WHERE d.user_id = ?5
               AND d.item_kind = 'episode' AND d.item_id = e.id AND d.played = 1)
         ORDER BY e.season_number = 0, e.season_number, e.episode_number LIMIT 1",
    )
    .bind(series_id)
    .bind(s0)
    .bind(season)
    .bind(episode)
    .bind(user_id)
    .fetch_optional(db)
    .await?)
}

/// The episode a series' play button starts: the one in progress, else the next
/// unwatched one after the last watched, else the first unwatched.
pub async fn series_up_next(db: &Db, user_id: i64, series_id: i64) -> ApiResult<Option<i64>> {
    match last_watched(db, user_id, series_id).await? {
        // In progress, or started then abandoned early: that's still the one to watch.
        Some(last) if !last.played => Ok(Some(last.id)),
        Some(last) => {
            let after = (last.season_number, last.episode_number);
            match next_unplayed_after(db, user_id, series_id, Some(after)).await? {
                Some(id) => Ok(Some(id)),
                None => next_unplayed_after(db, user_id, series_id, None).await,
            }
        }
        None => next_unplayed_after(db, user_id, series_id, None).await,
    }
}

/// A row of "Continue watching": (kind, id), most recent first, one episode per series.
pub async fn continue_watching(
    db: &Db,
    user_id: i64,
    limit: usize,
) -> ApiResult<Vec<(PlayableKind, i64)>> {
    let rows: Vec<(String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT d.item_kind, d.item_id, e.series_id
         FROM user_item_data d
         LEFT JOIN movies m ON d.item_kind = 'movie' AND m.id = d.item_id
         LEFT JOIN episodes e ON d.item_kind = 'episode' AND e.id = d.item_id
         WHERE d.user_id = ? AND d.position_ms > 0
           AND COALESCE(m.missing_since, e.missing_since) IS NULL
           AND (m.id IS NOT NULL OR e.id IS NOT NULL)
         ORDER BY d.last_played_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    let mut seen_series = std::collections::HashSet::new();
    Ok(rows
        .into_iter()
        .filter(|(_, _, series)| series.is_none_or(|s| seen_series.insert(s)))
        .map(|(kind, id, _)| {
            let kind = if kind == "movie" {
                PlayableKind::Movie
            } else {
                PlayableKind::Episode
            };
            (kind, id)
        })
        .take(limit)
        .collect())
}

/// "Next up": for each series recently watched (most recent first) whose last
/// watched episode is finished, the next unwatched episode.
pub async fn next_up(db: &Db, user_id: i64, limit: usize) -> ApiResult<Vec<i64>> {
    let series: Vec<i64> = sqlx::query_scalar(
        "SELECT e.series_id FROM user_item_data d
         JOIN episodes e ON d.item_kind = 'episode' AND e.id = d.item_id
         WHERE d.user_id = ? AND d.last_played_at IS NOT NULL
         GROUP BY e.series_id ORDER BY MAX(d.last_played_at) DESC LIMIT 50",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    let mut episodes = Vec::new();
    for series_id in series {
        let Some(last) = last_watched(db, user_id, series_id).await? else {
            continue;
        };
        let next = if !last.played {
            if last.position_ms > 0 {
                continue; // in progress: already in "Continue watching"
            }
            // Opened then stopped early: still the next one to watch.
            Some(last.id)
        } else {
            let after = (last.season_number, last.episode_number);
            next_unplayed_after(db, user_id, series_id, Some(after)).await?
        };
        if let Some(id) = next {
            episodes.push(id);
            if episodes.len() >= limit {
                break;
            }
        }
    }
    Ok(episodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_outcomes() {
        let hour = Some(3_600_000);
        assert_eq!(progress_outcome(10_000, hour), ProgressOutcome::NotStarted);
        assert_eq!(
            progress_outcome(600_000, hour),
            ProgressOutcome::Resume(600_000)
        );
        assert_eq!(
            progress_outcome(3_240_000, hour),
            ProgressOutcome::Played,
            "90 %"
        );
        assert_eq!(
            progress_outcome(3_239_999, hour),
            ProgressOutcome::Resume(3_239_999)
        );
        assert_eq!(
            progress_outcome(600_000, None),
            ProgressOutcome::Resume(600_000),
            "unknown duration"
        );
    }
}

/// One progress record made offline, uploaded when the device is back online.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct OfflineProgress {
    pub kind: PlayableKind,
    pub id: i64,
    pub position_ms: i64,
    pub played: bool,
    /// When it happened on the device, Unix seconds.
    pub at: i64,
}

/// Applies offline records. The most recent watch wins: a record older than what the
/// server already has (watched elsewhere since) is ignored. Returns how many applied.
pub async fn apply_offline(db: &Db, user_id: i64, records: &[OfflineProgress]) -> ApiResult<usize> {
    let now = now();
    let mut applied = 0;
    let mut sorted: Vec<&OfflineProgress> = records.iter().collect();
    sorted.sort_by_key(|r| r.at);
    for r in sorted {
        // A clock in the future must not lock the item forever.
        let at = r.at.min(now);
        let exists: bool = match r.kind {
            PlayableKind::Movie => {
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM movies WHERE id = ?)")
            }
            PlayableKind::Episode => {
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM episodes WHERE id = ?)")
            }
        }
        .bind(r.id)
        .fetch_one(db)
        .await?;
        if !exists {
            continue;
        }
        let current: Option<Option<i64>> = sqlx::query_scalar(
            "SELECT last_played_at FROM user_item_data WHERE user_id = ? AND item_kind = ? AND item_id = ?",
        )
        .bind(user_id)
        .bind(r.kind.as_str())
        .bind(r.id)
        .fetch_optional(db)
        .await?;
        if current.flatten().is_some_and(|server_at| server_at > at) {
            continue;
        }
        let duration = duration_of(db, r.kind, r.id).await?;
        let played =
            r.played || progress_outcome(r.position_ms, duration) == ProgressOutcome::Played;
        let position = match progress_outcome(r.position_ms.max(0), duration) {
            ProgressOutcome::Resume(p) if !played => p,
            _ => 0,
        };
        sqlx::query(
            "INSERT INTO user_item_data
               (user_id, item_kind, item_id, position_ms, played, play_count, last_played_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7)
             ON CONFLICT (user_id, item_kind, item_id) DO UPDATE SET
               play_count = user_item_data.play_count + (CASE WHEN ?5 = 1 AND user_item_data.played = 0 THEN 1 ELSE 0 END),
               played = CASE WHEN ?5 = 1 THEN 1 ELSE user_item_data.played END,
               position_ms = excluded.position_ms,
               last_played_at = excluded.last_played_at,
               updated_at = excluded.updated_at",
        )
        .bind(user_id)
        .bind(r.kind.as_str())
        .bind(r.id)
        .bind(position)
        .bind(played)
        .bind(at)
        .bind(now)
        .execute(db)
        .await?;
        applied += 1;
    }
    Ok(applied)
}
