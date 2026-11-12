//! Encoded variants for download: an MP4 per media file × profile × codec × audio
//! track, plus its text subtitles as WebVTT. Encoded only when a device asks.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{Notify, Semaphore},
};
use utoipa::ToSchema;

use super::plan::Profile;
use crate::{
    db::{Db, now},
    ffmpeg::FfmpegInfo,
    library::media_info::{self, MediaInfo, TrackKind},
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

/// Audio track for a language: a normal track in that language, else the file's
/// default, else the first.
pub fn pick_audio(info: &MediaInfo, language: Option<&str>) -> Option<i64> {
    let wanted = language.and_then(normalize_language);
    info.audio
        .iter()
        .find(|a| {
            a.kind == TrackKind::Normal
                && wanted.is_some()
                && a.language.as_deref().and_then(normalize_language) == wanted
        })
        .or_else(|| {
            info.audio
                .iter()
                .find(|a| a.default && a.kind != TrackKind::Commentary)
        })
        .or(info.audio.first())
        .map(|a| a.index)
}

#[derive(Debug, Clone)]
pub struct EncodeJob {
    pub input: PathBuf,
    pub profile: Profile,
    pub video: VideoPlan,
    pub video_index: i64,
    pub source_height: Option<i64>,
    pub hdr: bool,
    pub source: gpu::Source,
    pub audio_index: Option<i64>,
    pub audio_channels: Option<i64>,
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
    // Apple players need the hvc1 tag for HEVC in MP4.
    let hevc_out = job.video == VideoPlan::Hevc;
    if hevc_out {
        args.extend(["-tag:v".into(), "hvc1".into()]);
    }

    if let Some(audio) = job.audio_index {
        args.extend([
            "-map".into(),
            format!("0:{audio}"),
            "-c:a".into(),
            "aac".into(),
        ]);
        let surround = job.audio_channels.unwrap_or(2) > 2;
        let (channels, bitrate) = match job.profile {
            Profile::Max if surround => ("6", "384k"),
            Profile::Max => ("2", "192k"),
            Profile::Mobile => ("2", "128k"),
            Profile::Saver => ("2", "96k"),
        };
        args.extend(["-ac".into(), channels.into(), "-b:a".into(), bitrate.into()]);
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
pub struct Encoder {
    db: Db,
    root: PathBuf,
    media_root: PathBuf,
    ffmpeg: PathBuf,
    wake: Notify,
    slots: Arc<Semaphore>,
    budget_bytes: i64,
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
                                tracing::error!(variant = id, error = %format!("{err:#}"), "encode failed");
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
        let audio = row
            .audio_index
            .and_then(|i| info.audio.iter().find(|a| a.index == i));
        let job = EncodeJob {
            input: input.clone(),
            profile: row.profile,
            video: plan,
            video_index: video.index,
            source_height: video.height,
            hdr: video.hdr,
            source: gpu::Source::of(video),
            audio_index: row.audio_index,
            audio_channels: audio.and_then(|a| a.channels),
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

        if let Err(err) = self.run(id, &job, &partial, duration_us).await {
            if !job.on_gpu() {
                return Err(err);
            }
            // A source the GPU chain can't take: the CPU filters handle anything.
            tracing::warn!(variant = id, error = %format!("{err:#}"), "GPU encode failed, retrying on the CPU");
            let mut cpu = job.clone();
            cpu.encoders.gpu_filters = false;
            self.run(id, &cpu, &partial, duration_us).await?;
        }
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
    ) -> anyhow::Result<()> {
        let mut child = Command::new(&self.ffmpeg)
            .args(ffmpeg_args(job, partial))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child.stdout.take().expect("piped");
        let mut lines = BufReader::new(stdout).lines();
        let mut last_update = Instant::now();
        while let Some(line) = lines.next_line().await? {
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
        let output = child.wait_with_output().await?;
        anyhow::ensure!(
            output.status.success(),
            "ffmpeg: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }

    /// Deletes a variant's files and row.
    pub async fn delete(&self, id: i64) -> anyhow::Result<()> {
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
    use crate::library::media_info::AudioTrack;

    fn job(profile: Profile, video: VideoPlan) -> EncodeJob {
        EncodeJob {
            input: "/media/m.mkv".into(),
            profile,
            video,
            video_index: 0,
            source_height: Some(2160),
            hdr: false,
            source: gpu::Source {
                decode: true,
                deep: true,
            },
            audio_index: Some(2),
            audio_channels: Some(6),
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
        assert!(args.contains("-map 0:2 -c:a aac -ac 6 -b:a 384k"));
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
        assert!(args.contains("-ac 2 -b:a 128k"));
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

    fn track(index: i64, lang: &str, kind: TrackKind, default: bool) -> AudioTrack {
        AudioTrack {
            index,
            codec: "aac".into(),
            language: Some(lang.into()),
            title: None,
            channels: Some(2),
            default,
            kind,
        }
    }

    #[test]
    fn audio_is_picked_by_language() {
        let info = MediaInfo {
            duration_ms: None,
            container: None,
            size_bytes: None,
            bitrate: None,
            video: None,
            audio: vec![
                track(1, "eng", TrackKind::Normal, true),
                track(2, "fra", TrackKind::Commentary, false),
                track(3, "fre", TrackKind::Normal, false),
            ],
            subtitles: vec![],
        };
        assert_eq!(
            pick_audio(&info, Some("fre")),
            Some(3),
            "fra/fre are the same, commentary skipped"
        );
        assert_eq!(
            pick_audio(&info, Some("jpn")),
            Some(1),
            "missing: file default"
        );
        assert_eq!(pick_audio(&info, None), Some(1));
    }
}
