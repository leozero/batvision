//! Library scan: walk the folder, sync the database, probe new files, fetch metadata.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, UNIX_EPOCH},
};

use serde::Serialize;
use tokio::{process::Command, sync::Semaphore, task::JoinSet};
use utoipa::ToSchema;

use super::{
    media_info,
    naming::{self, DetectedMovie, DetectedSeries},
};
use crate::playback::subtitles;
use crate::{
    AppState,
    db::{Db, now},
    metadata,
};

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct ScanReport {
    pub movies: usize,
    pub series: usize,
    pub episodes: usize,
    /// Items not found anymore (hidden until their files come back).
    pub newly_missing: u64,
    pub files_probed: usize,
    pub probe_errors: usize,
    pub metadata_refreshed: usize,
    pub metadata_errors: usize,
    /// Why metadata was skipped, if it was (no TMDB token…).
    pub metadata_skipped: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ScanStatus {
    pub library_id: i64,
    pub running: bool,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub report: Option<ScanReport>,
    pub error: Option<String>,
}

/// Tracks scans so a library is never scanned twice at the same time.
#[derive(Default)]
pub struct ScanManager {
    status: Mutex<HashMap<i64, ScanStatus>>,
}

impl ScanManager {
    pub fn statuses(&self) -> Vec<ScanStatus> {
        let mut all: Vec<_> = self.status.lock().unwrap().values().cloned().collect();
        all.sort_by_key(|s| s.library_id);
        all
    }

    pub fn is_running(&self, library_id: i64) -> bool {
        self.status
            .lock()
            .unwrap()
            .get(&library_id)
            .is_some_and(|s| s.running)
    }

    fn try_begin(&self, library_id: i64) -> bool {
        let mut status = self.status.lock().unwrap();
        let entry = status.entry(library_id).or_insert(ScanStatus {
            library_id,
            running: false,
            started_at: None,
            finished_at: None,
            report: None,
            error: None,
        });
        if entry.running {
            return false;
        }
        entry.running = true;
        entry.started_at = Some(now());
        true
    }

    fn finish(&self, library_id: i64, result: anyhow::Result<ScanReport>) {
        let mut status = self.status.lock().unwrap();
        if let Some(entry) = status.get_mut(&library_id) {
            entry.running = false;
            entry.finished_at = Some(now());
            match result {
                Ok(report) => {
                    entry.report = Some(report);
                    entry.error = None;
                }
                Err(err) => entry.error = Some(format!("{err:#}")),
            }
        }
    }
}

/// Starts a scan in the background. Returns false if one is already running.
pub fn spawn_scan(state: &AppState, library_id: i64) -> bool {
    if !state.scans.try_begin(library_id) {
        return false;
    }
    let state = state.clone();
    tokio::spawn(async move {
        let result = scan_library(&state, library_id).await;
        match &result {
            Ok(report) => tracing::info!(library_id, ?report, "scan finished"),
            Err(err) => tracing::error!(library_id, error = %format!("{err:#}"), "scan failed"),
        }
        state.scans.finish(library_id, result);
    });
    true
}

/// Runs a scan to completion (used by `spawn_scan` and by tests).
pub async fn run_scan(state: &AppState, library_id: i64) -> anyhow::Result<ScanReport> {
    anyhow::ensure!(
        state.scans.try_begin(library_id),
        "a scan is already running"
    );
    let result = scan_library(state, library_id).await;
    let report = result
        .as_ref()
        .map(Clone::clone)
        .map_err(|e| anyhow::anyhow!("{e:#}"));
    state.scans.finish(library_id, result);
    report
}

/// Periodically scans every library.
pub fn spawn_scheduler(state: AppState) {
    let minutes = state.config.scan_interval_minutes;
    if minutes == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(minutes * 60));
        loop {
            interval.tick().await;
            match sqlx::query_scalar::<_, i64>("SELECT id FROM libraries")
                .fetch_all(&state.db)
                .await
            {
                Ok(ids) => {
                    for id in ids {
                        spawn_scan(&state, id);
                    }
                }
                Err(err) => tracing::error!(error = %err, "scheduler could not list libraries"),
            }
        }
    });
}

struct ScannedFile {
    size: u64,
    modified: i64,
    /// Subtitle files next to the video (relative to the library), with what their name says.
    subtitles: Vec<(PathBuf, subtitles::Sidecar)>,
}

/// Lists files under `root` (relative paths), off the async runtime.
async fn walk(root: PathBuf) -> anyhow::Result<HashMap<PathBuf, ScannedFile>> {
    tokio::task::spawn_blocking(move || {
        anyhow::ensure!(
            root.is_dir(),
            "library folder {} does not exist",
            root.display()
        );
        let mut files = HashMap::new();
        let mut subtitle_files = Vec::new();
        for entry in walkdir::WalkDir::new(&root).follow_links(true) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    tracing::warn!(error = %err, "skipping unreadable path");
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry.path().strip_prefix(&root)?.to_path_buf();
            let is_subtitle = relative
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| {
                    subtitles::SUBTITLE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())
                });
            if is_subtitle {
                subtitle_files.push(relative);
                continue;
            }
            if naming::should_ignore(&relative) {
                continue;
            }
            let meta = entry.metadata()?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default();
            files.insert(
                relative,
                ScannedFile {
                    size: meta.len(),
                    modified,
                    subtitles: Vec::new(),
                },
            );
        }
        // Attach each subtitle file to the video it's named after, in the same folder.
        for subtitle in subtitle_files {
            for (video, file) in files.iter_mut() {
                if video.parent() != subtitle.parent() {
                    continue;
                }
                let Some(stem) = video.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if let Some(sidecar) = subtitles::parse_sidecar(stem, &subtitle) {
                    file.subtitles.push((subtitle.clone(), sidecar));
                    break;
                }
            }
        }
        Ok(files)
    })
    .await?
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

async fn upsert_file(
    db: &Db,
    path: &str,
    kind: &str,
    item_id: i64,
    file: &ScannedFile,
    library_path: &Path,
) -> anyhow::Result<()> {
    // A changed size or date means a new file: its probe is redone.
    let file_id: i64 = sqlx::query_as(
        "INSERT INTO media_files (path, item_kind, item_id, size_bytes, modified_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (path) DO UPDATE SET
           item_kind = excluded.item_kind,
           item_id = excluded.item_id,
           missing_since = NULL,
           probe_json = CASE WHEN media_files.size_bytes != excluded.size_bytes
               OR media_files.modified_at != excluded.modified_at THEN NULL ELSE media_files.probe_json END,
           probe_error = CASE WHEN media_files.size_bytes != excluded.size_bytes
               OR media_files.modified_at != excluded.modified_at THEN NULL ELSE media_files.probe_error END,
           duration_ms = CASE WHEN media_files.size_bytes != excluded.size_bytes
               OR media_files.modified_at != excluded.modified_at THEN NULL ELSE media_files.duration_ms END,
           size_bytes = excluded.size_bytes,
           modified_at = excluded.modified_at
         RETURNING id",
    )
    .bind(path)
    .bind(kind)
    .bind(item_id)
    .bind(file.size as i64)
    .bind(file.modified)
    .fetch_one(db)
    .await
    .map(|row: (i64,)| row.0)?;
    sync_subtitles(db, file_id, file, library_path).await
}

async fn sync_subtitles(
    db: &Db,
    media_file_id: i64,
    file: &ScannedFile,
    library_path: &Path,
) -> anyhow::Result<()> {
    let mut seen = Vec::new();
    for (relative, sidecar) in &file.subtitles {
        let path = path_str(&library_path.join(relative));
        sqlx::query(
            "INSERT INTO subtitle_files (media_file_id, path, format, language, forced, sdh)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (path) DO UPDATE SET media_file_id = excluded.media_file_id,
               format = excluded.format, language = excluded.language,
               forced = excluded.forced, sdh = excluded.sdh",
        )
        .bind(media_file_id)
        .bind(&path)
        .bind(&sidecar.format)
        .bind(&sidecar.language)
        .bind(sidecar.forced)
        .bind(sidecar.sdh)
        .execute(db)
        .await?;
        seen.push(path);
    }
    sqlx::query(
        "DELETE FROM subtitle_files WHERE media_file_id = ?
         AND path NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(media_file_id)
    .bind(serde_json::to_string(&seen)?)
    .execute(db)
    .await?;
    Ok(())
}

async fn sync_movies(
    db: &Db,
    library_id: i64,
    detected: &[DetectedMovie],
    files: &HashMap<PathBuf, ScannedFile>,
    library_path: &Path,
    report: &mut ScanReport,
) -> anyhow::Result<()> {
    let now = now();
    let mut seen_ids = Vec::new();
    let mut seen_files = Vec::new();
    for movie in detected {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO movies (library_id, source_path, parsed_title, parsed_year, tmdb_id, added_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (source_path) DO UPDATE SET
               library_id = excluded.library_id,
               parsed_title = excluded.parsed_title,
               parsed_year = excluded.parsed_year,
               missing_since = NULL,
               metadata_updated_at = CASE WHEN movies.identify_locked = 0
                   AND ((excluded.tmdb_id IS NOT NULL AND excluded.tmdb_id IS NOT movies.tmdb_id)
                        OR (movies.tmdb_id IS NULL AND excluded.tmdb_id IS NULL
                            AND (excluded.parsed_title IS NOT movies.parsed_title
                                 OR excluded.parsed_year IS NOT movies.parsed_year)))
                   THEN NULL ELSE movies.metadata_updated_at END,
               tmdb_id = CASE WHEN movies.identify_locked = 1 THEN movies.tmdb_id
                   ELSE COALESCE(excluded.tmdb_id, movies.tmdb_id) END
             RETURNING id",
        )
        .bind(library_id)
        .bind(path_str(&movie.source_path))
        .bind(&movie.name.title)
        .bind(movie.name.year)
        .bind(movie.name.tmdb_id)
        .bind(now)
        .fetch_one(db)
        .await?;
        let relative = movie.file.strip_prefix(library_path)?;
        let file_path = path_str(&movie.file);
        upsert_file(db, &file_path, "movie", id, &files[relative], library_path).await?;
        seen_ids.push(id);
        seen_files.push(file_path);
    }
    report.movies = detected.len();

    let missing = sqlx::query(
        "UPDATE movies SET missing_since = ? WHERE library_id = ? AND missing_since IS NULL
         AND id NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(now)
    .bind(library_id)
    .bind(serde_json::to_string(&seen_ids)?)
    .execute(db)
    .await?;
    report.newly_missing += missing.rows_affected();

    sqlx::query(
        "UPDATE media_files SET missing_since = ? WHERE missing_since IS NULL AND item_kind = 'movie'
         AND item_id IN (SELECT id FROM movies WHERE library_id = ?)
         AND path NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(now)
    .bind(library_id)
    .bind(serde_json::to_string(&seen_files)?)
    .execute(db)
    .await?;
    Ok(())
}

async fn sync_series(
    db: &Db,
    library_id: i64,
    detected: &[DetectedSeries],
    files: &HashMap<PathBuf, ScannedFile>,
    library_path: &Path,
    report: &mut ScanReport,
) -> anyhow::Result<()> {
    let now = now();
    let mut seen_series = Vec::new();
    let mut seen_episodes = Vec::new();
    let mut seen_files = Vec::new();

    for show in detected {
        let series_id: i64 = sqlx::query_scalar(
            "INSERT INTO series (library_id, source_path, parsed_title, parsed_year, tmdb_id, added_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (source_path) DO UPDATE SET
               library_id = excluded.library_id,
               parsed_title = excluded.parsed_title,
               parsed_year = excluded.parsed_year,
               missing_since = NULL,
               metadata_updated_at = CASE WHEN series.identify_locked = 0
                   AND ((excluded.tmdb_id IS NOT NULL AND excluded.tmdb_id IS NOT series.tmdb_id)
                        OR (series.tmdb_id IS NULL AND excluded.tmdb_id IS NULL
                            AND (excluded.parsed_title IS NOT series.parsed_title
                                 OR excluded.parsed_year IS NOT series.parsed_year)))
                   THEN NULL ELSE series.metadata_updated_at END,
               tmdb_id = CASE WHEN series.identify_locked = 1 THEN series.tmdb_id
                   ELSE COALESCE(excluded.tmdb_id, series.tmdb_id) END
             RETURNING id",
        )
        .bind(library_id)
        .bind(path_str(&show.source_path))
        .bind(&show.name.title)
        .bind(show.name.year)
        .bind(show.name.tmdb_id)
        .bind(now)
        .fetch_one(db)
        .await?;
        seen_series.push(series_id);

        for episode in &show.episodes {
            let season_id: i64 = sqlx::query_scalar(
                "INSERT INTO seasons (series_id, season_number) VALUES (?, ?)
                 ON CONFLICT (series_id, season_number) DO UPDATE SET season_number = excluded.season_number
                 RETURNING id",
            )
            .bind(series_id)
            .bind(episode.season)
            .fetch_one(db)
            .await?;
            let episode_id: i64 = sqlx::query_scalar(
                "INSERT INTO episodes (series_id, season_id, season_number, episode_number,
                   episode_number_end, added_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT (series_id, season_number, episode_number) DO UPDATE SET
                   season_id = excluded.season_id,
                   episode_number_end = excluded.episode_number_end,
                   missing_since = NULL
                 RETURNING id",
            )
            .bind(series_id)
            .bind(season_id)
            .bind(episode.season)
            .bind(episode.episode)
            .bind(episode.episode_end)
            .bind(now)
            .fetch_one(db)
            .await?;
            let relative = episode.file.strip_prefix(library_path)?;
            let file_path = path_str(&episode.file);
            upsert_file(
                db,
                &file_path,
                "episode",
                episode_id,
                &files[relative],
                library_path,
            )
            .await?;
            seen_episodes.push(episode_id);
            seen_files.push(file_path);
        }
        report.episodes += show.episodes.len();
    }
    report.series = detected.len();

    let missing_series = sqlx::query(
        "UPDATE series SET missing_since = ? WHERE library_id = ? AND missing_since IS NULL
         AND id NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(now)
    .bind(library_id)
    .bind(serde_json::to_string(&seen_series)?)
    .execute(db)
    .await?;
    let missing_episodes = sqlx::query(
        "UPDATE episodes SET missing_since = ? WHERE missing_since IS NULL
         AND series_id IN (SELECT id FROM series WHERE library_id = ?)
         AND id NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(now)
    .bind(library_id)
    .bind(serde_json::to_string(&seen_episodes)?)
    .execute(db)
    .await?;
    report.newly_missing += missing_series.rows_affected() + missing_episodes.rows_affected();

    sqlx::query(
        "UPDATE media_files SET missing_since = ? WHERE missing_since IS NULL AND item_kind = 'episode'
         AND item_id IN (SELECT e.id FROM episodes e JOIN series s ON s.id = e.series_id
                         WHERE s.library_id = ?)
         AND path NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(now)
    .bind(library_id)
    .bind(serde_json::to_string(&seen_files)?)
    .execute(db)
    .await?;
    Ok(())
}

/// Runs `ffprobe` and returns its JSON output.
pub async fn ffprobe(ffprobe: &Path, file: &Path) -> Result<String, String> {
    let run = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(file)
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(120), run).await {
        Err(_) => Err("ffprobe timed out".into()),
        Ok(Err(err)) => Err(format!("could not run ffprobe: {err}")),
        Ok(Ok(out)) if !out.status.success() => Err(String::from_utf8_lossy(&out.stderr)
            .trim()
            .chars()
            .take(500)
            .collect()),
        Ok(Ok(out)) => String::from_utf8(out.stdout).map_err(|e| e.to_string()),
    }
}

async fn probe_new_files(
    state: &AppState,
    library_id: i64,
    report: &mut ScanReport,
) -> anyhow::Result<()> {
    let pending: Vec<(i64, String)> = sqlx::query_as(
        "SELECT mf.id, mf.path FROM media_files mf
         WHERE mf.probe_json IS NULL AND mf.probe_error IS NULL AND mf.missing_since IS NULL AND (
           (mf.item_kind = 'movie' AND mf.item_id IN (SELECT id FROM movies WHERE library_id = ?))
           OR (mf.item_kind = 'episode' AND mf.item_id IN (
               SELECT e.id FROM episodes e JOIN series s ON s.id = e.series_id WHERE s.library_id = ?)))",
    )
    .bind(library_id)
    .bind(library_id)
    .fetch_all(&state.db)
    .await?;

    let limiter = Arc::new(Semaphore::new(4));
    let mut tasks = JoinSet::new();
    for (id, path) in pending {
        let limiter = limiter.clone();
        let full = state.config.media_dir.join(&path);
        let bin = state.config.ffprobe.clone();
        tasks.spawn(async move {
            let _permit = limiter.acquire_owned().await.expect("semaphore open");
            (id, path, ffprobe(&bin, &full).await)
        });
    }
    while let Some(joined) = tasks.join_next().await {
        let (id, path, result) = joined?;
        match result {
            Ok(json) => {
                let duration = media_info::parse(&json).ok().and_then(|i| i.duration_ms);
                sqlx::query("UPDATE media_files SET probe_json = ?, duration_ms = ? WHERE id = ?")
                    .bind(&json)
                    .bind(duration)
                    .bind(id)
                    .execute(&state.db)
                    .await?;
                report.files_probed += 1;
            }
            Err(err) => {
                tracing::warn!(path, error = %err, "ffprobe failed");
                sqlx::query("UPDATE media_files SET probe_error = ? WHERE id = ?")
                    .bind(err)
                    .bind(id)
                    .execute(&state.db)
                    .await?;
                report.probe_errors += 1;
            }
        }
    }
    Ok(())
}

async fn scan_library(state: &AppState, library_id: i64) -> anyhow::Result<ScanReport> {
    let (kind, path): (String, String) =
        sqlx::query_as("SELECT kind, path FROM libraries WHERE id = ?")
            .bind(library_id)
            .fetch_one(&state.db)
            .await?;
    let library_path = PathBuf::from(&path);
    let files = walk(state.config.media_dir.join(&library_path)).await?;
    let listing: Vec<(PathBuf, u64)> = files.iter().map(|(p, f)| (p.clone(), f.size)).collect();

    let mut report = ScanReport::default();
    match kind.as_str() {
        "movies" => {
            let detected = naming::detect_movies(&library_path, &listing);
            sync_movies(
                &state.db,
                library_id,
                &detected,
                &files,
                &library_path,
                &mut report,
            )
            .await?;
        }
        _ => {
            let detected = naming::detect_series(&library_path, &listing);
            sync_series(
                &state.db,
                library_id,
                &detected,
                &files,
                &library_path,
                &mut report,
            )
            .await?;
        }
    }

    probe_new_files(state, library_id, &mut report).await?;

    match &state.tmdb {
        Some(tmdb) => {
            let languages = metadata::languages(&state.db, &state.config.default_language).await?;
            let counts = metadata::refresh_library(&state.db, tmdb, library_id, &languages).await?;
            report.metadata_refreshed = counts.refreshed;
            report.metadata_errors = counts.errors;
        }
        None => report.metadata_skipped = Some("BATVISION_TMDB_TOKEN is not set".into()),
    }

    sqlx::query("UPDATE libraries SET last_scan_at = ? WHERE id = ?")
        .bind(now())
        .bind(library_id)
        .execute(&state.db)
        .await?;
    Ok(report)
}
