//! Timeline thumbnails ("trickplay", D99): sheets of 10 × 10 thumbnails, one every
//! 10 s, made from the key frames only (fast), decoded on the GPU when there is one. Made when a player first asks for them,
//! or in batches an administrator starts; what a player waits for goes first. One file
//! at a time; stored in `cache/trickplay/<media_file_id>/<sheet>.jpg`.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tokio::{process::Command, sync::Notify};

use crate::{
    db::{Db, now},
    downloads::encode::Encoders,
    ffmpeg::FfmpegInfo,
    library::media_info,
    playback::{gpu, transcode},
};

/// SQL condition: `mf` (a `media_files` row) is the file players use for its movie or
/// episode: present, analyzed, and the largest one of the item. A macro, so the queries
/// using it stay static strings.
macro_rules! main_file {
    () => {
        "mf.missing_since IS NULL AND mf.probe_json IS NOT NULL
         AND mf.id = (SELECT m2.id FROM media_files m2 WHERE m2.item_kind = mf.item_kind
           AND m2.item_id = mf.item_id AND m2.missing_since IS NULL
           ORDER BY m2.size_bytes DESC LIMIT 1)"
    };
}

/// How the library's thumbnails stand: videos, with thumbnails, without possible ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub videos: i64,
    pub ready: i64,
    pub unavailable: i64,
}

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
    /// Asked for by a player (and the next episode): before any batch.
    urgent: VecDeque<i64>,
    /// Started by an administrator.
    batch: VecDeque<i64>,
    current: Option<i64>,
}

impl Queue {
    fn holds(&self, id: i64) -> bool {
        self.current == Some(id) || self.urgent.contains(&id) || self.batch.contains(&id)
    }
}

/// What the queue holds, for the administration page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueState {
    /// Files waiting, urgent and batch together.
    pub waiting: usize,
    /// The file being made.
    pub current: Option<i64>,
}

pub struct Trickplay {
    db: Db,
    root: PathBuf,
    media_root: PathBuf,
    ffmpeg: PathBuf,
    /// What ffmpeg can do here, once probed: the GPU decodes and scales when it can.
    ffmpeg_info: Arc<tokio::sync::RwLock<Option<FfmpegInfo>>>,
    queue: Mutex<Queue>,
    wake: Notify,
}

impl Trickplay {
    pub fn new(
        db: Db,
        root: PathBuf,
        media_root: PathBuf,
        ffmpeg: PathBuf,
        ffmpeg_info: Arc<tokio::sync::RwLock<Option<FfmpegInfo>>>,
    ) -> Self {
        Self {
            db,
            root,
            media_root,
            ffmpeg,
            ffmpeg_info,
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
                self.enqueue_urgent(media_file_id);
                Status::Pending
            }
        })
    }

    /// A player waits for it: ahead of every batch (moved there if a batch holds it).
    fn enqueue_urgent(&self, media_file_id: i64) {
        let mut queue = self.queue.lock().unwrap();
        queue.batch.retain(|id| *id != media_file_id);
        if !queue.holds(media_file_id) {
            queue.urgent.push_back(media_file_id);
            // A stored permit: the worker sees it even if it isn't waiting yet.
            self.wake.notify_one();
        }
    }

    /// Queues the main file of up to `limit` movies and episodes without thumbnails (or
    /// with thumbnails of an older version of the file), newest first. All of them
    /// without a limit. Returns how many were added.
    pub async fn queue_missing(&self, limit: Option<i64>) -> anyhow::Result<usize> {
        let missing: Vec<i64> = sqlx::query_scalar(concat!(
            "SELECT mf.id FROM media_files mf LEFT JOIN trickplay t ON t.media_file_id = mf.id
             WHERE ",
            main_file!(),
            " AND (t.media_file_id IS NULL
               OR t.size_bytes != mf.size_bytes OR t.modified_at != mf.modified_at)
             ORDER BY mf.id DESC"
        ))
        .fetch_all(&self.db)
        .await?;
        let mut queue = self.queue.lock().unwrap();
        let wanted = limit.map_or(usize::MAX, |l| l.max(0) as usize);
        let mut added = 0;
        for id in missing {
            if added == wanted {
                break;
            }
            if !queue.holds(id) {
                queue.batch.push_back(id);
                added += 1;
            }
        }
        if added > 0 {
            self.wake.notify_one();
        }
        Ok(added)
    }

    pub async fn counts(&self) -> anyhow::Result<Counts> {
        let (videos, ready, unavailable) = sqlx::query_as(concat!(
            "SELECT COUNT(*),
               COALESCE(SUM(t.count IS NOT NULL AND t.size_bytes = mf.size_bytes
                 AND t.modified_at = mf.modified_at), 0),
               COALESCE(SUM(t.media_file_id IS NOT NULL AND t.count IS NULL
                 AND t.size_bytes = mf.size_bytes AND t.modified_at = mf.modified_at), 0)
             FROM media_files mf LEFT JOIN trickplay t ON t.media_file_id = mf.id WHERE ",
            main_file!()
        ))
        .fetch_one(&self.db)
        .await?;
        Ok(Counts {
            videos,
            ready,
            unavailable,
        })
    }

    /// Drops the batches still waiting; what players wait for and the file being made stay.
    pub fn clear_batch(&self) {
        self.queue.lock().unwrap().batch.clear();
    }

    pub fn queue_state(&self) -> QueueState {
        let queue = self.queue.lock().unwrap();
        QueueState {
            waiting: queue.urgent.len() + queue.batch.len(),
            current: queue.current,
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
                    queue.current = queue.urgent.pop_front().or_else(|| queue.batch.pop_front());
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
                let input = self.media_root.join(&path);
                match self
                    .make_sheets(media_file_id, &input, video, duration)
                    .await
                {
                    Ok(count) => Some(count),
                    Err(err) => {
                        tracing::warn!(media_file_id, error = %err, "trickplay: ffmpeg failed");
                        None
                    }
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

    /// Two passes. The key frames first, each a small picture named after its time: with
    /// key frames only, decoders hand them out of order, so `fps` would drop or shift
    /// them. Then each slot gets the last key frame at or before its time, and the
    /// pictures are tiled into sheets. Decoded on the GPU when it can, else on the CPU.
    async fn make_sheets(
        &self,
        media_file_id: i64,
        input: &Path,
        video: &media_info::VideoStream,
        duration_ms: i64,
    ) -> anyhow::Result<i64> {
        let partial = self.root.join(format!("{media_file_id}.partial"));
        let frames = partial.join("frames");
        let _ = tokio::fs::remove_dir_all(&partial).await;
        tokio::fs::create_dir_all(&frames).await?;
        let result = async {
            let gpu = self
                .gpu_filters()
                .await
                .then(|| gpu::Source::of(video))
                .filter(|s| s.decode);
            let size = fit(video.width, video.height);
            let mut keys = Vec::new();
            if let Some(source) = gpu {
                let args =
                    key_frame_args(input, video.index, video.hdr, Some(source), size, &frames);
                match self.run(&args).await {
                    Ok(log) => keys = frame_times(&log),
                    Err(err) => tracing::info!(
                        media_file_id,
                        error = %err,
                        "trickplay: GPU decoding failed, using the CPU"
                    ),
                }
            }
            if keys.is_empty() {
                let _ = tokio::fs::remove_dir_all(&frames).await;
                tokio::fs::create_dir_all(&frames).await?;
                let args = key_frame_args(input, video.index, video.hdr, None, size, &frames);
                keys = frame_times(&self.run(&args).await?);
            }
            anyhow::ensure!(!keys.is_empty(), "no key frame decoded");
            let count = (duration_ms + INTERVAL_MS - 1) / INTERVAL_MS;
            let list = partial.join("frames.txt");
            let lines: String = pick_frames(&keys, count)
                .iter()
                .map(|n| format!("file '{}'\n", frames.join(format!("{n}.jpg")).display()))
                .collect();
            tokio::fs::write(&list, lines).await?;
            self.run(&tile_args(&list, &partial)).await?;
            let _ = tokio::fs::remove_dir_all(&frames).await;
            let _ = tokio::fs::remove_file(&list).await;
            let sheets = sheet_count(&partial).await;
            anyhow::ensure!(sheets > 0, "no sheet made");
            let dir = self.dir(media_file_id);
            let _ = tokio::fs::remove_dir_all(&dir).await;
            tokio::fs::rename(&partial, &dir).await?;
            // The last sheet may be cut short: never point past it.
            Ok(count.min(sheets * COLUMNS * ROWS))
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_dir_all(&partial).await;
        }
        result
    }

    async fn gpu_filters(&self) -> bool {
        self.ffmpeg_info
            .read()
            .await
            .as_ref()
            .is_some_and(|info| Encoders::from_info(info).gpu_filters)
    }

    /// Runs ffmpeg; its log on success (the first pass reads frame times from it).
    async fn run(&self, args: &[String]) -> anyhow::Result<String> {
        let output = Command::new(&self.ffmpeg)
            .args(args)
            .kill_on_drop(true)
            .output()
            .await?;
        let log = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            // The log runs at info level for the frame times: keep its end, where errors are.
            let tail: Vec<&str> = log.lines().rev().take(5).collect();
            anyhow::bail!("{}", tail.into_iter().rev().collect::<Vec<_>>().join(" / "));
        }
        Ok(log)
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

/// The first pass's pictures, from the `showinfo` lines of its log: (time in ms, picture
/// number), in time order. Pictures are numbered in the order the decoder hands them out,
/// which isn't time order when it decodes key frames only.
fn frame_times(log: &str) -> Vec<(i64, i64)> {
    let mut frames: Vec<(i64, i64)> = log
        .lines()
        .filter(|l| l.contains("Parsed_showinfo"))
        .filter_map(|l| {
            let n = l
                .split(" n:")
                .nth(1)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()?;
            let t: f64 = l
                .split("pts_time:")
                .nth(1)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()?;
            Some(((t * 1000.0).round() as i64, n))
        })
        .collect();
    frames.sort_unstable();
    frames
}

/// For each of `count` slots, the picture of the key frame nearest its time (the earlier
/// one on a tie). Key frames can be 20 s apart: the last one before a slot may be another
/// scene. `frames` is (time in ms, picture number), in time order.
pub fn pick_frames(frames: &[(i64, i64)], count: i64) -> Vec<i64> {
    (0..count)
        .map(|n| {
            let at = n * INTERVAL_MS;
            let i = frames.partition_point(|&(t, _)| t <= at);
            let before = frames.get(i.wrapping_sub(1));
            match (before, frames.get(i)) {
                (Some(b), Some(a)) if a.0 - at < at - b.0 => a.1,
                (Some(b), _) => b.1,
                (None, Some(a)) => a.1,
                (None, None) => 0,
            }
        })
        .collect()
}

/// The picture size inside [`WIDTH`] × [`HEIGHT`] that keeps the aspect ratio (even sizes).
fn fit(width: Option<i64>, height: Option<i64>) -> (i64, i64) {
    match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => {
            let (fw, fh) = if w * HEIGHT > h * WIDTH {
                (WIDTH, h * WIDTH / w)
            } else {
                (w * HEIGHT / h, HEIGHT)
            };
            ((fw / 2 * 2).max(2), (fh / 2 * 2).max(2))
        }
        _ => (WIDTH, HEIGHT),
    }
}

/// First pass: every key frame as a letterboxed picture, numbered from 0; the log gives
/// each one's time.
pub fn key_frame_args(
    input: &Path,
    video_index: i64,
    hdr: bool,
    gpu: Option<gpu::Source>,
    (w, h): (i64, i64),
    out: &Path,
) -> Vec<String> {
    // Info level: `showinfo` logs there.
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "info",
        "-nostats",
        "-nostdin",
        "-y",
    ]
    .map(String::from)
    .to_vec();
    let mut filters = Vec::new();
    match gpu {
        Some(source) => {
            args.extend(gpu::input_args(source));
            // Scaled (and tone-mapped) on the GPU, then back to memory for the rest.
            let format = if hdr { "p010le" } else { "yuv420p" };
            filters.push(format!("scale_cuda={w}:{h}:format={format}"));
            if hdr {
                filters.push(gpu::TONEMAP.to_owned());
            }
            filters.push("hwdownload,format=yuv420p".into());
        }
        None => {
            filters.push(format!("scale={w}:{h}"));
            if hdr {
                filters.push(transcode::TONEMAP.to_owned());
            }
        }
    }
    filters.push(format!("pad={WIDTH}:{HEIGHT}:-1:-1:color=black"));
    // The JPEG encoder takes full-range pictures only.
    filters.push("scale=out_range=full,format=yuvj420p".into());
    // Each picture's time, logged; then renumbered in order, as the decoder hands key
    // frames out of order and the encoder refuses times going back.
    filters.push("showinfo,setpts=N/TB".into());
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
        // One picture per decoded frame.
        "-fps_mode".into(),
        "passthrough".into(),
        "-q:v".into(),
        "5".into(),
        "-start_number".into(),
        "0".into(),
    ]);
    args.push(out.join("%d.jpg").to_string_lossy().into_owned());
    args
}

/// Second pass: the picked pictures, in slot order, tiled into sheets `0.jpg`, `1.jpg`…
pub fn tile_args(list: &Path, out: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
    ]
    .map(String::from)
    .to_vec();
    args.push(list.to_string_lossy().into_owned());
    args.extend([
        "-vf".into(),
        format!("tile={COLUMNS}x{ROWS}"),
        "-fps_mode".into(),
        "passthrough".into(),
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
    fn frame_times_come_from_the_log_in_time_order() {
        // Key frames come out of the decoder out of order (seen on a 4K HEVC file).
        let log = "\
[Parsed_showinfo_4 @ 0x1] config in time_base: 1/1000, frame_rate: 24/1
[Parsed_showinfo_4 @ 0x1] n:   0 pts:      0 pts_time:0       duration:     42
[Parsed_showinfo_4 @ 0x1]   color_range:pc color_space:bt709
[Parsed_showinfo_4 @ 0x1] n:   1 pts:  11970 pts_time:11.97   duration:     42
[Parsed_showinfo_4 @ 0x1] n:   2 pts:   2711 pts_time:2.711   duration:     42";
        assert_eq!(frame_times(log), vec![(0, 0), (2_711, 2), (11_970, 1)]);
    }

    #[test]
    fn each_slot_gets_the_nearest_key_frame() {
        let frames = [
            (0, 0),
            (2_711, 2),
            (11_970, 1),
            (17_643, 4),
            (27_653, 3),
            (80_539, 6),
            (100_559, 5),
        ];
        assert_eq!(
            pick_frames(&frames, 4),
            vec![0, 1, 4, 3],
            "0 s, 10 s, 20 s, 30 s"
        );
        assert_eq!(
            pick_frames(&frames, 11)[10],
            5,
            "100 s: 100.559 s, not 80.539 s"
        );
        assert_eq!(
            pick_frames(&[(1_500, 7)], 2),
            vec![7, 7],
            "one key frame: it"
        );
        assert_eq!(pick_frames(&[(0, 1), (10_000, 2), (20_000, 3)], 1), vec![1]);
        assert_eq!(
            pick_frames(&[(5_000, 1), (15_000, 2)], 2),
            vec![1, 1],
            "a tie: the earlier"
        );
    }

    #[test]
    fn pictures_fit_the_thumbnail_box() {
        assert_eq!(fit(Some(3840), Some(2160)), (320, 180));
        assert_eq!(fit(Some(3840), Some(1608)), (320, 134));
        assert_eq!(fit(Some(1440), Some(1080)), (240, 180));
        assert_eq!(fit(None, None), (320, 180));
    }

    #[test]
    fn key_frames_on_the_cpu() {
        let args = key_frame_args(
            Path::new("/m/a.mkv"),
            0,
            true,
            None,
            (320, 134),
            Path::new("/c/f"),
        )
        .join(" ");
        assert!(args.contains("-skip_frame nokey -i /m/a.mkv -map 0:0 -an -sn -dn"));
        assert!(args.contains(&format!(
            "-vf scale=320:134,{},pad=320:180:-1:-1:color=black,scale=out_range=full,format=yuvj420p,showinfo,setpts=N/TB",
            transcode::TONEMAP
        )));
        assert!(args.ends_with("-fps_mode passthrough -q:v 5 -start_number 0 /c/f/%d.jpg"));
        assert!(!args.contains("cuda"));
    }

    #[test]
    fn key_frames_on_the_gpu() {
        let source = gpu::Source {
            decode: true,
            deep: true,
        };
        let args = key_frame_args(
            Path::new("/m/a.mkv"),
            0,
            true,
            Some(source),
            (320, 180),
            Path::new("/c/f"),
        )
        .join(" ");
        assert!(args.contains("-hwaccel cuda"));
        assert!(args.contains(&format!(
            "-vf scale_cuda=320:180:format=p010le,{},hwdownload,format=yuv420p,pad=",
            gpu::TONEMAP
        )));
    }

    #[test]
    fn sheets_tile_the_picked_pictures() {
        let args = tile_args(
            Path::new("/c/1.partial/frames.txt"),
            Path::new("/c/1.partial"),
        )
        .join(" ");
        assert!(args.contains("-f concat -safe 0 -i /c/1.partial/frames.txt -vf tile=10x10"));
        assert!(args.ends_with("-start_number 0 /c/1.partial/%d.jpg"));
    }
}
