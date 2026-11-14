//! Timeline thumbnails ("trickplay", D99): sheets of 10 × 10 thumbnails, one every
//! 10 s, made from the key frames only (fast) when a player first asks for them.
//! One file at a time; stored in `cache/trickplay/<media_file_id>/<sheet>.jpg`.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tokio::{process::Command, sync::Notify};

use crate::{
    db::{Db, now},
    library::media_info,
    playback::transcode,
};

/// One thumbnail every…
pub const INTERVAL_MS: i64 = 10_000;
pub const WIDTH: i64 = 320;
pub const HEIGHT: i64 = 180;
pub const COLUMNS: i64 = 10;
pub const ROWS: i64 = 10;

/// Where a file's thumbnails stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// `count` thumbnails, `COLUMNS * ROWS` per sheet.
    Ready { count: i64 },
    /// Queued or being made.
    Pending,
    /// No video, or ffmpeg failed on it.
    Unavailable,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<i64>,
    current: Option<i64>,
}

pub struct Trickplay {
    db: Db,
    root: PathBuf,
    media_root: PathBuf,
    ffmpeg: PathBuf,
    queue: Mutex<Queue>,
    wake: Notify,
}

impl Trickplay {
    pub fn new(db: Db, root: PathBuf, media_root: PathBuf, ffmpeg: PathBuf) -> Self {
        Self {
            db,
            root,
            media_root,
            ffmpeg,
            queue: Mutex::default(),
            wake: Notify::new(),
        }
    }

    pub fn dir(&self, media_file_id: i64) -> PathBuf {
        self.root.join(media_file_id.to_string())
    }

    /// The thumbnails of a media file, queued when missing or made for an older version
    /// of the file.
    pub async fn status(&self, media_file_id: i64) -> anyhow::Result<Status> {
        let row: Option<(Option<i64>, bool)> = sqlx::query_as(
            "SELECT t.count, t.size_bytes = mf.size_bytes AND t.modified_at = mf.modified_at
             FROM trickplay t JOIN media_files mf ON mf.id = t.media_file_id
             WHERE t.media_file_id = ?",
        )
        .bind(media_file_id)
        .fetch_optional(&self.db)
        .await?;
        Ok(match row {
            Some((Some(count), true)) => Status::Ready { count },
            Some((None, true)) => Status::Unavailable,
            _ => {
                self.enqueue(media_file_id);
                Status::Pending
            }
        })
    }

    fn enqueue(&self, media_file_id: i64) {
        let mut queue = self.queue.lock().unwrap();
        if queue.current != Some(media_file_id) && !queue.pending.contains(&media_file_id) {
            queue.pending.push_back(media_file_id);
            // A stored permit: the worker sees it even if it isn't waiting yet.
            self.wake.notify_one();
        }
    }

    /// Clears what a crash left, then makes the queued thumbnails one file at a time.
    pub fn spawn(self: Arc<Self>) {
        tokio::spawn(async move {
            if let Err(err) = self.clean().await {
                tracing::warn!(error = %err, "trickplay cleanup failed");
            }
            loop {
                let next = {
                    let mut queue = self.queue.lock().unwrap();
                    queue.current = queue.pending.pop_front();
                    queue.current
                };
                match next {
                    Some(id) => {
                        if let Err(err) = self.generate(id).await {
                            tracing::warn!(media_file_id = id, error = %err, "trickplay failed");
                        }
                    }
                    None => self.wake.notified().await,
                }
            }
        });
    }

    /// Folders without a row (a deleted library, a crash mid-way).
    async fn clean(&self) -> anyhow::Result<()> {
        let known: std::collections::HashSet<i64> =
            sqlx::query_scalar("SELECT media_file_id FROM trickplay WHERE count IS NOT NULL")
                .fetch_all(&self.db)
                .await?
                .into_iter()
                .collect();
        let Ok(mut entries) = tokio::fs::read_dir(&self.root).await else {
            return Ok(());
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let id = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<i64>().ok());
            if !id.is_some_and(|id| known.contains(&id)) {
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
        Ok(())
    }

    async fn generate(&self, media_file_id: i64) -> anyhow::Result<()> {
        let row: Option<(String, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT path, probe_json, size_bytes, modified_at FROM media_files
             WHERE id = ? AND missing_since IS NULL",
        )
        .bind(media_file_id)
        .fetch_optional(&self.db)
        .await?;
        let Some((path, probe, size, modified)) = row else {
            return Ok(());
        };
        let info = media_info::parse(probe.as_deref().unwrap_or("{}"))?;
        let count = match (&info.video, info.duration_ms) {
            (Some(video), Some(duration)) if duration > 0 => {
                let partial = self.root.join(format!("{media_file_id}.partial"));
                let _ = tokio::fs::remove_dir_all(&partial).await;
                tokio::fs::create_dir_all(&partial).await?;
                let args = ffmpeg_args(
                    &self.media_root.join(&path),
                    video.index,
                    video.hdr,
                    &partial,
                );
                let output = Command::new(&self.ffmpeg)
                    .args(&args)
                    .kill_on_drop(true)
                    .output()
                    .await?;
                let sheets = sheet_count(&partial).await;
                if output.status.success() && sheets > 0 {
                    let dir = self.dir(media_file_id);
                    let _ = tokio::fs::remove_dir_all(&dir).await;
                    tokio::fs::rename(&partial, &dir).await?;
                    // The last sheet may be cut short: never point past it.
                    Some(((duration + INTERVAL_MS - 1) / INTERVAL_MS).min(sheets * COLUMNS * ROWS))
                } else {
                    let _ = tokio::fs::remove_dir_all(&partial).await;
                    tracing::warn!(
                        media_file_id,
                        stderr = %String::from_utf8_lossy(&output.stderr),
                        "trickplay: ffmpeg failed"
                    );
                    None
                }
            }
            _ => None,
        };
        sqlx::query(
            "INSERT INTO trickplay (media_file_id, size_bytes, modified_at, count, created_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (media_file_id) DO UPDATE SET size_bytes = excluded.size_bytes,
               modified_at = excluded.modified_at, count = excluded.count,
               created_at = excluded.created_at",
        )
        .bind(media_file_id)
        .bind(size)
        .bind(modified)
        .bind(count)
        .bind(now())
        .execute(&self.db)
        .await?;
        Ok(())
    }
}

async fn sheet_count(dir: &Path) -> i64 {
    let mut count = 0;
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.file_name().to_string_lossy().ends_with(".jpg") {
                count += 1;
            }
        }
    }
    count
}

/// Key frames only, one picture every [`INTERVAL_MS`] (the last key frame before it),
/// letterboxed to [`WIDTH`] × [`HEIGHT`], tiled into sheets `0.jpg`, `1.jpg`…
pub fn ffmpeg_args(input: &Path, video_index: i64, hdr: bool, out: &Path) -> Vec<String> {
    let mut filters = vec![
        // `pass`: the last picture too, and the only one of a file shorter than the interval.
        format!("fps=1000/{INTERVAL_MS}:round=down:eof_action=pass"),
        format!("scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=decrease"),
    ];
    if hdr {
        filters.push(transcode::TONEMAP.to_owned());
    }
    filters.push(format!("pad={WIDTH}:{HEIGHT}:-1:-1:color=black"));
    filters.push(format!("tile={COLUMNS}x{ROWS}"));
    // The JPEG encoder takes full-range pictures only.
    filters.push("scale=out_range=full,format=yuvj420p".into());
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]
        .map(String::from)
        .to_vec();
    args.extend(["-skip_frame".into(), "nokey".into(), "-i".into()]);
    args.push(input.to_string_lossy().into_owned());
    args.extend([
        "-map".into(),
        format!("0:{video_index}"),
        "-an".into(),
        "-sn".into(),
        "-dn".into(),
        "-vf".into(),
        filters.join(","),
        "-q:v".into(),
        "5".into(),
        "-start_number".into(),
        "0".into(),
    ]);
    args.push(out.join("%d.jpg").to_string_lossy().into_owned());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_frames_scaled_letterboxed_and_tiled() {
        let args =
            ffmpeg_args(Path::new("/m/a.mkv"), 0, false, Path::new("/c/1.partial")).join(" ");
        assert!(args.contains("-skip_frame nokey -i /m/a.mkv -map 0:0 -an -sn -dn"));
        assert!(args.contains(
            "-vf fps=1000/10000:round=down:eof_action=pass,\
             scale=320:180:force_original_aspect_ratio=decrease,\
             pad=320:180:-1:-1:color=black,tile=10x10,scale=out_range=full,format=yuvj420p"
        ));
        assert!(args.ends_with("-start_number 0 /c/1.partial/%d.jpg"));
    }

    #[test]
    fn hdr_is_tone_mapped_after_scaling() {
        let args = ffmpeg_args(Path::new("/m/a.mkv"), 1, true, Path::new("/c/1.partial")).join(" ");
        assert!(args.contains(&format!(
            "force_original_aspect_ratio=decrease,{},pad=",
            transcode::TONEMAP
        )));
    }
}
