//! Libraries (admin), browsing (any signed-in user), manual identification, images.

use std::path::{Component, Path as FsPath};

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    AppState,
    auth::{Admin, CurrentDevice},
    db::now,
    error::{ApiError, ApiResult},
    library::{
        media_info::{self, MediaInfo},
        scanner::{self, ScanStatus},
    },
    metadata::{self, tmdb::TmdbClient},
};

// ---- Libraries (admin) ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum LibraryKind {
    Movies,
    Series,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub kind: LibraryKind,
    /// Relative to the media root.
    pub path: String,
    pub last_scan_at: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateLibraryRequest {
    pub name: String,
    pub kind: LibraryKind,
    /// Folder relative to the media root, e.g. `Films`.
    pub path: String,
}

/// Normalizes a library path and checks it is a folder inside the media root
/// (no `..`, no absolute path, no symlink escaping the root).
fn validate_library_path(media_root: &FsPath, raw: &str) -> ApiResult<String> {
    let invalid = |msg: &str| ApiError::bad_request("invalid_path", msg.to_owned());
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(invalid("path must name a folder inside the media root"));
    }
    let relative = FsPath::new(trimmed);
    if !relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid("path must be relative, without '..'"));
    }
    let root = media_root
        .canonicalize()
        .map_err(|_| invalid("the media root is not accessible"))?;
    let full = media_root
        .join(relative)
        .canonicalize()
        .map_err(|_| invalid("folder not found in the media root"))?;
    if !full.starts_with(&root) || !full.is_dir() {
        return Err(invalid("path must be a folder inside the media root"));
    }
    Ok(trimmed.to_owned())
}

#[utoipa::path(get, path = "/api/admin/libraries", tag = "admin", security(("bearer" = [])),
    responses((status = 200, body = Vec<Library>)))]
pub async fn list_libraries(
    State(state): State<AppState>,
    _: Admin,
) -> ApiResult<Json<Vec<Library>>> {
    Ok(Json(
        sqlx::query_as("SELECT id, name, kind, path, last_scan_at FROM libraries ORDER BY name")
            .fetch_all(&state.db)
            .await?,
    ))
}

/// Adds a library and starts its first scan.
#[utoipa::path(post, path = "/api/admin/libraries", tag = "admin", security(("bearer" = [])),
    request_body = CreateLibraryRequest,
    responses(
        (status = 201, body = Library),
        (status = 400, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn create_library(
    State(state): State<AppState>,
    _: Admin,
    Json(req): Json<CreateLibraryRequest>,
) -> ApiResult<(StatusCode, Json<Library>)> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(ApiError::bad_request(
            "invalid_name",
            "name must be 1 to 64 characters",
        ));
    }
    let path = validate_library_path(&state.config.media_dir, &req.path)?;
    let result = sqlx::query_as(
        "INSERT INTO libraries (name, kind, path, created_at) VALUES (?, ?, ?, ?)
         RETURNING id, name, kind, path, last_scan_at",
    )
    .bind(name)
    .bind(req.kind)
    .bind(&path)
    .bind(now())
    .fetch_one(&state.db)
    .await;
    let library: Library = match result {
        Ok(library) => library,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::conflict(
                "library_exists",
                "a library already uses this folder",
            ));
        }
        Err(e) => return Err(e.into()),
    };
    scanner::spawn_scan(&state, library.id);
    Ok((StatusCode::CREATED, Json(library)))
}

/// Removes a library from BatVision (files on disk are never touched).
#[utoipa::path(delete, path = "/api/admin/libraries/{id}", tag = "admin", security(("bearer" = [])),
    params(("id" = i64, Path)),
    responses(
        (status = 204),
        (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn delete_library(
    State(state): State<AppState>,
    _: Admin,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if state.scans.is_running(id) {
        return Err(ApiError::conflict(
            "scan_running",
            "wait for the scan to finish",
        ));
    }
    let mut tx = state.db.begin().await?;
    // Files, translations and ratings point to items without a foreign key: clean them first.
    sqlx::query(
        "DELETE FROM media_files WHERE
           (item_kind = 'movie' AND item_id IN (SELECT id FROM movies WHERE library_id = ?1))
           OR (item_kind = 'episode' AND item_id IN (
               SELECT e.id FROM episodes e JOIN series s ON s.id = e.series_id WHERE s.library_id = ?1))",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM translations WHERE
           (item_kind = 'movie' AND item_id IN (SELECT id FROM movies WHERE library_id = ?1))
           OR (item_kind = 'series' AND item_id IN (SELECT id FROM series WHERE library_id = ?1))
           OR (item_kind = 'season' AND item_id IN (
               SELECT se.id FROM seasons se JOIN series s ON s.id = se.series_id WHERE s.library_id = ?1))
           OR (item_kind = 'episode' AND item_id IN (
               SELECT e.id FROM episodes e JOIN series s ON s.id = e.series_id WHERE s.library_id = ?1))",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM ratings WHERE
           (item_kind = 'movie' AND item_id IN (SELECT id FROM movies WHERE library_id = ?1))
           OR (item_kind = 'series' AND item_id IN (SELECT id FROM series WHERE library_id = ?1))
           OR (item_kind = 'season' AND item_id IN (
               SELECT se.id FROM seasons se JOIN series s ON s.id = se.series_id WHERE s.library_id = ?1))
           OR (item_kind = 'episode' AND item_id IN (
               SELECT e.id FROM episodes e JOIN series s ON s.id = e.series_id WHERE s.library_id = ?1))",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    // Watch state and downloads too: SQLite gives a later item the id of a deleted one,
    // which would then inherit them.
    macro_rules! of_this_library {
        ($table:literal) => {
            concat!(
                "DELETE FROM ",
                $table,
                " WHERE (item_kind = 'movie' AND item_id IN (SELECT id FROM movies WHERE library_id = ?1))
                  OR (item_kind = 'episode' AND item_id IN (
                      SELECT e.id FROM episodes e JOIN series s ON s.id = e.series_id WHERE s.library_id = ?1))"
            )
        };
    }
    for sql in [
        of_this_library!("user_item_data"),
        of_this_library!("manual_downloads"),
        of_this_library!("device_downloads"),
    ] {
        sqlx::query(sql).bind(id).execute(&mut *tx).await?;
    }
    let deleted = sqlx::query("DELETE FROM libraries WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if deleted.rows_affected() == 1 {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(post, path = "/api/admin/libraries/{id}/scan", tag = "admin",
    security(("bearer" = [])), params(("id" = i64, Path)),
    responses(
        (status = 202),
        (status = 404, body = crate::error::ErrorBody),
        (status = 409, body = crate::error::ErrorBody)))]
pub async fn scan_library(
    State(state): State<AppState>,
    _: Admin,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM libraries WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    exists.ok_or(ApiError::NotFound)?;
    if scanner::spawn_scan(&state, id) {
        Ok(StatusCode::ACCEPTED)
    } else {
        Err(ApiError::conflict(
            "scan_running",
            "a scan is already running",
        ))
    }
}

#[utoipa::path(get, path = "/api/admin/scans", tag = "admin", security(("bearer" = [])),
    responses((status = 200, body = Vec<ScanStatus>)))]
pub async fn list_scans(State(state): State<AppState>, _: Admin) -> Json<Vec<ScanStatus>> {
    Json(state.scans.statuses())
}

// ---- Manual identification (admin) ------------------------------------------------------

fn require_tmdb(state: &AppState) -> ApiResult<&TmdbClient> {
    state.tmdb.as_deref().ok_or_else(|| {
        ApiError::conflict(
            "tmdb_not_configured",
            "set BATVISION_TMDB_TOKEN to fetch metadata",
        )
    })
}

#[derive(Deserialize, IntoParams)]
pub struct MetadataSearchQuery {
    #[param(inline)]
    pub kind: LibraryKind,
    pub query: String,
    pub year: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct MetadataCandidate {
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub poster_path: Option<String>,
    pub overview: Option<String>,
}

/// Searches TMDB, to pick the right match for a misidentified item.
#[utoipa::path(get, path = "/api/admin/metadata/search", tag = "admin",
    security(("bearer" = [])), params(MetadataSearchQuery),
    responses((status = 200, body = Vec<MetadataCandidate>)))]
pub async fn search_metadata(
    State(state): State<AppState>,
    Admin(admin): Admin,
    Query(q): Query<MetadataSearchQuery>,
) -> ApiResult<Json<Vec<MetadataCandidate>>> {
    let tmdb = require_tmdb(&state)?;
    let language = &admin.user.language;
    let results = match q.kind {
        LibraryKind::Movies => tmdb.search_movie(&q.query, q.year, language).await,
        LibraryKind::Series => tmdb.search_tv(&q.query, q.year, language).await,
    }
    .map_err(|e| ApiError::Upstream(e.to_string()))?;
    Ok(Json(
        results
            .into_iter()
            .map(|r| MetadataCandidate {
                tmdb_id: r.id,
                year: r.year(),
                title: r.title,
                poster_path: r.poster_path,
                overview: r.overview,
            })
            .collect(),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct IdentifyRequest {
    pub tmdb_id: i64,
}

/// Pins a movie to a TMDB id. Later scans never override a manual identification.
#[utoipa::path(post, path = "/api/admin/movies/{id}/identify", tag = "admin",
    security(("bearer" = [])), params(("id" = i64, Path)), request_body = IdentifyRequest,
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
pub async fn identify_movie(
    State(state): State<AppState>,
    _: Admin,
    Path(id): Path<i64>,
    Json(req): Json<IdentifyRequest>,
) -> ApiResult<StatusCode> {
    let tmdb = require_tmdb(&state)?;
    // Before anything is erased: a mistyped id must leave the movie as it was.
    tmdb.movie(req.tmdb_id, &state.config.default_language)
        .await
        .map_err(identify_error)?;
    let mut tx = state.db.begin().await?;
    let updated = sqlx::query(
        "UPDATE movies SET tmdb_id = ?, identify_locked = 1, metadata_updated_at = NULL WHERE id = ?",
    )
    .bind(req.tmdb_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    sqlx::query("DELETE FROM translations WHERE item_kind = 'movie' AND item_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM ratings WHERE item_kind = 'movie' AND item_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let languages = metadata::languages(&state.db, &state.config.default_language).await?;
    metadata::refresh_movie(&state.db, tmdb, id, &languages)
        .await
        .map_err(|e| ApiError::Upstream(format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Pins a series to a TMDB id and refetches its seasons and episodes.
#[utoipa::path(post, path = "/api/admin/series/{id}/identify", tag = "admin",
    security(("bearer" = [])), params(("id" = i64, Path)), request_body = IdentifyRequest,
    responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
pub async fn identify_series(
    State(state): State<AppState>,
    _: Admin,
    Path(id): Path<i64>,
    Json(req): Json<IdentifyRequest>,
) -> ApiResult<StatusCode> {
    let tmdb = require_tmdb(&state)?;
    // Before anything is erased: a mistyped id must leave the series as it was.
    tmdb.tv(req.tmdb_id, &state.config.default_language)
        .await
        .map_err(identify_error)?;
    let mut tx = state.db.begin().await?;
    let updated = sqlx::query(
        "UPDATE series SET tmdb_id = ?, identify_locked = 1, metadata_updated_at = NULL WHERE id = ?",
    )
    .bind(req.tmdb_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    sqlx::query(
        "DELETE FROM translations WHERE
           (item_kind = 'series' AND item_id = ?1)
           OR (item_kind = 'season' AND item_id IN (SELECT id FROM seasons WHERE series_id = ?1))
           OR (item_kind = 'episode' AND item_id IN (SELECT id FROM episodes WHERE series_id = ?1))",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM ratings WHERE
           (item_kind = 'series' AND item_id = ?1)
           OR (item_kind = 'season' AND item_id IN (SELECT id FROM seasons WHERE series_id = ?1))
           OR (item_kind = 'episode' AND item_id IN (SELECT id FROM episodes WHERE series_id = ?1))",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE episodes SET air_date = NULL, runtime_minutes = NULL, still_path = NULL WHERE series_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let languages = metadata::languages(&state.db, &state.config.default_language).await?;
    metadata::refresh_series(&state.db, tmdb, id, &languages)
        .await
        .map_err(|e| ApiError::Upstream(format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// A TMDB id that doesn't exist is the caller's mistake; anything else is TMDB's.
fn identify_error(err: metadata::tmdb::TmdbError) -> ApiError {
    match err {
        metadata::tmdb::TmdbError::NotFound => ApiError::NotFound,
        other => ApiError::Upstream(other.to_string()),
    }
}

// ---- Browsing ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RatingSource {
    Tmdb,
}

/// An average rating from one source. Ratings with fewer than 5 votes are left out.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Rating {
    pub source: RatingSource,
    /// Between 0 and `scale`.
    pub value: f64,
    /// Best possible value (10 for TMDB).
    pub scale: i64,
    pub votes: i64,
}

/// `ratings` column: a JSON array of [`Rating`] for one item, built in SQL.
type Ratings = sqlx::types::Json<Vec<Rating>>;

/// SQL expression for the ratings of an item (`kind` literal, `id` column), as a
/// JSON array, without those with fewer than 5 votes.
macro_rules! ratings_sql {
    ($kind:literal, $id:literal) => {
        concat!(
            "(SELECT json_group_array(json_object('source', r.source, 'value', r.value,
                'scale', r.scale, 'votes', r.votes))
              FROM ratings r WHERE r.item_kind = '",
            $kind,
            "' AND r.item_id = ",
            $id,
            " AND r.votes >= 5)"
        )
    };
}

/// SQL expression for the rating browse pages sort on: TMDB's, out of 1.
macro_rules! rating_sort_sql {
    ($kind:literal, $id:literal) => {
        concat!(
            "(SELECT r.value / r.scale FROM ratings r WHERE r.item_kind = '",
            $kind,
            "' AND r.item_id = ",
            $id,
            " AND r.source = 'tmdb' AND r.votes >= 5)"
        )
    };
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct LibrarySummary {
    pub id: i64,
    pub name: String,
    pub kind: LibraryKind,
}

#[utoipa::path(get, path = "/api/libraries", tag = "library", security(("bearer" = [])),
    responses((status = 200, body = Vec<LibrarySummary>)))]
pub async fn libraries(
    State(state): State<AppState>,
    _: CurrentDevice,
) -> ApiResult<Json<Vec<LibrarySummary>>> {
    Ok(Json(
        sqlx::query_as("SELECT id, name, kind FROM libraries ORDER BY name")
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// A to Z by default.
    #[default]
    Title,
    /// Most recently added first by default.
    Added,
    /// Most recent first by default; items without a year last.
    Year,
    /// Best rated first by default; unrated items last.
    Rating,
}

/// Sort direction (D75). Omitted: the sort's natural one (`asc` for title, else `desc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortOrder {
    fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Added => "added",
            Self::Year => "year",
            Self::Rating => "rating",
        }
    }
}

#[derive(Deserialize, IntoParams)]
pub struct BrowseQuery {
    pub library_id: Option<i64>,
    #[serde(default)]
    #[param(inline)]
    pub sort: SortOrder,
    #[param(inline)]
    pub order: Option<SortDirection>,
    /// Default 50, max 200.
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl BrowseQuery {
    /// 1 for ascending, -1 for descending.
    fn sign(&self) -> i64 {
        let natural = match self.sort {
            SortOrder::Title => SortDirection::Asc,
            _ => SortDirection::Desc,
        };
        match self.order.unwrap_or(natural) {
            SortDirection::Asc => 1,
            SortDirection::Desc => -1,
        }
    }
    fn limit(&self) -> i64 {
        self.limit.unwrap_or(50).clamp(1, 200)
    }
    fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct MovieSummary {
    pub id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub added_at: i64,
    pub duration_ms: Option<i64>,
    /// The caller's resume position (0 if not started or already watched).
    pub position_ms: i64,
    pub played: bool,
    #[schema(value_type = Vec<Rating>)]
    pub ratings: Ratings,
}

#[derive(Serialize, ToSchema)]
pub struct MoviePage {
    pub items: Vec<MovieSummary>,
    pub total: i64,
}

#[utoipa::path(get, path = "/api/movies", tag = "library", security(("bearer" = [])),
    params(BrowseQuery), responses((status = 200, body = MoviePage)))]
pub async fn movies(
    State(state): State<AppState>,
    current: CurrentDevice,
    Query(q): Query<BrowseQuery>,
) -> ApiResult<Json<MoviePage>> {
    Ok(Json(query_movies(&state, &current.user, &q, None).await?))
}

/// Movies as seen by `user`; `only_ids` restricts to a set of ids (search).
pub async fn query_movies(
    state: &AppState,
    user: &crate::auth::User,
    q: &BrowseQuery,
    only_ids: Option<&[i64]>,
) -> ApiResult<MoviePage> {
    let ids = only_ids.map(|ids| serde_json::to_string(ids).expect("serializable"));
    let items = sqlx::query_as(concat!(
        "SELECT * FROM (
           SELECT m.id,
             COALESCE(
               (SELECT title FROM translations WHERE item_kind = 'movie' AND item_id = m.id AND language = ?1),
               (SELECT title FROM translations WHERE item_kind = 'movie' AND item_id = m.id AND language = ?2),
               m.parsed_title) AS title,
             COALESCE(CAST(substr(m.release_date, 1, 4) AS INTEGER), m.parsed_year) AS year,
             m.poster_path, m.backdrop_path, m.added_at,
             (SELECT duration_ms FROM media_files mf WHERE mf.item_kind = 'movie' AND mf.item_id = m.id
                AND mf.missing_since IS NULL ORDER BY mf.size_bytes DESC LIMIT 1) AS duration_ms,
             COALESCE((SELECT position_ms FROM user_item_data u
                WHERE u.user_id = ?7 AND u.item_kind = 'movie' AND u.item_id = m.id), 0) AS position_ms,
             COALESCE((SELECT played FROM user_item_data u
                WHERE u.user_id = ?7 AND u.item_kind = 'movie' AND u.item_id = m.id), 0) AS played,
             ",
        ratings_sql!("movie", "m.id"),
        " AS ratings, ",
        rating_sort_sql!("movie", "m.id"),
        " AS rating_sort
           FROM movies m
           WHERE m.missing_since IS NULL AND (?3 IS NULL OR m.library_id = ?3)
             AND (?8 IS NULL OR m.id IN (SELECT value FROM json_each(?8))))
         -- ?9: 1 ascending, -1 descending. Items without a year or rating stay last.
         ORDER BY
           CASE WHEN ?4 = 'year' THEN year IS NULL ELSE 0 END,
           CASE WHEN ?4 = 'rating' THEN rating_sort IS NULL ELSE 0 END,
           CASE ?4 WHEN 'added' THEN added_at * ?9 WHEN 'year' THEN COALESCE(year, 0) * ?9
             WHEN 'rating' THEN COALESCE(rating_sort, 0) * ?9 ELSE 0 END,
           CASE WHEN ?4 = 'title' AND ?9 < 0 THEN title END COLLATE NOCASE DESC,
           title COLLATE NOCASE
         LIMIT ?5 OFFSET ?6",
    ))
    .bind(&user.language)
    .bind(&state.config.default_language)
    .bind(q.library_id)
    .bind(q.sort.as_str())
    .bind(q.limit())
    .bind(q.offset())
    .bind(user.id)
    .bind(&ids)
    .bind(q.sign())
    .fetch_all(&state.db)
    .await?;
    let total = sqlx::query_scalar(
        "SELECT COUNT(*) FROM movies WHERE missing_since IS NULL AND (?1 IS NULL OR library_id = ?1)
         AND (?2 IS NULL OR id IN (SELECT value FROM json_each(?2)))",
    )
    .bind(q.library_id)
    .bind(&ids)
    .fetch_one(&state.db)
    .await?;
    Ok(MoviePage { items, total })
}

#[derive(Serialize, ToSchema)]
pub struct FileInfo {
    pub id: i64,
    pub size_bytes: i64,
    /// `None` until the file has been analyzed (or if analysis failed).
    pub media: Option<MediaInfo>,
    /// Subtitle files next to the video.
    pub external_subtitles: Vec<ExternalSubtitleInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct ExternalSubtitleInfo {
    pub language: Option<String>,
    pub kind: media_info::TrackKind,
}

pub async fn file_info(state: &AppState, kind: &str, item_id: i64) -> ApiResult<Option<FileInfo>> {
    let row: Option<(i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, size_bytes, probe_json FROM media_files
         WHERE item_kind = ? AND item_id = ? AND missing_since IS NULL
         ORDER BY size_bytes DESC LIMIT 1",
    )
    .bind(kind)
    .bind(item_id)
    .fetch_optional(&state.db)
    .await?;
    let Some((id, size_bytes, probe)) = row else {
        return Ok(None);
    };
    let external_subtitles = sqlx::query_as::<_, (Option<String>, bool, bool)>(
        "SELECT language, forced, sdh FROM subtitle_files WHERE media_file_id = ? ORDER BY path",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(language, forced, sdh)| ExternalSubtitleInfo {
        language,
        kind: if forced {
            media_info::TrackKind::Forced
        } else if sdh {
            media_info::TrackKind::Sdh
        } else {
            media_info::TrackKind::Normal
        },
    })
    .collect();
    Ok(Some(FileInfo {
        id,
        size_bytes,
        media: probe.and_then(|json| media_info::parse(&json).ok()),
        external_subtitles,
    }))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct MovieDetail {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    pub overview: Option<String>,
    pub year: Option<i64>,
    pub release_date: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub tmdb_id: Option<i64>,
    pub added_at: i64,
    pub position_ms: i64,
    pub played: bool,
    pub in_watchlist: bool,
    #[schema(value_type = Vec<Rating>)]
    pub ratings: Ratings,
    #[sqlx(skip)]
    pub file: Option<FileInfo>,
}

#[utoipa::path(get, path = "/api/movies/{id}", tag = "library", security(("bearer" = [])),
    params(("id" = i64, Path)),
    responses((status = 200, body = MovieDetail), (status = 404, body = crate::error::ErrorBody)))]
pub async fn movie(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(id): Path<i64>,
) -> ApiResult<Json<MovieDetail>> {
    Ok(Json(load_movie_detail(&state, &current.user, id).await?))
}

pub async fn load_movie_detail(
    state: &AppState,
    user: &crate::auth::User,
    id: i64,
) -> ApiResult<MovieDetail> {
    let mut detail: MovieDetail = sqlx::query_as(concat!(
        "SELECT m.id, m.library_id,
           COALESCE(
             (SELECT title FROM translations WHERE item_kind = 'movie' AND item_id = m.id AND language = ?1),
             (SELECT title FROM translations WHERE item_kind = 'movie' AND item_id = m.id AND language = ?2),
             m.parsed_title) AS title,
           COALESCE(
             (SELECT overview FROM translations WHERE item_kind = 'movie' AND item_id = m.id AND language = ?1),
             (SELECT overview FROM translations WHERE item_kind = 'movie' AND item_id = m.id AND language = ?2)
           ) AS overview,
           COALESCE(CAST(substr(m.release_date, 1, 4) AS INTEGER), m.parsed_year) AS year,
           m.release_date, m.runtime_minutes, m.poster_path, m.backdrop_path, m.tmdb_id, m.added_at,
           COALESCE((SELECT position_ms FROM user_item_data u
              WHERE u.user_id = ?4 AND u.item_kind = 'movie' AND u.item_id = m.id), 0) AS position_ms,
           COALESCE((SELECT played FROM user_item_data u
              WHERE u.user_id = ?4 AND u.item_kind = 'movie' AND u.item_id = m.id), 0) AS played,
           EXISTS (SELECT 1 FROM watchlist w WHERE w.user_id = ?4 AND w.movie_id = m.id) AS in_watchlist,
           ",
        ratings_sql!("movie", "m.id"),
        " AS ratings
         FROM movies m WHERE m.id = ?3 AND m.missing_since IS NULL",
    ))
    .bind(&user.language)
    .bind(&state.config.default_language)
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    detail.file = file_info(state, "movie", id).await?;
    Ok(detail)
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct SeriesSummary {
    pub id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub episode_count: i64,
    /// Episodes the caller hasn't watched yet.
    pub unplayed_count: i64,
    pub added_at: i64,
    #[schema(value_type = Vec<Rating>)]
    pub ratings: Ratings,
}

#[derive(Serialize, ToSchema)]
pub struct SeriesPage {
    pub items: Vec<SeriesSummary>,
    pub total: i64,
}

#[utoipa::path(get, path = "/api/series", tag = "library", security(("bearer" = [])),
    params(BrowseQuery), responses((status = 200, body = SeriesPage)))]
pub async fn series_list(
    State(state): State<AppState>,
    current: CurrentDevice,
    Query(q): Query<BrowseQuery>,
) -> ApiResult<Json<SeriesPage>> {
    Ok(Json(query_series(&state, &current.user, &q, None).await?))
}

/// Series as seen by `user`; `only_ids` restricts to a set of ids (search).
pub async fn query_series(
    state: &AppState,
    user: &crate::auth::User,
    q: &BrowseQuery,
    only_ids: Option<&[i64]>,
) -> ApiResult<SeriesPage> {
    let ids = only_ids.map(|ids| serde_json::to_string(ids).expect("serializable"));
    let items = sqlx::query_as(concat!(
        "SELECT * FROM (
           SELECT s.id,
             COALESCE(
               (SELECT title FROM translations WHERE item_kind = 'series' AND item_id = s.id AND language = ?1),
               (SELECT title FROM translations WHERE item_kind = 'series' AND item_id = s.id AND language = ?2),
               s.parsed_title) AS title,
             COALESCE(CAST(substr(s.first_air_date, 1, 4) AS INTEGER), s.parsed_year) AS year,
             s.poster_path, s.backdrop_path,
             (SELECT COUNT(*) FROM episodes e WHERE e.series_id = s.id AND e.missing_since IS NULL)
               AS episode_count,
             (SELECT COUNT(*) FROM episodes e WHERE e.series_id = s.id AND e.missing_since IS NULL
                AND NOT EXISTS (SELECT 1 FROM user_item_data u WHERE u.user_id = ?7
                  AND u.item_kind = 'episode' AND u.item_id = e.id AND u.played = 1)) AS unplayed_count,
             -- A new episode brings the series back to the top of recently added.
             COALESCE((SELECT MAX(e.added_at) FROM episodes e
                       WHERE e.series_id = s.id AND e.missing_since IS NULL), s.added_at) AS added_at,
             ",
        ratings_sql!("series", "s.id"),
        " AS ratings, ",
        rating_sort_sql!("series", "s.id"),
        " AS rating_sort
           FROM series s
           WHERE s.missing_since IS NULL AND (?3 IS NULL OR s.library_id = ?3)
             AND (?8 IS NULL OR s.id IN (SELECT value FROM json_each(?8))))
         -- ?9: 1 ascending, -1 descending. Items without a year or rating stay last.
         ORDER BY
           CASE WHEN ?4 = 'year' THEN year IS NULL ELSE 0 END,
           CASE WHEN ?4 = 'rating' THEN rating_sort IS NULL ELSE 0 END,
           CASE ?4 WHEN 'added' THEN added_at * ?9 WHEN 'year' THEN COALESCE(year, 0) * ?9
             WHEN 'rating' THEN COALESCE(rating_sort, 0) * ?9 ELSE 0 END,
           CASE WHEN ?4 = 'title' AND ?9 < 0 THEN title END COLLATE NOCASE DESC,
           title COLLATE NOCASE
         LIMIT ?5 OFFSET ?6",
    ))
    .bind(&user.language)
    .bind(&state.config.default_language)
    .bind(q.library_id)
    .bind(q.sort.as_str())
    .bind(q.limit())
    .bind(q.offset())
    .bind(user.id)
    .bind(&ids)
    .bind(q.sign())
    .fetch_all(&state.db)
    .await?;
    let total = sqlx::query_scalar(
        "SELECT COUNT(*) FROM series WHERE missing_since IS NULL AND (?1 IS NULL OR library_id = ?1)
         AND (?2 IS NULL OR id IN (SELECT value FROM json_each(?2)))",
    )
    .bind(q.library_id)
    .bind(&ids)
    .fetch_one(&state.db)
    .await?;
    Ok(SeriesPage { items, total })
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct SeasonSummary {
    pub id: i64,
    pub season_number: i64,
    pub title: Option<String>,
    pub poster_path: Option<String>,
    pub episode_count: i64,
    #[schema(value_type = Vec<Rating>)]
    pub ratings: Ratings,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct SeriesDetail {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    pub overview: Option<String>,
    pub year: Option<i64>,
    pub first_air_date: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub tmdb_id: Option<i64>,
    /// In the caller's watchlist: its next episodes download automatically (D73).
    pub in_watchlist: bool,
    #[schema(value_type = Vec<Rating>)]
    pub ratings: Ratings,
    #[sqlx(skip)]
    pub seasons: Vec<SeasonSummary>,
    /// Episode the play button starts: the one in progress, else the next unwatched.
    #[sqlx(skip)]
    pub up_next: Option<EpisodeSummary>,
}

#[utoipa::path(get, path = "/api/series/{id}", tag = "library", security(("bearer" = [])),
    params(("id" = i64, Path)),
    responses((status = 200, body = SeriesDetail), (status = 404, body = crate::error::ErrorBody)))]
pub async fn series(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(id): Path<i64>,
) -> ApiResult<Json<SeriesDetail>> {
    let lang = &current.user.language;
    let default = &state.config.default_language;
    let mut detail: SeriesDetail = sqlx::query_as(concat!(
        "SELECT s.id, s.library_id,
           COALESCE(
             (SELECT title FROM translations WHERE item_kind = 'series' AND item_id = s.id AND language = ?1),
             (SELECT title FROM translations WHERE item_kind = 'series' AND item_id = s.id AND language = ?2),
             s.parsed_title) AS title,
           COALESCE(
             (SELECT overview FROM translations WHERE item_kind = 'series' AND item_id = s.id AND language = ?1),
             (SELECT overview FROM translations WHERE item_kind = 'series' AND item_id = s.id AND language = ?2)
           ) AS overview,
           COALESCE(CAST(substr(s.first_air_date, 1, 4) AS INTEGER), s.parsed_year) AS year,
           s.first_air_date, s.poster_path, s.backdrop_path, s.tmdb_id,
           EXISTS (SELECT 1 FROM watchlist_series w WHERE w.user_id = ?4 AND w.series_id = s.id)
             AS in_watchlist, ",
        ratings_sql!("series", "s.id"),
        " AS ratings
         FROM series s WHERE s.id = ?3 AND s.missing_since IS NULL",
    ))
    .bind(lang)
    .bind(default)
    .bind(id)
    .bind(current.user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    detail.seasons = sqlx::query_as(concat!(
        "SELECT se.id, se.season_number,
           COALESCE(
             (SELECT title FROM translations WHERE item_kind = 'season' AND item_id = se.id AND language = ?1),
             (SELECT title FROM translations WHERE item_kind = 'season' AND item_id = se.id AND language = ?2)
           ) AS title,
           se.poster_path,
           (SELECT COUNT(*) FROM episodes e WHERE e.season_id = se.id AND e.missing_since IS NULL)
             AS episode_count, ",
        ratings_sql!("season", "se.id"),
        " AS ratings
         FROM seasons se WHERE se.series_id = ?3
           AND EXISTS (SELECT 1 FROM episodes e WHERE e.season_id = se.id AND e.missing_since IS NULL)
         -- Specials (season 0) last.
         ORDER BY se.season_number = 0, se.season_number",
    ))
    .bind(lang)
    .bind(default)
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    if let Some(episode_id) =
        crate::userdata::series_up_next(&state.db, current.user.id, id).await?
    {
        detail.up_next = Some(load_episode(&state, &current.user, episode_id).await?);
    }
    Ok(Json(detail))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct EpisodeSummary {
    pub id: i64,
    pub series_id: i64,
    pub season_number: i64,
    pub episode_number: i64,
    pub episode_number_end: Option<i64>,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub still_path: Option<String>,
    pub duration_ms: Option<i64>,
    pub position_ms: i64,
    pub played: bool,
    #[schema(value_type = Vec<Rating>)]
    pub ratings: Ratings,
}

#[utoipa::path(get, path = "/api/series/{id}/seasons/{season}", tag = "library",
    security(("bearer" = [])), params(("id" = i64, Path), ("season" = i64, Path)),
    responses((status = 200, body = Vec<EpisodeSummary>)))]
pub async fn season_episodes(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path((id, season)): Path<(i64, i64)>,
) -> ApiResult<Json<Vec<EpisodeSummary>>> {
    Ok(Json(
        sqlx::query_as(concat!(
            "SELECT e.id, e.series_id, e.season_number, e.episode_number, e.episode_number_end,
               COALESCE(
                 (SELECT title FROM translations WHERE item_kind = 'episode' AND item_id = e.id AND language = ?1),
                 (SELECT title FROM translations WHERE item_kind = 'episode' AND item_id = e.id AND language = ?2)
               ) AS title,
               COALESCE(
                 (SELECT overview FROM translations WHERE item_kind = 'episode' AND item_id = e.id AND language = ?1),
                 (SELECT overview FROM translations WHERE item_kind = 'episode' AND item_id = e.id AND language = ?2)
               ) AS overview,
               e.air_date, e.runtime_minutes, e.still_path,
               (SELECT duration_ms FROM media_files mf
                WHERE mf.item_kind = 'episode' AND mf.item_id = e.id AND mf.missing_since IS NULL
                ORDER BY size_bytes DESC LIMIT 1) AS duration_ms,
               COALESCE((SELECT position_ms FROM user_item_data u
                  WHERE u.user_id = ?5 AND u.item_kind = 'episode' AND u.item_id = e.id), 0) AS position_ms,
               COALESCE((SELECT played FROM user_item_data u
                  WHERE u.user_id = ?5 AND u.item_kind = 'episode' AND u.item_id = e.id), 0) AS played,
               ",
            ratings_sql!("episode", "e.id"),
            " AS ratings
             FROM episodes e
             WHERE e.series_id = ?3 AND e.season_number = ?4 AND e.missing_since IS NULL
             ORDER BY e.episode_number",
        ))
        .bind(&current.user.language)
        .bind(&state.config.default_language)
        .bind(id)
        .bind(season)
        .bind(current.user.id)
        .fetch_all(&state.db)
        .await?,
    ))
}

#[derive(Serialize, ToSchema)]
pub struct EpisodeDetail {
    #[serde(flatten)]
    pub episode: EpisodeSummary,
    pub series_title: String,
    pub series_poster_path: Option<String>,
    pub series_backdrop_path: Option<String>,
    pub previous_episode_id: Option<i64>,
    pub next_episode_id: Option<i64>,
    pub file: Option<FileInfo>,
}

/// One episode as seen by `user` (title in their language, their progress).
pub async fn load_episode(
    state: &AppState,
    user: &crate::auth::User,
    id: i64,
) -> ApiResult<EpisodeSummary> {
    sqlx::query_as(concat!(
        "SELECT e.id, e.series_id, e.season_number, e.episode_number, e.episode_number_end,
           COALESCE(
             (SELECT title FROM translations WHERE item_kind = 'episode' AND item_id = e.id AND language = ?1),
             (SELECT title FROM translations WHERE item_kind = 'episode' AND item_id = e.id AND language = ?2)
           ) AS title,
           COALESCE(
             (SELECT overview FROM translations WHERE item_kind = 'episode' AND item_id = e.id AND language = ?1),
             (SELECT overview FROM translations WHERE item_kind = 'episode' AND item_id = e.id AND language = ?2)
           ) AS overview,
           e.air_date, e.runtime_minutes, e.still_path,
           (SELECT duration_ms FROM media_files mf
            WHERE mf.item_kind = 'episode' AND mf.item_id = e.id AND mf.missing_since IS NULL
            ORDER BY size_bytes DESC LIMIT 1) AS duration_ms,
           COALESCE((SELECT position_ms FROM user_item_data u
              WHERE u.user_id = ?4 AND u.item_kind = 'episode' AND u.item_id = e.id), 0) AS position_ms,
           COALESCE((SELECT played FROM user_item_data u
              WHERE u.user_id = ?4 AND u.item_kind = 'episode' AND u.item_id = e.id), 0) AS played,
           ",
        ratings_sql!("episode", "e.id"),
        " AS ratings
         FROM episodes e WHERE e.id = ?3 AND e.missing_since IS NULL",
    ))
    .bind(&user.language)
    .bind(&state.config.default_language)
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)
}

#[derive(sqlx::FromRow)]
pub struct SeriesHeader {
    pub title: String,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

pub async fn load_series_header(
    state: &AppState,
    user: &crate::auth::User,
    series_id: i64,
) -> ApiResult<SeriesHeader> {
    Ok(sqlx::query_as(
        "SELECT COALESCE(
           (SELECT title FROM translations WHERE item_kind = 'series' AND item_id = s.id AND language = ?1),
           (SELECT title FROM translations WHERE item_kind = 'series' AND item_id = s.id AND language = ?2),
           s.parsed_title) AS title, s.poster_path, s.backdrop_path
         FROM series s WHERE s.id = ?3",
    )
    .bind(&user.language)
    .bind(&state.config.default_language)
    .bind(series_id)
    .fetch_one(&state.db)
    .await?)
}

#[utoipa::path(get, path = "/api/episodes/{id}", tag = "library", security(("bearer" = [])),
    params(("id" = i64, Path)),
    responses((status = 200, body = EpisodeDetail), (status = 404, body = crate::error::ErrorBody)))]
pub async fn episode(
    State(state): State<AppState>,
    current: CurrentDevice,
    Path(id): Path<i64>,
) -> ApiResult<Json<EpisodeDetail>> {
    let episode = load_episode(&state, &current.user, id).await?;
    let header = load_series_header(&state, &current.user, episode.series_id).await?;
    let (previous_episode_id, next_episode_id) = crate::userdata::neighbours(
        &state.db,
        episode.series_id,
        episode.season_number,
        episode.episode_number,
    )
    .await?;
    let file = file_info(&state, "episode", id).await?;
    Ok(Json(EpisodeDetail {
        episode,
        series_title: header.title,
        series_poster_path: header.poster_path,
        series_backdrop_path: header.backdrop_path,
        previous_episode_id,
        next_episode_id,
        file,
    }))
}

// ---- Images -------------------------------------------------------------------------------

const IMAGE_SIZES: &[&str] = &[
    "w92", "w185", "w300", "w342", "w500", "w780", "w1280", "original",
];

fn valid_image_file(file: &str) -> bool {
    let Some((stem, ext)) = file.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && stem.len() <= 64
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && matches!(ext, "jpg" | "png" | "svg")
}

/// TMDB image (poster, backdrop, still), cached on disk. Public, so it works in
/// `<img>` tags; only images referenced by the library are served.
#[utoipa::path(get, path = "/api/images/{size}/{file}", tag = "library",
    params(("size" = String, Path), ("file" = String, Path)),
    responses((status = 200, content_type = "image/jpeg"), (status = 404)))]
pub async fn image(
    State(state): State<AppState>,
    Path((size, file)): Path<(String, String)>,
) -> ApiResult<Response> {
    if !IMAGE_SIZES.contains(&size.as_str()) || !valid_image_file(&file) {
        return Err(ApiError::NotFound);
    }
    let tmdb_path = format!("/{file}");
    let referenced: bool = sqlx::query_scalar(
        "SELECT EXISTS (
           SELECT 1 FROM movies WHERE poster_path = ?1 OR backdrop_path = ?1
           UNION ALL SELECT 1 FROM series WHERE poster_path = ?1 OR backdrop_path = ?1
           UNION ALL SELECT 1 FROM seasons WHERE poster_path = ?1
           UNION ALL SELECT 1 FROM episodes WHERE still_path = ?1)",
    )
    .bind(&tmdb_path)
    .fetch_one(&state.db)
    .await?;
    if !referenced {
        return Err(ApiError::NotFound);
    }

    let cached = state.config.data_dir.join("images").join(&size).join(&file);
    let bytes = match tokio::fs::read(&cached).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let tmdb = state.tmdb.as_deref().ok_or(ApiError::NotFound)?;
            let bytes = tmdb
                .image(&size, &tmdb_path)
                .await
                .map_err(|e| ApiError::Upstream(e.to_string()))?;
            if let Some(dir) = cached.parent() {
                tokio::fs::create_dir_all(dir)
                    .await
                    .map_err(anyhow::Error::from)?;
            }
            // Write then rename: a concurrent reader never sees a partial file.
            // A name of its own: two first requests for one image don't share it.
            let tmp =
                cached.with_extension(format!("{}.tmp", crate::auth::crypto::random_token("")));
            tokio::fs::write(&tmp, &bytes)
                .await
                .map_err(anyhow::Error::from)?;
            tokio::fs::rename(&tmp, &cached)
                .await
                .map_err(anyhow::Error::from)?;
            bytes
        }
    };
    let content_type = match file.rsplit_once('.').map(|(_, e)| e) {
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        _ => "image/jpeg",
    };
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(bytes))
        .expect("valid response"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_paths_must_stay_inside_the_media_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("Films/Sub")).unwrap();
        assert_eq!(
            validate_library_path(root.path(), "Films").unwrap(),
            "Films"
        );
        assert_eq!(
            validate_library_path(root.path(), "/Films/Sub/").unwrap(),
            "Films/Sub"
        );
        assert!(validate_library_path(root.path(), "../etc").is_err());
        assert!(validate_library_path(root.path(), "Films/../..").is_err());
        assert!(validate_library_path(root.path(), "Missing").is_err());
        assert!(validate_library_path(root.path(), "").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_the_media_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        assert!(validate_library_path(root.path(), "escape").is_err());
    }

    #[test]
    fn image_file_names_are_strict() {
        assert!(valid_image_file("abc123XYZ.jpg"));
        assert!(!valid_image_file("../x.jpg"));
        assert!(!valid_image_file("x.exe"));
        assert!(!valid_image_file(".jpg"));
    }
}
