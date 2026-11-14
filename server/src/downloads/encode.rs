//! Encoded variants for download: an MP4 per media file × profile × codec with every
//! audio track, plus its text subtitles as WebVTT. Encoded only when a device asks.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::{Notify, Semaphore},
};
use utoipa::ToSchema;

use super::plan::Profile;
use crate::{
    db::{Db, now},
    ffmpeg::FfmpegInfo,
    library::media_info::{self, TrackKind},
    playback::{gpu, normalize_language, subtitles, transcode},
};

/// Video codecs a profile can produce; `Copy` keeps the source stream (Max).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoPlan {
    Copy,
    Hevc,
    Av1,
}

/// Which encoders this server has (from the startup probe).
#[derive(Debug, Clone, Default)]
pub struct Encoders {
    pub hevc_nvenc: bool,
    pub av1_nvenc: bool,
    pub libsvtav1: bool,
    /// Decode, scale and tone-map on the GPU (see `playback::gpu`).
    pub gpu_filters: bool,
}

impl Encoders {
    pub fn from_info(info: &FfmpegInfo) -> Self {
        let has = |n: &str| info.working_encoders.iter().any(|e| e == n);
        Self {
            hevc_nvenc: has("hevc_nvenc"),
            av1_nvenc: has("av1_nvenc"),
            libsvtav1: has("libsvtav1"),
            gpu_filters: info.gpu_filters,
        }
    }
}

/// Video plan for a profile. Max copies anything a player handles; lighter profiles
/// use AV1 when the device decodes it in hardware and the server can encode it.
pub fn video_plan(
    profile: Profile,
    source_codec: &str,
    device_av1: bool,
    encoders: &Encoders,
) -> VideoPlan {
    match profile {
        Profile::Max if matches!(source_codec, "h264" | "hevc" | "av1") => VideoPlan::Copy,
        Profile::Max => VideoPlan::Hevc,
        _ if device_av1 && (encoders.av1_nvenc || encoders.libsvtav1) => VideoPlan::Av1,
        _ => VideoPlan::Hevc,
    }
}

/// Codec label stored with the variant (part of its identity).
pub fn codec_label(plan: VideoPlan, source_codec: &str) -> String {
    match plan {
        VideoPlan::Copy => source_codec.to_owned(),
        VideoPlan::Hevc => "hevc".into(),
        VideoPlan::Av1 => "av1".into(),
    }
}

/// An audio track of the source, re-encoded into the download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOut {
    pub index: i64,
    pub channels: Option<i64>,
    /// The source's track name ("Commentary"…), shown by players as the track's label.
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EncodeJob {
    pub input: PathBuf,
    pub profile: Profile,
    pub video: VideoPlan,
    pub video_index: i64,
    pub source_height: Option<i64>,
    /// The source video is HEVC (it matters when it's copied as is).
    pub source_hevc: bool,
    pub hdr: bool,
    pub source: gpu::Source,
    /// Every audio track goes in, in the source's order (D40).
    pub audio: Vec<AudioOut>,
    pub encoders: Encoders,
}

impl EncodeJob {
    fn encoder(&self) -> &'static str {
        match self.video {
            VideoPlan::Av1 if self.encoders.av1_nvenc => "av1_nvenc",
            VideoPlan::Av1 => "libsvtav1",
            _ if self.encoders.hevc_nvenc => "hevc_nvenc",
            _ => "libx265",
        }
    }

    /// Frames go from the decoder to NVENC without leaving the GPU.
    pub fn on_gpu(&self) -> bool {
        self.video != VideoPlan::Copy
            && self.encoders.gpu_filters
            && self.encoder().ends_with("_nvenc")
    }
}

fn target(profile: Profile) -> Option<(i64, u32)> {
    match profile {
        Profile::Max => None,
        Profile::Mobile => Some((720, 1500)),
        Profile::Saver => Some((480, 700)),
    }
}

/// ffmpeg arguments writing `out` (MP4). Pure, for testing.
pub fn ffmpeg_args(job: &EncodeJob, out: &Path) -> Vec<String> {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]
        .map(String::from)
        .to_vec();
    if job.on_gpu() {
        args.extend(gpu::input_args(job.source));
    }
    args.push("-i".into());
    args.push(job.input.to_string_lossy().into_owned());
    args.extend(["-map".into(), format!("0:{}", job.video_index)]);

    match job.video {
        VideoPlan::Copy => {
            args.extend(["-c:v".into(), "copy".into()]);
        }
        _ => {
            let (height, kbps) =
                target(job.profile).unwrap_or((job.source_height.unwrap_or(1080), 8000));
            let scale = job.source_height.is_some_and(|h| h > height);
            let filters = if job.on_gpu() {
                let size = format!("w=-2:h={height}");
                gpu::chain(job.source, scale.then_some(size.as_str()), job.hdr)
            } else {
                // Scaling first: tone mapping the smaller picture is much cheaper.
                let mut filters = Vec::new();
                if scale {
                    filters.push(format!("scale=-2:{height}"));
                }
                if job.hdr {
                    filters.push(transcode::TONEMAP.to_owned());
                }
                filters.push("format=yuv420p".into());
                filters.join(",")
            };
            args.extend(["-vf".into(), filters]);
            let encoder = job.encoder();
            args.extend(["-c:v".into(), encoder.into()]);
            match encoder {
                "av1_nvenc" | "hevc_nvenc" => {
                    args.extend(["-preset", "p5", "-rc", "vbr"].map(String::from))
                }
                "libsvtav1" => args.extend(["-preset", "8"].map(String::from)),
                _ => args.extend(["-preset", "medium"].map(String::from)),
            }
            args.extend(["-b:v".into(), format!("{kbps}k")]);
            // SVT-AV1 caps the bitrate only in CRF mode: its VBR rejects -maxrate.
            if encoder != "libsvtav1" {
                args.extend([
                    "-maxrate".into(),
                    format!("{}k", kbps * 3 / 2),
                    "-bufsize".into(),
                    format!("{}k", kbps * 3),
                ]);
            }
        }
    }
    // Apple players need the hvc1 tag for HEVC in MP4, copied from the source or not.
    let hevc_out =
        job.video == VideoPlan::Hevc || (job.video == VideoPlan::Copy && job.source_hevc);
    if hevc_out {
        args.extend(["-tag:v".into(), "hvc1".into()]);
    }

    for audio in &job.audio {
        args.extend(["-map".into(), format!("0:{}", audio.index)]);
    }
    if !job.audio.is_empty() {
        args.extend(["-c:a".into(), "aac".into()]);
    }
    for (n, audio) in job.audio.iter().enumerate() {
        let surround = audio.channels.unwrap_or(2) > 2;
        let (channels, bitrate) = match job.profile {
            Profile::Max if surround => ("6", "384k"),
            Profile::Max => ("2", "192k"),
            Profile::Mobile => ("2", "128k"),
            Profile::Saver => ("2", "96k"),
        };
        args.extend([
            format!("-ac:a:{n}"),
            channels.into(),
            format!("-b:a:{n}"),
            bitrate.into(),
        ]);
        // MP4 keeps a track's name as its handler name.
        if let Some(title) = audio.title.as_deref().filter(|t| !t.trim().is_empty()) {
            args.extend([
                format!("-metadata:s:a:{n}"),
                format!("handler_name={title}"),
            ]);
        }
    }

    args.extend(
        [
            "-sn",
            "-dn",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
        ]
        .map(String::from),
    );
    args.push(out.to_string_lossy().into_owned());
    args
}

/// A text subtitle shipped next to the video.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct VariantSubtitle {
    pub file: String,
    pub language: Option<String>,
    pub kind: TrackKind,
}

/// Sizes and runs the encode queue. One worker per allowed concurrent encode.
/// An encode stopped because nobody waits for it any more.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("encode cancelled")
    }
}

impl std::error::Error for Cancelled {}

pub struct Encoder {
    db: Db,
    root: PathBuf,
    media_root: PathBuf,
    ffmpeg: PathBuf,
    wake: Notify,
    slots: Arc<Semaphore>,
    budget_bytes: i64,
    /// Encodes in progress, each with what stops its ffmpeg.
    running: Mutex<HashMap<i64, Arc<Notify>>>,
}

impl Encoder {
    pub fn new(
        db: Db,
        root: PathBuf,
        media_root: PathBuf,
        ffmpeg: PathBuf,
        concurrency: usize,
        budget_bytes: i64,
    ) -> Self {
        Self {
            db,
            root,
            media_root,
            ffmpeg,
            wake: Notify::new(),
            slots: Arc::new(Semaphore::new(concurrency.max(1))),
            budget_bytes,
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Whether ffmpeg is encoding `id` right now.
    pub fn is_encoding(&self, id: i64) -> bool {
        self.running.lock().unwrap().contains_key(&id)
    }

    /// Stops the encode of `id` if one runs; its worker then moves on.
    pub fn abort(&self, id: i64) {
        if let Some(stop) = self.running.lock().unwrap().get(&id) {
            // A stored permit: the encode sees it even between two reads.
            stop.notify_one();
        }
    }

    pub fn dir(&self, variant_id: i64) -> PathBuf {
        self.root.join(variant_id.to_string())
    }

    pub fn budget_bytes(&self) -> i64 {
        self.budget_bytes
    }

    /// Wakes the workers after a new request.
    pub fn notify(&self) {
        self.wake.notify_waiters();
    }

    /// Requeues encodes interrupted by a restart and starts the workers.
    pub async fn start(
        self: Arc<Self>,
        encoders: impl Fn() -> Encoders + Send + Sync + 'static,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE variants SET status = 'queued', progress = 0 WHERE status = 'encoding'",
        )
        .execute(&self.db)
        .await?;
        self.reconcile().await?;
        let encoders = Arc::new(encoders);
        tokio::spawn(async move {
            loop {
                let permit = self
                    .slots
                    .clone()
                    .acquire_owned()
                    .await
                    .expect("semaphore open");
                match self.claim_next().await {
                    Ok(Some(id)) => {
                        let this = self.clone();
                        let encoders = encoders();
                        tokio::spawn(async move {
                            let _permit = permit;
                            if let Err(err) = this.encode(id, encoders).await {
                                if err.is::<Cancelled>() {
                                    tracing::info!(
                                        variant = id,
                                        "encode cancelled: nobody waits for it"
                                    );
                                    this.wake.notify_waiters();
                                    return;
                                }
                                tracing::error!(variant = id, error = %format!("{err:#}"), "encode failed");
                                // The partial file is of no use and isn't counted in the budget.
                                let _ = tokio::fs::remove_dir_all(this.dir(id)).await;
                                let _ = sqlx::query(
                                    "UPDATE variants SET status = 'failed', error = ? WHERE id = ?",
                                )
                                .bind(format!("{err:#}").chars().take(500).collect::<String>())
                                .bind(id)
                                .execute(&this.db)
                                .await;
                            }
                            this.wake.notify_waiters();
                        });
                    }
                    Ok(None) => {
                        drop(permit);
                        // Woken by a request, or checks again regularly.
                        let _ = tokio::time::timeout(Duration::from_secs(30), self.wake.notified())
                            .await;
                    }
                    Err(err) => {
                        drop(permit);
                        tracing::error!(error = %err, "encode queue");
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    }
                }
            }
        });
        Ok(())
    }

    /// Brings the table and the cache folder back in line: the cache is disposable, and
    /// a wiped one must not leave "ready" versions that answer 404 for ever.
    async fn reconcile(&self) -> anyhow::Result<()> {
        let ready: Vec<i64> = sqlx::query_scalar("SELECT id FROM variants WHERE status = 'ready'")
            .fetch_all(&self.db)
            .await?;
        for id in ready {
            if tokio::fs::try_exists(self.dir(id).join("video.mp4"))
                .await
                .unwrap_or(true)
            {
                continue;
            }
            let wanted: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM variant_requests
                                WHERE variant_id = ? AND confirmed_at IS NULL)",
            )
            .bind(id)
            .fetch_one(&self.db)
            .await?;
            if wanted {
                tracing::warn!(variant = id, "encoded file is gone: encoding it again");
                sqlx::query("UPDATE variants SET status = 'queued', progress = 0 WHERE id = ?")
                    .bind(id)
                    .execute(&self.db)
                    .await?;
            } else {
                self.delete(id).await?;
            }
        }
        // Folders without a row (a deleted library, an encode stopped by a crash).
        let known: std::collections::HashSet<i64> = sqlx::query_scalar("SELECT id FROM variants")
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
            if id.is_some_and(|id| !known.contains(&id)) {
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
        Ok(())
    }

    /// Stops the encodes and removes the files of variants whose rows are already gone
    /// (deleted with their library).
    pub async fn forget(&self, ids: &[i64]) {
        for id in ids {
            self.abort(*id);
            let _ = tokio::fs::remove_dir_all(self.dir(*id)).await;
        }
    }

    async fn claim_next(&self) -> anyhow::Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "UPDATE variants SET status = 'encoding', progress = 0
             WHERE id = (SELECT id FROM variants WHERE status = 'queued' ORDER BY created_at, id LIMIT 1)
             RETURNING id",
        )
        .fetch_optional(&self.db)
        .await?)
    }

    async fn encode(&self, id: i64, encoders: Encoders) -> anyhow::Result<()> {
        #[derive(sqlx::FromRow)]
        struct Row {
            profile: Profile,
            codec: String,
            audio_index: Option<i64>,
            path: String,
            probe_json: Option<String>,
            media_file_id: i64,
        }
        let row: Row = sqlx::query_as(
            "SELECT v.profile, v.codec, v.audio_index, mf.path, mf.probe_json, mf.id AS media_file_id
             FROM variants v JOIN media_files mf ON mf.id = v.media_file_id WHERE v.id = ?",
        )
        .bind(id)
        .fetch_one(&self.db)
        .await?;
        let info = media_info::parse(row.probe_json.as_deref().unwrap_or("{}"))?;
        let video = info
            .video
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no video stream"))?;
        let plan = if row.codec == video.codec && row.profile == Profile::Max {
            VideoPlan::Copy
        } else if row.codec == "av1" {
            VideoPlan::Av1
        } else {
            VideoPlan::Hevc
        };
        let input = self.media_root.join(&row.path);
        // Variants from before D40 was revised hold one track.
        let audio = info
            .audio
            .iter()
            .filter(|a| row.audio_index.is_none_or(|i| a.index == i))
            .map(|a| AudioOut {
                index: a.index,
                channels: a.channels,
                title: a.title.clone(),
            })
            .collect();
        let job = EncodeJob {
            input: input.clone(),
            profile: row.profile,
            video: plan,
            video_index: video.index,
            source_height: video.height,
            source_hevc: video.codec == "hevc",
            hdr: video.hdr,
            source: gpu::Source::of(video),
            audio,
            encoders,
        };

        let dir = self.dir(id);
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await?;
        let partial = dir.join("video.partial.mp4");
        let out = dir.join("video.mp4");
        let duration_us = info.duration_ms.unwrap_or(0) * 1000;
        tracing::info!(
            variant = id,
            profile = ?row.profile,
            codec = %row.codec,
            gpu = job.on_gpu(),
            "encoding"
        );

        let stop = Arc::new(Notify::new());
        self.running.lock().unwrap().insert(id, stop.clone());
        let result = self.run(id, &job, &partial, duration_us, &stop).await;
        let result = match result {
            Err(err) if job.on_gpu() && !err.is::<Cancelled>() => {
                // A source the GPU chain can't take: the CPU filters handle anything.
                tracing::warn!(variant = id, error = %format!("{err:#}"), "GPU encode failed, retrying on the CPU");
                let mut cpu = job.clone();
                cpu.encoders.gpu_filters = false;
                self.run(id, &cpu, &partial, duration_us, &stop).await
            }
            other => other,
        };
        self.running.lock().unwrap().remove(&id);
        result?;
        tokio::fs::rename(&partial, &out).await?;

        // Every text subtitle, embedded or external, as WebVTT next to the video.
        let mut subs = Vec::new();
        for track in info.subtitles.iter().filter(|s| s.text) {
            if let Ok(vtt) = subtitles::to_webvtt(&self.ffmpeg, &input, Some(track.index)).await {
                let file = format!("sub{}.vtt", subs.len());
                tokio::fs::write(dir.join(&file), vtt).await?;
                subs.push(VariantSubtitle {
                    file,
                    language: track.language.as_deref().and_then(normalize_language),
                    kind: track.kind,
                });
            }
        }
        let external: Vec<(String, Option<String>, bool, bool)> = sqlx::query_as(
            "SELECT path, language, forced, sdh FROM subtitle_files WHERE media_file_id = ? ORDER BY path",
        )
        .bind(row.media_file_id)
        .fetch_all(&self.db)
        .await?;
        for (path, language, forced, sdh) in external {
            if let Ok(vtt) =
                subtitles::to_webvtt(&self.ffmpeg, &self.media_root.join(path), None).await
            {
                let file = format!("sub{}.vtt", subs.len());
                tokio::fs::write(dir.join(&file), vtt).await?;
                let kind = if forced {
                    TrackKind::Forced
                } else if sdh {
                    TrackKind::Sdh
                } else {
                    TrackKind::Normal
                };
                subs.push(VariantSubtitle {
                    file,
                    language,
                    kind,
                });
            }
        }

        let size = tokio::fs::metadata(&out).await?.len() as i64;
        sqlx::query(
            "UPDATE variants SET status = 'ready', progress = 1, size_bytes = ?, subtitles_json = ?,
             error = NULL, finished_at = ? WHERE id = ?",
        )
        .bind(size)
        .bind(serde_json::to_string(&subs)?)
        .bind(now())
        .bind(id)
        .execute(&self.db)
        .await?;
        tracing::info!(variant = id, size, "encoded");
        self.enforce_budget().await?;
        Ok(())
    }

    /// Runs ffmpeg for `job`, reporting progress on the variant.
    async fn run(
        &self,
        id: i64,
        job: &EncodeJob,
        partial: &Path,
        duration_us: i64,
        stop: &Notify,
    ) -> anyhow::Result<()> {
        let mut child = Command::new(&self.ffmpeg)
            .args(ffmpeg_args(job, partial))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child.stdout.take().expect("piped");
        // Read while ffmpeg runs: a damaged source fills the pipe with errors, and ffmpeg
        // would then wait for a reader for ever. Only the end is kept for the report.
        let mut stderr = child.stderr.take().expect("piped");
        let errors = tokio::spawn(async move {
            const KEPT: usize = 8 * 1024;
            let mut tail = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(read) = stderr.read(&mut chunk).await {
                if read == 0 {
                    break;
                }
                tail.extend_from_slice(&chunk[..read]);
                if tail.len() > 2 * KEPT {
                    tail.drain(..tail.len() - KEPT);
                }
            }
            tail
        });
        let mut lines = BufReader::new(stdout).lines();
        let mut last_update = Instant::now();
        loop {
            let line = tokio::select! {
                line = lines.next_line() => line?,
                _ = stop.notified() => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    return Err(Cancelled.into());
                }
            };
            let Some(line) = line else { break };
            if let Some(us) = line
                .strip_prefix("out_time_us=")
                .and_then(|v| v.parse::<i64>().ok())
                && duration_us > 0
                && last_update.elapsed() > Duration::from_secs(2)
            {
                last_update = Instant::now();
                let progress = (us as f64 / duration_us as f64).clamp(0.0, 0.99);
                sqlx::query("UPDATE variants SET progress = ? WHERE id = ?")
                    .bind(progress)
                    .bind(id)
                    .execute(&self.db)
                    .await?;
            }
        }
        let status = child.wait().await?;
        let errors = errors.await.unwrap_or_default();
        anyhow::ensure!(
            status.success(),
            "ffmpeg: {}",
            String::from_utf8_lossy(&errors).trim()
        );
        Ok(())
    }

    /// Deletes a variant's files and row.
    pub async fn delete(&self, id: i64) -> anyhow::Result<()> {
        // Its encode, if one runs, would otherwise hold a slot to the end for nothing.
        self.abort(id);
        let _ = tokio::fs::remove_dir_all(self.dir(id)).await;
        sqlx::query("DELETE FROM variants WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Over budget: drop ready variants no device is still waiting for, oldest first.
    /// Variants a device still needs are never dropped, even over budget.
    pub async fn enforce_budget(&self) -> anyhow::Result<()> {
        let total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM variants WHERE status = 'ready'",
        )
        .fetch_one(&self.db)
        .await?;
        if total <= self.budget_bytes {
            return Ok(());
        }
        let candidates: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT v.id, v.size_bytes FROM variants v WHERE v.status = 'ready'
               AND NOT EXISTS (SELECT 1 FROM variant_requests r
                               WHERE r.variant_id = v.id AND r.confirmed_at IS NULL)
             ORDER BY v.finished_at",
        )
        .fetch_all(&self.db)
        .await?;
        let mut excess = total - self.budget_bytes;
        for (id, size) in candidates {
            if excess <= 0 {
                break;
            }
            self.delete(id).await?;
            excess -= size;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(profile: Profile, video: VideoPlan) -> EncodeJob {
        EncodeJob {
            input: "/media/m.mkv".into(),
            profile,
            video,
            video_index: 0,
            source_height: Some(2160),
            source_hevc: false,
            hdr: false,
            source: gpu::Source {
                decode: true,
                deep: true,
            },
            audio: vec![AudioOut {
                index: 2,
                channels: Some(6),
                title: None,
            }],
            encoders: Encoders {
                hevc_nvenc: true,
                av1_nvenc: true,
                libsvtav1: true,
                gpu_filters: false,
            },
        }
    }

    fn joined(job: &EncodeJob) -> String {
        ffmpeg_args(job, Path::new("/cache/v/1/video.partial.mp4")).join(" ")
    }

    #[test]
    fn video_plan_rules() {
        let all = Encoders {
            hevc_nvenc: true,
            av1_nvenc: true,
            libsvtav1: true,
            gpu_filters: true,
        };
        assert_eq!(
            video_plan(Profile::Max, "hevc", false, &all),
            VideoPlan::Copy
        );
        assert_eq!(
            video_plan(Profile::Max, "mpeg4", false, &all),
            VideoPlan::Hevc
        );
        assert_eq!(
            video_plan(Profile::Mobile, "h264", true, &all),
            VideoPlan::Av1
        );
        assert_eq!(
            video_plan(Profile::Mobile, "h264", false, &all),
            VideoPlan::Hevc
        );
        assert_eq!(
            video_plan(Profile::Saver, "h264", true, &Encoders::default()),
            VideoPlan::Hevc
        );
        assert_eq!(codec_label(VideoPlan::Copy, "h264"), "h264");
    }

    #[test]
    fn max_copies_video_and_keeps_surround() {
        let args = joined(&job(Profile::Max, VideoPlan::Copy));
        assert!(args.contains("-map 0:0 -c:v copy"));
        assert!(args.contains("-map 0:2 -c:a aac -ac:a:0 6 -b:a:0 384k"));
        assert!(args.contains("-movflags +faststart"));
        assert!(!args.contains("-vf"));
        assert!(args.ends_with("/cache/v/1/video.partial.mp4"));
    }

    #[test]
    fn mobile_scales_and_uses_the_gpu() {
        let args = joined(&job(Profile::Mobile, VideoPlan::Av1));
        assert!(args.contains("-vf scale=-2:720,format=yuv420p"));
        assert!(args.contains(
            "-c:v av1_nvenc -preset p5 -rc vbr -b:v 1500k -maxrate 2250k -bufsize 4500k"
        ));
        assert!(args.contains("-ac:a:0 2 -b:a:0 128k"));
    }

    #[test]
    fn saver_hevc_scales_then_tonemaps_hdr_and_tags_hvc1() {
        let mut j = job(Profile::Saver, VideoPlan::Hevc);
        j.hdr = true;
        j.encoders = Encoders::default();
        let args = joined(&j);
        assert!(args.contains(&format!(
            "-vf scale=-2:480,{},format=yuv420p",
            transcode::TONEMAP
        )));
        assert!(args.contains("-c:v libx265 -preset medium -b:v 700k"));
        assert!(args.contains("-tag:v hvc1"));
        assert!(!args.contains("cuda"));
    }

    #[test]
    fn gpu_filters_keep_frames_on_the_gpu() {
        let mut j = job(Profile::Saver, VideoPlan::Av1);
        j.hdr = true;
        j.encoders.gpu_filters = true;
        let args = joined(&j);
        assert!(args.starts_with(
            "-hide_banner -loglevel error -nostdin -y -init_hw_device cuda=cu:0 \
             -filter_hw_device cu -hwaccel cuda -hwaccel_device cu -hwaccel_output_format cuda \
             -i /media/m.mkv"
        ));
        assert!(args.contains(&format!(
            "-vf scale_cuda=w=-2:h=480:format=p010le,{} -c:v av1_nvenc",
            gpu::TONEMAP
        )));
    }

    #[test]
    fn gpu_filters_need_an_nvenc_encoder() {
        let mut j = job(Profile::Saver, VideoPlan::Av1);
        j.encoders = Encoders {
            libsvtav1: true,
            gpu_filters: true,
            ..Encoders::default()
        };
        assert!(!j.on_gpu());
        let args = joined(&j);
        assert!(
            args.contains("-vf scale=-2:480,format=yuv420p -c:v libsvtav1 -preset 8 -b:v 700k")
        );
        assert!(
            !args.contains("-maxrate"),
            "SVT-AV1's VBR rejects a max bitrate"
        );
        assert!(!job(Profile::Max, VideoPlan::Copy).on_gpu());
    }

    #[test]
    fn small_sources_are_not_upscaled() {
        let mut j = job(Profile::Mobile, VideoPlan::Hevc);
        j.source_height = Some(480);
        assert!(joined(&j).contains("-vf format=yuv420p"));
    }

    #[test]
    fn every_audio_track_goes_in_with_its_own_layout() {
        let mut j = job(Profile::Max, VideoPlan::Copy);
        j.audio = vec![
            AudioOut {
                index: 1,
                channels: Some(6),
                title: None,
            },
            AudioOut {
                index: 3,
                channels: Some(2),
                title: Some("Commentary".into()),
            },
        ];
        let args = joined(&j);
        assert!(args.contains(
            "-map 0:1 -map 0:3 -c:a aac -ac:a:0 6 -b:a:0 384k -ac:a:1 2 -b:a:1 192k \
                 -metadata:s:a:1 handler_name=Commentary"
        ));
        j.audio.clear();
        assert!(
            !joined(&j).contains("-c:a"),
            "a silent video has no audio options"
        );
    }
}
