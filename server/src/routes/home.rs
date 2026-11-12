//! Home screen, search, progress reports and track preferences.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::library::{
    self, BrowseQuery, EpisodeSummary, MovieSummary, SeriesSummary, SortOrder, load_episode,
    load_series_header,
};
use crate::{
    AppState,
    auth::{CurrentDevice, User},
    error::{ApiError, ApiResult},
    playback::SubtitleMode,
    userdata::{self, MarkableKind, PlayableKind},
};

// ---- Progress ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct ProgressReport {
    pub kind: PlayableKind,
    pub id: i64,
    pub position_ms: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ProgressResult {
    /// True once watched past 90 %: the client can offer the next episode.
    pub played: bool,
}

/// Reports the playback position (every few seconds, on pause and on exit).
#[utoipa::path(post, path = "/api/progress", tag = "playback", security(("bearer" = [])),
    request_body = ProgressReport,
    responses((status = 200, body = ProgressResult), (status = 404, body = crate::error::ErrorBody)))]
pub async fn report_progress(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(report): Json<ProgressReport>,
) -> ApiResult<Json<ProgressResult>> {
    let played = userdata::record_progress(
        &state.db,
        current.user.id,
        report.kind,
        report.id,
        report.position_ms,
    )
    .await?;
    Ok(Json(ProgressResult { played }))
}

#[derive(Deserialize, ToSchema)]
pub struct SetPlayedRequest {
    pub played: bool,
}

/// Marks a movie, an episode, or every episode of a season or series as (un)watched.
#[utoipa::path(put, path = "/api/played/{kind}/{id}", tag = "playback", security(("bearer" = [])),
    params(("kind" = MarkableKind, Path), ("id" = i64, Path)), request_body = SetPlayedRequest,
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
pub async fn set_played(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path((kind, id)): Path<(MarkableKind, i64)>,
    Json(req): Json<SetPlayedRequest>,
) -> ApiResult<StatusCode> {
    userdata::set_played(&state.db, current.user.id, kind, id, req.played).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Home -------------------------------------------------------------------------------

/// A movie or an episode, ready to show as a card.
#[derive(Serialize, ToSchema)]
pub struct HomeItem {
    pub kind: PlayableKind,
    pub id: i64,
    /// Movie title, or series title for an episode.
    pub title: String,
    /// Episode title.
    pub subtitle: Option<String>,
    pub series_id: Option<i64>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub overview: Option<String>,
    /// Wide image: movie backdrop, episode still (or series backdrop).
    pub image_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub poster_path: Option<String>,
    pub position_ms: i64,
    pub duration_ms: Option<i64>,
}

/// A card for any playable item.
pub async fn item_card(
    state: &AppState,
    user: &User,
    kind: PlayableKind,
    id: i64,
) -> ApiResult<HomeItem> {
    match kind {
        PlayableKind::Movie => movie_item(state, user, id).await,
        PlayableKind::Episode => episode_item(state, user, id).await,
    }
}

async fn movie_item(state: &AppState, user: &User, id: i64) -> ApiResult<HomeItem> {
    let movie = library::load_movie_detail(state, user, id).await?;
    Ok(HomeItem {
        kind: PlayableKind::Movie,
        id,
        title: movie.title,
        subtitle: None,
        series_id: None,
        season_number: None,
        episode_number: None,
        overview: movie.overview,
        image_path: movie.backdrop_path.clone(),
        backdrop_path: movie.backdrop_path,
        poster_path: movie.poster_path,
        position_ms: movie.position_ms,
        duration_ms: movie.file.and_then(|f| f.media.and_then(|m| m.duration_ms)),
    })
}

pub async fn episode_item(state: &AppState, user: &User, id: i64) -> ApiResult<HomeItem> {
    let episode: EpisodeSummary = load_episode(state, user, id).await?;
    let header = load_series_header(state, user, episode.series_id).await?;
    Ok(HomeItem {
        kind: PlayableKind::Episode,
        id,
        title: header.title,
        subtitle: episode.title,
        series_id: Some(episode.series_id),
        season_number: Some(episode.season_number),
        episode_number: Some(episode.episode_number),
        overview: episode.overview,
        image_path: episode.still_path.or_else(|| header.backdrop_path.clone()),
        backdrop_path: header.backdrop_path,
        poster_path: header.poster_path,
        position_ms: episode.position_ms,
        duration_ms: episode.duration_ms,
    })
}

#[derive(Serialize, ToSchema)]
pub struct Home {
    /// Big banner at the top: what's in progress, else what's next, else the latest movie.
    pub hero: Option<HomeItem>,
    pub continue_watching: Vec<HomeItem>,
    pub next_up: Vec<HomeItem>,
    pub recent_movies: Vec<MovieSummary>,
    pub recent_series: Vec<SeriesSummary>,
    /// Best TMDB ratings with enough votes to mean something (D76).
    pub top_rated_movies: Vec<MovieSummary>,
}

/// Votes a movie needs to enter the top-rated rail: fewer lets obscure titles in (D76).
pub const TOP_RATED_MIN_VOTES: i64 = 50;

fn recent(sort: SortOrder) -> BrowseQuery {
    BrowseQuery {
        library_id: None,
        sort,
        order: None,
        limit: Some(20),
        offset: None,
    }
}

#[utoipa::path(get, path = "/api/home", tag = "library", security(("bearer" = [])),
    responses((status = 200, body = Home)))]
pub async fn home(State(state): State<AppState>, current: CurrentDevice) -> ApiResult<Json<Home>> {
    let user = &current.user;
    let mut continue_watching = Vec::new();
    for (kind, id) in userdata::continue_watching(&state.db, user.id, 20).await? {
        continue_watching.push(match kind {
            PlayableKind::Movie => movie_item(&state, user, id).await?,
            PlayableKind::Episode => episode_item(&state, user, id).await?,
        });
    }
    let mut next_up = Vec::new();
    for id in userdata::next_up(&state.db, user.id, 20).await? {
        next_up.push(episode_item(&state, user, id).await?);
    }
    let recent_movies = library::query_movies(&state, user, &recent(SortOrder::Added), None)
        .await?
        .items;
    let recent_series = library::query_series(&state, user, &recent(SortOrder::Added), None)
        .await?
        .items;
    let top_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT r.item_id FROM ratings r JOIN movies m ON m.id = r.item_id AND m.missing_since IS NULL
         WHERE r.item_kind = 'movie' AND r.source = 'tmdb' AND r.votes >= ?
         ORDER BY r.value / r.scale DESC, r.votes DESC LIMIT 20",
    )
    .bind(TOP_RATED_MIN_VOTES)
    .fetch_all(&state.db)
    .await?;
    let top_rated_movies = if top_ids.is_empty() {
        Vec::new()
    } else {
        library::query_movies(&state, user, &recent(SortOrder::Rating), Some(&top_ids))
            .await?
            .items
    };

    let hero = match continue_watching.first().or(next_up.first()) {
        Some(item) => Some(match item.kind {
            PlayableKind::Movie => movie_item(&state, user, item.id).await?,
            PlayableKind::Episode => episode_item(&state, user, item.id).await?,
        }),
        None => match recent_movies.iter().find(|m| m.backdrop_path.is_some()) {
            Some(movie) => Some(movie_item(&state, user, movie.id).await?),
            None => None,
        },
    };

    Ok(Json(Home {
        hero,
        continue_watching,
        next_up,
        recent_movies,
        recent_series,
        top_rated_movies,
    }))
}

// ---- Search -----------------------------------------------------------------------------

#[derive(Deserialize, IntoParams)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Serialize, ToSchema)]
pub struct SearchResults {
    pub movies: Vec<MovieSummary>,
    pub series: Vec<SeriesSummary>,
}

/// Finds movies and series whose title contains `q` (in any fetched language).
#[utoipa::path(get, path = "/api/search", tag = "library", security(("bearer" = [])),
    params(SearchQuery), responses((status = 200, body = SearchResults)))]
pub async fn search(
    State(state): State<AppState>,
    current: CurrentDevice,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<SearchResults>> {
    let needle = q.q.trim();
    if needle.chars().count() < 2 {
        return Ok(Json(SearchResults {
            movies: vec![],
            series: vec![],
        }));
    }
    let pattern = format!(
        "%{}%",
        needle
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let movie_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT m.id FROM movies m WHERE m.missing_since IS NULL AND (
           m.parsed_title LIKE ?1 ESCAPE '\\'
           OR EXISTS (SELECT 1 FROM translations t WHERE t.item_kind = 'movie' AND t.item_id = m.id
                      AND t.title LIKE ?1 ESCAPE '\\'))
         LIMIT 30",
    )
    .bind(&pattern)
    .fetch_all(&state.db)
    .await?;
    let series_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT s.id FROM series s WHERE s.missing_since IS NULL AND (
           s.parsed_title LIKE ?1 ESCAPE '\\'
           OR EXISTS (SELECT 1 FROM translations t WHERE t.item_kind = 'series' AND t.item_id = s.id
                      AND t.title LIKE ?1 ESCAPE '\\'))
         LIMIT 30",
    )
    .bind(&pattern)
    .fetch_all(&state.db)
    .await?;

    let all = BrowseQuery {
        library_id: None,
        sort: SortOrder::Title,
        order: None,
        limit: Some(60),
        offset: None,
    };
    let movies = library::query_movies(&state, &current.user, &all, Some(&movie_ids))
        .await?
        .items;
    let series = library::query_series(&state, &current.user, &all, Some(&series_ids))
        .await?
        .items;
    Ok(Json(SearchResults { movies, series }))
}

// ---- Track preferences --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct Preferences {
    /// ISO 639-2 code (`fre`, `eng`…). `None`: the file's default track.
    pub audio_language: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_mode: SubtitleMode,
    /// Unwatched episodes kept for each watchlist series, 1 to 10 (D73).
    pub watchlist_episodes: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct PreferencesUpdate {
    pub audio_language: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_mode: SubtitleMode,
    /// Omitted: unchanged.
    pub watchlist_episodes: Option<i64>,
}

pub async fn load_preferences(state: &AppState, user_id: i64) -> ApiResult<Preferences> {
    Ok(sqlx::query_as(
        "SELECT audio_language, subtitle_language, subtitle_mode, watchlist_episodes
         FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?)
}

#[utoipa::path(get, path = "/api/me/preferences", tag = "me", security(("bearer" = [])),
    responses((status = 200, body = Preferences)))]
pub async fn get_preferences(
    State(state): State<AppState>,
    current: CurrentDevice,
) -> ApiResult<Json<Preferences>> {
    Ok(Json(load_preferences(&state, current.user.id).await?))
}

fn valid_language_code(code: &Option<String>) -> bool {
    code.as_deref()
        .is_none_or(|c| c.len() == 3 && c.chars().all(|ch| ch.is_ascii_lowercase()))
}

#[utoipa::path(put, path = "/api/me/preferences", tag = "me", security(("bearer" = [])),
    request_body = PreferencesUpdate, responses((status = 200, body = Preferences)))]
pub async fn put_preferences(
    State(state): State<AppState>,
    current: CurrentDevice,
    Json(prefs): Json<PreferencesUpdate>,
) -> ApiResult<Json<Preferences>> {
    if prefs
        .watchlist_episodes
        .is_some_and(|n| !(1..=10).contains(&n))
    {
        return Err(ApiError::bad_request(
            "invalid_count",
            "watchlist_episodes must be between 1 and 10",
        ));
    }
    if !valid_language_code(&prefs.audio_language) || !valid_language_code(&prefs.subtitle_language)
    {
        return Err(ApiError::bad_request(
            "invalid_language",
            "track languages are ISO 639-2 codes such as fre or eng",
        ));
    }
    sqlx::query(
        "UPDATE users SET audio_language = ?, subtitle_language = ?, subtitle_mode = ?,
           watchlist_episodes = COALESCE(?, watchlist_episodes) WHERE id = ?",
    )
    .bind(&prefs.audio_language)
    .bind(&prefs.subtitle_language)
    .bind(prefs.subtitle_mode)
    .bind(prefs.watchlist_episodes)
    .bind(current.user.id)
    .execute(&state.db)
    .await?;
    Ok(Json(load_preferences(&state, current.user.id).await?))
}
