//! Fetches TMDB metadata for library items, in every language used by an account.

pub mod tmdb;

use std::collections::HashMap;

use crate::db::{Db, now};
use tmdb::{TmdbClient, TmdbError, best_match};

/// An item without a TMDB match is searched again after this long.
const RETRY_UNMATCHED_SECS: i64 = 7 * 24 * 3600;
const FALLBACK_LANGUAGE: &str = "en-US";
/// A matched item is fetched again after this long, so its ratings stay current.
const STALE_AFTER_SECS: i64 = 30 * 24 * 3600;
/// Stale items refetched per library and per scan, to spread the load over scans.
const STALE_MOVIES_PER_SCAN: i64 = 40;
const STALE_SERIES_PER_SCAN: i64 = 10;
/// A stale item whose refresh failed is tried again after this long.
const STALE_RETRY_SECS: i64 = 24 * 3600;

/// Languages to fetch: the server default first, then each user's language.
pub async fn languages(db: &Db, default_language: &str) -> anyhow::Result<Vec<String>> {
    let mut languages = vec![default_language.to_owned()];
    let users: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT language FROM users ORDER BY language")
            .fetch_all(db)
            .await?;
    for language in users {
        if !languages.contains(&language) {
            languages.push(language);
        }
    }
    Ok(languages)
}

async fn upsert_translation(
    db: &Db,
    kind: &str,
    id: i64,
    language: &str,
    title: Option<&str>,
    overview: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO translations (item_kind, item_id, language, title, overview)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (item_kind, item_id, language)
         DO UPDATE SET title = excluded.title, overview = excluded.overview",
    )
    .bind(kind)
    .bind(id)
    .bind(language)
    .bind(title.filter(|t| !t.is_empty()))
    .bind(overview.filter(|o| !o.is_empty()))
    .execute(db)
    .await?;
    Ok(())
}

/// Stores (or clears) an item's TMDB rating. Every vote count is kept: the API
/// decides what is shown.
async fn set_tmdb_rating(
    db: &Db,
    kind: &str,
    id: i64,
    average: Option<f64>,
    votes: Option<i64>,
) -> anyhow::Result<()> {
    match (average, votes) {
        (Some(value), Some(votes)) => {
            sqlx::query(
                "INSERT INTO ratings (item_kind, item_id, source, value, scale, votes, updated_at)
                 VALUES (?, ?, 'tmdb', ?, 10, ?, ?)
                 ON CONFLICT (item_kind, item_id, source)
                 DO UPDATE SET value = excluded.value, scale = excluded.scale,
                   votes = excluded.votes, updated_at = excluded.updated_at",
            )
            .bind(kind)
            .bind(id)
            .bind(value)
            .bind(votes)
            .bind(now())
            .execute(db)
            .await?;
        }
        _ => {
            sqlx::query(
                "DELETE FROM ratings WHERE item_kind = ? AND item_id = ? AND source = 'tmdb'",
            )
            .bind(kind)
            .bind(id)
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

fn non_empty(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|t| !t.trim().is_empty())
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    parsed_title: String,
    parsed_year: Option<i64>,
    tmdb_id: Option<i64>,
}

/// Refreshes one movie. Returns false if no TMDB match was found.
pub async fn refresh_movie(
    db: &Db,
    tmdb: &TmdbClient,
    movie_id: i64,
    languages: &[String],
) -> anyhow::Result<bool> {
    let row: ItemRow =
        sqlx::query_as("SELECT parsed_title, parsed_year, tmdb_id FROM movies WHERE id = ?")
            .bind(movie_id)
            .fetch_one(db)
            .await?;
    let primary = &languages[0];

    let tmdb_id = match row.tmdb_id {
        Some(id) => id,
        None => {
            let mut results = tmdb
                .search_movie(&row.parsed_title, row.parsed_year, primary)
                .await?;
            if results.is_empty() && row.parsed_year.is_some() {
                results = tmdb.search_movie(&row.parsed_title, None, primary).await?;
            }
            match best_match(&results, row.parsed_year) {
                Some(found) => found.id,
                None => {
                    sqlx::query("UPDATE movies SET metadata_updated_at = ? WHERE id = ?")
                        .bind(now())
                        .bind(movie_id)
                        .execute(db)
                        .await?;
                    return Ok(false);
                }
            }
        }
    };

    let mut details = Vec::with_capacity(languages.len());
    for language in languages {
        details.push((language, tmdb.movie(tmdb_id, language).await?));
    }
    let mut fallback_overview = None;
    if details
        .iter()
        .any(|(l, d)| *l != FALLBACK_LANGUAGE && non_empty(&d.overview).is_none())
    {
        fallback_overview = tmdb.movie(tmdb_id, FALLBACK_LANGUAGE).await?.overview;
    }

    let main = &details[0].1;
    sqlx::query(
        "UPDATE movies SET tmdb_id = ?, release_date = ?, runtime_minutes = ?, poster_path = ?,
         backdrop_path = ?, metadata_updated_at = ? WHERE id = ?",
    )
    .bind(tmdb_id)
    .bind(&main.release_date)
    .bind(main.runtime)
    .bind(&main.poster_path)
    .bind(&main.backdrop_path)
    .bind(now())
    .bind(movie_id)
    .execute(db)
    .await?;

    set_tmdb_rating(db, "movie", movie_id, main.vote_average, main.vote_count).await?;

    for (language, d) in &details {
        let overview = non_empty(&d.overview).or(non_empty(&fallback_overview));
        upsert_translation(db, "movie", movie_id, language, Some(&d.title), overview).await?;
    }
    Ok(true)
}

/// Refreshes a series, its local seasons and episodes. Returns false if no TMDB match.
pub async fn refresh_series(
    db: &Db,
    tmdb: &TmdbClient,
    series_id: i64,
    languages: &[String],
) -> anyhow::Result<bool> {
    let row: ItemRow =
        sqlx::query_as("SELECT parsed_title, parsed_year, tmdb_id FROM series WHERE id = ?")
            .bind(series_id)
            .fetch_one(db)
            .await?;
    let primary = &languages[0];

    let tmdb_id = match row.tmdb_id {
        Some(id) => id,
        None => {
            let mut results = tmdb
                .search_tv(&row.parsed_title, row.parsed_year, primary)
                .await?;
            if results.is_empty() && row.parsed_year.is_some() {
                results = tmdb.search_tv(&row.parsed_title, None, primary).await?;
            }
            match best_match(&results, row.parsed_year) {
                Some(found) => found.id,
                None => {
                    sqlx::query("UPDATE series SET metadata_updated_at = ? WHERE id = ?")
                        .bind(now())
                        .bind(series_id)
                        .execute(db)
                        .await?;
                    return Ok(false);
                }
            }
        }
    };

    // Series level.
    let mut shows = Vec::with_capacity(languages.len());
    for language in languages {
        shows.push((language, tmdb.tv(tmdb_id, language).await?));
    }
    let needs_fallback = |overview: &Option<String>, l: &str| {
        l != FALLBACK_LANGUAGE && non_empty(overview).is_none()
    };
    let fallback_show = if shows.iter().any(|(l, s)| needs_fallback(&s.overview, l)) {
        Some(tmdb.tv(tmdb_id, FALLBACK_LANGUAGE).await?)
    } else {
        None
    };
    let main = &shows[0].1;
    sqlx::query(
        "UPDATE series SET tmdb_id = ?, first_air_date = ?, poster_path = ?, backdrop_path = ?,
         metadata_updated_at = ? WHERE id = ?",
    )
    .bind(tmdb_id)
    .bind(&main.first_air_date)
    .bind(&main.poster_path)
    .bind(&main.backdrop_path)
    .bind(now())
    .bind(series_id)
    .execute(db)
    .await?;
    set_tmdb_rating(db, "series", series_id, main.vote_average, main.vote_count).await?;
    for (language, show) in &shows {
        let overview = non_empty(&show.overview)
            .or_else(|| fallback_show.as_ref().and_then(|f| non_empty(&f.overview)));
        upsert_translation(
            db,
            "series",
            series_id,
            language,
            Some(&show.name),
            overview,
        )
        .await?;
    }

    // Seasons and episodes that exist locally.
    let seasons: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, season_number FROM seasons WHERE series_id = ?")
            .bind(series_id)
            .fetch_all(db)
            .await?;
    for (season_id, number) in seasons {
        let mut per_language = Vec::with_capacity(languages.len());
        for language in languages {
            match tmdb.season(tmdb_id, number, language).await {
                Ok(season) => per_language.push((language, season)),
                // Seasons unknown to TMDB (fan-made specials…) are not an error.
                Err(TmdbError::NotFound) => break,
                Err(err) => return Err(err.into()),
            }
        }
        let Some((_, main_season)) = per_language.first() else {
            continue;
        };
        let fallback_season = if per_language.iter().any(|(l, s)| {
            needs_fallback(&s.overview, l)
                || s.episodes.iter().any(|e| needs_fallback(&e.overview, l))
        }) {
            tmdb.season(tmdb_id, number, FALLBACK_LANGUAGE).await.ok()
        } else {
            None
        };
        let fallback_episodes: HashMap<i64, &tmdb::EpisodeDetails> = fallback_season
            .iter()
            .flat_map(|s| s.episodes.iter().map(|e| (e.episode_number, e)))
            .collect();

        sqlx::query("UPDATE seasons SET poster_path = ? WHERE id = ?")
            .bind(&main_season.poster_path)
            .bind(season_id)
            .execute(db)
            .await?;
        set_tmdb_rating(
            db,
            "season",
            season_id,
            main_season.vote_average,
            Some(main_season.votes()),
        )
        .await?;

        let local: Vec<(i64, i64)> =
            sqlx::query_as("SELECT id, episode_number FROM episodes WHERE season_id = ?")
                .bind(season_id)
                .fetch_all(db)
                .await?;
        let main_episodes: HashMap<i64, &tmdb::EpisodeDetails> = main_season
            .episodes
            .iter()
            .map(|e| (e.episode_number, e))
            .collect();
        for (episode_id, episode_number) in &local {
            if let Some(e) = main_episodes.get(episode_number) {
                sqlx::query(
                    "UPDATE episodes SET air_date = ?, runtime_minutes = ?, still_path = ? WHERE id = ?",
                )
                .bind(&e.air_date)
                .bind(e.runtime)
                .bind(&e.still_path)
                .bind(episode_id)
                .execute(db)
                .await?;
                set_tmdb_rating(db, "episode", *episode_id, e.vote_average, e.vote_count).await?;
            }
        }

        for (language, season) in &per_language {
            let overview = non_empty(&season.overview).or_else(|| {
                fallback_season
                    .as_ref()
                    .and_then(|f| non_empty(&f.overview))
            });
            upsert_translation(
                db,
                "season",
                season_id,
                language,
                season.name.as_deref(),
                overview,
            )
            .await?;
            let by_number: HashMap<i64, &tmdb::EpisodeDetails> = season
                .episodes
                .iter()
                .map(|e| (e.episode_number, e))
                .collect();
            for (episode_id, episode_number) in &local {
                let Some(e) = by_number.get(episode_number) else {
                    continue;
                };
                let overview = non_empty(&e.overview).or_else(|| {
                    fallback_episodes
                        .get(episode_number)
                        .and_then(|f| non_empty(&f.overview))
                });
                upsert_translation(
                    db,
                    "episode",
                    *episode_id,
                    language,
                    e.name.as_deref(),
                    overview,
                )
                .await?;
            }
        }
    }
    Ok(true)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RefreshCounts {
    pub refreshed: usize,
    pub errors: usize,
}

/// Refreshes every item of a library that has no metadata yet, lacks a language,
/// or (for series) has episodes without metadata; then a few matched items whose
/// metadata is older than [`STALE_AFTER_SECS`], oldest first.
pub async fn refresh_library(
    db: &Db,
    tmdb: &TmdbClient,
    library_id: i64,
    languages: &[String],
) -> anyhow::Result<RefreshCounts> {
    let langs_json = serde_json::to_string(languages)?;
    let retry_before = now() - RETRY_UNMATCHED_SECS;
    let mut counts = RefreshCounts::default();

    let movies: Vec<i64> = sqlx::query_scalar(
        "SELECT m.id FROM movies m
         WHERE m.library_id = ? AND m.missing_since IS NULL AND (
           m.metadata_updated_at IS NULL
           OR (m.tmdb_id IS NULL AND m.metadata_updated_at < ?)
           OR (m.tmdb_id IS NOT NULL AND (
                SELECT COUNT(*) FROM translations t
                WHERE t.item_kind = 'movie' AND t.item_id = m.id
                  AND t.language IN (SELECT value FROM json_each(?))) < json_array_length(?)))",
    )
    .bind(library_id)
    .bind(retry_before)
    .bind(&langs_json)
    .bind(&langs_json)
    .fetch_all(db)
    .await?;
    for id in movies {
        match refresh_movie(db, tmdb, id, languages).await {
            Ok(_) => counts.refreshed += 1,
            Err(err) => {
                counts.errors += 1;
                tracing::warn!(movie_id = id, error = %err, "metadata refresh failed");
                if is_fatal(&err) {
                    return Err(err);
                }
            }
        }
    }

    let series: Vec<i64> = sqlx::query_scalar(
        "SELECT s.id FROM series s
         WHERE s.library_id = ? AND s.missing_since IS NULL AND (
           s.metadata_updated_at IS NULL
           OR (s.tmdb_id IS NULL AND s.metadata_updated_at < ?)
           OR (s.tmdb_id IS NOT NULL AND (
                (SELECT COUNT(*) FROM translations t
                 WHERE t.item_kind = 'series' AND t.item_id = s.id
                   AND t.language IN (SELECT value FROM json_each(?))) < json_array_length(?)
                OR EXISTS (
                  SELECT 1 FROM episodes e WHERE e.series_id = s.id AND e.missing_since IS NULL
                    AND NOT EXISTS (SELECT 1 FROM translations t
                      WHERE t.item_kind = 'episode' AND t.item_id = e.id AND t.language = ?)
                    -- >= : an episode added in the same second as the last refresh still
                    -- counts. One unknown to TMDB is retried at most once more.
                    AND e.added_at >= COALESCE(s.metadata_updated_at, 0)))))",
    )
    .bind(library_id)
    .bind(retry_before)
    .bind(&langs_json)
    .bind(&langs_json)
    .bind(&languages[0])
    .fetch_all(db)
    .await?;
    for id in series {
        match refresh_series(db, tmdb, id, languages).await {
            Ok(_) => counts.refreshed += 1,
            Err(err) => {
                counts.errors += 1;
                tracing::warn!(series_id = id, error = %err, "metadata refresh failed");
                if is_fatal(&err) {
                    return Err(err);
                }
            }
        }
    }
    refresh_stale(db, tmdb, library_id, languages, &mut counts).await?;
    Ok(counts)
}

#[derive(Clone, Copy)]
enum ItemKind {
    Movie,
    Series,
}

async fn refresh_stale(
    db: &Db,
    tmdb: &TmdbClient,
    library_id: i64,
    languages: &[String],
    counts: &mut RefreshCounts,
) -> anyhow::Result<()> {
    let stale_before = now() - STALE_AFTER_SECS;
    let movies: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM movies
         WHERE library_id = ? AND missing_since IS NULL AND tmdb_id IS NOT NULL
           AND metadata_updated_at < ?
         ORDER BY metadata_updated_at, id LIMIT ?",
    )
    .bind(library_id)
    .bind(stale_before)
    .bind(STALE_MOVIES_PER_SCAN)
    .fetch_all(db)
    .await?;
    let series: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM series
         WHERE library_id = ? AND missing_since IS NULL AND tmdb_id IS NOT NULL
           AND metadata_updated_at < ?
         ORDER BY metadata_updated_at, id LIMIT ?",
    )
    .bind(library_id)
    .bind(stale_before)
    .bind(STALE_SERIES_PER_SCAN)
    .fetch_all(db)
    .await?;
    let items = movies
        .into_iter()
        .map(|id| (ItemKind::Movie, id))
        .chain(series.into_iter().map(|id| (ItemKind::Series, id)));
    for (kind, id) in items {
        let result = match kind {
            ItemKind::Movie => refresh_movie(db, tmdb, id, languages).await,
            ItemKind::Series => refresh_series(db, tmdb, id, languages).await,
        };
        match result {
            Ok(_) => counts.refreshed += 1,
            Err(err) => {
                counts.errors += 1;
                tracing::warn!(id, error = %err, "stale metadata refresh failed");
                if is_fatal(&err) {
                    return Err(err);
                }
                // Keep the old metadata, and let other stale items go first.
                let retry_at = now() - STALE_AFTER_SECS + STALE_RETRY_SECS;
                let sql = match kind {
                    ItemKind::Movie => "UPDATE movies SET metadata_updated_at = ? WHERE id = ?",
                    ItemKind::Series => "UPDATE series SET metadata_updated_at = ? WHERE id = ?",
                };
                sqlx::query(sql).bind(retry_at).bind(id).execute(db).await?;
            }
        }
    }
    Ok(())
}

/// A bad token fails every request: stop instead of hammering TMDB.
fn is_fatal(err: &anyhow::Error) -> bool {
    matches!(
        err.downcast_ref::<TmdbError>(),
        Some(TmdbError::Unauthorized)
    )
}
