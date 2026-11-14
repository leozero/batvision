//! Sagas (D101): TMDB movie collections, with the movies the library has.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    AppState,
    auth::{CurrentDevice, User},
    error::{ApiError, ApiResult},
    routes::library::{BrowseQuery, MovieSummary, SortOrder, query_movies},
};

/// SQL expression for a collection's name (collection `c`): the user's language
/// (?1), the server's (?2), then any.
macro_rules! name_sql {
    () => {
        "COALESCE(
          (SELECT name FROM collection_translations WHERE collection_id = c.id AND language = ?1),
          (SELECT name FROM collection_translations WHERE collection_id = c.id AND language = ?2),
          (SELECT name FROM collection_translations WHERE collection_id = c.id AND name IS NOT NULL
             ORDER BY language LIMIT 1),
          '')"
    };
}

/// A saga shows in the list once the library has this many of its movies.
const MIN_MOVIES_LISTED: i64 = 2;

/// The saga a movie belongs to.
#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct CollectionLink {
    pub id: i64,
    pub name: String,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct CollectionSummary {
    pub id: i64,
    pub name: String,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    /// Movies of the saga in the library.
    pub movie_count: i64,
    /// Released movies of the saga, in the library or not. `None` until TMDB told.
    pub total_count: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct CollectionDetail {
    pub id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    /// Released movies of the saga, in the library or not. `None` until TMDB told.
    pub total_count: Option<i64>,
    /// The saga's movies in the library, in release order.
    pub movies: Vec<MovieSummary>,
}

/// The saga of a movie, if it has one with a name.
pub async fn movie_collection(
    state: &AppState,
    user: &User,
    movie_id: i64,
) -> ApiResult<Option<CollectionLink>> {
    let link: Option<CollectionLink> = sqlx::query_as(concat!(
        "SELECT c.id, ",
        name_sql!(),
        " AS name FROM movies m JOIN collections c ON c.id = m.collection_id WHERE m.id = ?3"
    ))
    .bind(&user.language)
    .bind(&state.config.default_language)
    .bind(movie_id)
    .fetch_optional(&state.db)
    .await?;
    Ok(link.filter(|l| !l.name.is_empty()))
}

/// Sagas with at least two movies in the library, by name.
#[utoipa::path(get, path = "/api/collections", tag = "library", security(("bearer" = [])),
    responses((status = 200, body = Vec<CollectionSummary>)))]
pub async fn collections(
    State(state): State<AppState>,
    current: CurrentDevice,
) -> ApiResult<Json<Vec<CollectionSummary>>> {
    let items = sqlx::query_as(concat!(
        "SELECT * FROM (
           SELECT c.id, ",
        name_sql!(),
        " AS name, c.poster_path, c.backdrop_path,
             (SELECT COUNT(*) FROM movies m
                WHERE m.collection_id = c.id AND m.missing_since IS NULL) AS movie_count,
             c.parts_count AS total_count
           FROM collections c)
         WHERE movie_count >= ?3 AND name != ''
         ORDER BY name COLLATE NOCASE"
    ))
    .bind(&current.user.language)
    .bind(&state.config.default_language)
    .bind(MIN_MOVIES_LISTED)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(items))
}

#[derive(sqlx::FromRow)]
struct CollectionRow {
    id: i64,
    name: String,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    total_count: Option<i64>,
}

/// A saga and its movies in the library, in release order. 404 when the library
/// has none of them.
#[utoipa::path(get, path = "/api/collections/{id}", tag = "library", security(("bearer" = [])),
    params(("id" = i64, Path)),
    responses((status = 200, body = CollectionDetail), (status = 404, body = crate::error::ErrorBody)))]
pub async fn collection(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(id): Path<i64>,
) -> ApiResult<Json<CollectionDetail>> {
    let row: CollectionRow = sqlx::query_as(concat!(
        "SELECT c.id, ",
        name_sql!(),
        " AS name,
           COALESCE(
             (SELECT overview FROM collection_translations WHERE collection_id = c.id AND language = ?1),
             (SELECT overview FROM collection_translations WHERE collection_id = c.id AND language = ?2)
           ) AS overview,
           c.poster_path, c.backdrop_path, c.parts_count AS total_count
         FROM collections c WHERE c.id = ?3"
    ))
    .bind(&current.user.language)
    .bind(&state.config.default_language)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;

    // Release order; a movie TMDB gives no date for goes by its folder's year, then last.
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM movies WHERE collection_id = ? AND missing_since IS NULL
         ORDER BY COALESCE(release_date, CAST(parsed_year AS TEXT)) IS NULL,
           COALESCE(release_date, CAST(parsed_year AS TEXT)), id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    if ids.is_empty() {
        return Err(ApiError::NotFound);
    }
    let query = BrowseQuery {
        library_id: None,
        sort: SortOrder::Title,
        order: None,
        limit: Some(200),
        offset: None,
    };
    let mut movies = query_movies(&state, &current.user, &query, Some(&ids))
        .await?
        .items;
    let rank: HashMap<i64, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    movies.sort_by_key(|m| rank.get(&m.id).copied().unwrap_or(usize::MAX));

    Ok(Json(CollectionDetail {
        id: row.id,
        name: row.name,
        overview: row.overview,
        poster_path: row.poster_path,
        backdrop_path: row.backdrop_path,
        total_count: row.total_count,
        movies,
    }))
}
