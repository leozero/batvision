//! HLS transcoding sessions: one ffmpeg process per playing device, writing
//! fMP4 segments to the cache. A seek outside what's produced starts a new session.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::process::{Child, Command};

use super::{Method, Plan, gpu};

pub const SEGMENT_SECONDS: u32 = 6;
/// A session nobody fetched from for this long is stopped and deleted.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Which H.264 encoder to use when the video must be re-encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoEncoder {
    Nvenc,
    Software,
}

#[derive(Debug, Clone)]
pub struct TranscodeJob {
    pub input: PathBuf,
    pub plan: Plan,
    pub video_index: Option<i64>,
    pub start_ms: i64,
    pub encoder: VideoEncoder,
    /// Set when the server's GPU filters work (with NVENC): decode, scale and tone-map there.
    pub gpu: Option<gpu::Source>,
    pub source_width: Option<i64>,
}

/// Output size for a source scaled to `height` (even width, like `scale=-2:h`).
fn scaled_size(width: i64, height: i64, target: i64) -> (i64, i64) {
    let w = (width as f64 * target as f64 / height as f64 / 2.0).round() as i64 * 2;
    (w.max(2), target)
}

pub const TONEMAP: &str = "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,\
tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv";

/// Maximum video bitrate for an output height, in kbit/s.
pub fn max_bitrate_kbps(height: i64) -> i64 {
    match height {
        h if h > 1440 => 25_000,
        h if h > 1080 => 16_000,
        h if h > 720 => 10_000,
        h if h > 480 => 5_000,
        _ => 2_500,
    }
}

/// ffmpeg arguments for a job writing into `out_dir`. Pure, for testing.
pub fn ffmpeg_args(job: &TranscodeJob, source_height: Option<i64>, out_dir: &Path) -> Vec<String> {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "warning", "-nostdin", "-y"]
        .map(String::from)
        .to_vec();
    let plan = &job.plan;
    let video = job.video_index.map(|v| format!("0:{v}"));
    let encode_video = plan.method == Method::Transcode;
    // Burning subtitles in needs the output size up front (the overlay runs on the CPU).
    let out_size = match (job.source_width, source_height) {
        (Some(w), Some(h)) => Some(scaled_size(w, h, plan.max_height.unwrap_or(h).min(h))),
        _ => None,
    };
    let gpu = job
        .gpu
        .filter(|_| encode_video && job.encoder == VideoEncoder::Nvenc)
        .filter(|_| plan.burn_subtitle.is_none() || out_size.is_some());

    if let Some(source) = gpu {
        args.extend(gpu::input_args(source));
    }
    if job.start_ms > 0 {
        args.extend(["-ss".into(), format!("{:.3}", job.start_ms as f64 / 1000.0)]);
    }
    args.extend(["-i".into(), job.input.to_string_lossy().into_owned()]);

    match (&video, encode_video) {
        (Some(v), true) if let Some(source) = gpu => {
            let graph = match (plan.burn_subtitle, out_size) {
                (Some(sub), Some((w, h))) => {
                    let size = format!("w={w}:h={h}");
                    format!(
                        "[{v}]{},hwdownload,format=yuv420p[main];\
                         [0:{sub}]scale={w}:{h}[sub];[main][sub]overlay=eof_action=pass[vout]",
                        gpu::chain(source, Some(&size), plan.tonemap)
                    )
                }
                _ => {
                    let size = plan.max_height.map(|h| format!("w=-2:h={h}"));
                    format!(
                        "[{v}]{}[vout]",
                        gpu::chain(source, size.as_deref(), plan.tonemap)
                    )
                }
            };
            args.extend([
                "-filter_complex".into(),
                graph,
                "-map".into(),
                "[vout]".into(),
            ]);
            video_encoder_args(&mut args, job, plan, source_height);
        }
        (Some(v), true) if plan.burn_subtitle.is_none() => {
            // Scaling first: tone mapping the smaller picture is much cheaper.
            let mut filters = Vec::new();
            if let Some(height) = plan.max_height {
                filters.push(format!("scale=-2:{height}"));
            }
            if plan.tonemap {
                filters.push(TONEMAP.to_owned());
            }
            filters.push("format=yuv420p".into());
            args.extend([
                "-filter_complex".into(),
                format!("[{v}]{}[vout]", filters.join(",")),
                "-map".into(),
                "[vout]".into(),
            ]);
            video_encoder_args(&mut args, job, plan, source_height);
        }
        (Some(v), true) => {
            let mut graph = String::new();
            let mut current = format!("[{v}]");
            if plan.tonemap {
                graph.push_str(&format!("{current}{TONEMAP}[tm];"));
                current = "[tm]".into();
            }
            if let Some(sub) = plan.burn_subtitle {
                graph.push_str(&format!("{current}[0:{sub}]overlay=eof_action=pass[ov];"));
                current = "[ov]".into();
            }
            let mut tail = Vec::new();
            if let Some(height) = plan.max_height {
                tail.push(format!("scale=-2:{height}"));
            }
            tail.push("format=yuv420p".into());
            graph.push_str(&format!("{current}{}[vout]", tail.join(",")));
            args.extend([
                "-filter_complex".into(),
                graph,
                "-map".into(),
                "[vout]".into(),
            ]);
            video_encoder_args(&mut args, job, plan, source_height);
        }
        (Some(v), false) => args.extend(["-map".into(), v.clone(), "-c:v".into(), "copy".into()]),
        (None, _) => {}
    }
    audio_and_hls_args(&mut args, plan, out_dir);
    args
}

fn video_encoder_args(
    args: &mut Vec<String>,
    job: &TranscodeJob,
    plan: &Plan,
    source_height: Option<i64>,
) {
    let height = plan.max_height.or(source_height).unwrap_or(1080);
    let rate = plan.video_kbps.unwrap_or_else(|| max_bitrate_kbps(height));
    let keyframes = format!("expr:gte(t,n_forced*{SEGMENT_SECONDS})");
    match job.encoder {
        VideoEncoder::Nvenc => args.extend(
            [
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p4",
                "-tune",
                "hq",
                "-rc",
                "vbr",
                "-cq",
                "21",
                "-forced-idr",
                "1",
            ]
            .map(String::from),
        ),
        VideoEncoder::Software => args.extend(
            [
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "21",
                "-sc_threshold",
                "0",
            ]
            .map(String::from),
        ),
    }
    args.extend([
        "-maxrate".into(),
        format!("{rate}k"),
        "-bufsize".into(),
        format!("{}k", rate * 2),
        "-force_key_frames".into(),
        keyframes,
    ]);
}

fn audio_and_hls_args(args: &mut Vec<String>, plan: &Plan, out_dir: &Path) {
    if let Some(audio) = plan.audio_index {
        args.extend(["-map".into(), format!("0:{audio}")]);
        if plan.copy_audio {
            args.extend(["-c:a".into(), "copy".into()]);
        } else {
            args.extend(["-c:a", "aac", "-ac", "2", "-b:a"].map(String::from));
            args.push(format!("{}k", plan.audio_kbps));
        }
    }

    let dir = out_dir.to_string_lossy();
    args.extend(["-max_muxing_queue_size", "2048", "-f", "hls", "-hls_time"].map(String::from));
    args.push(SEGMENT_SECONDS.to_string());
    args.extend(
        [
            "-hls_list_size",
            "0",
            "-hls_playlist_type",
            "event",
            "-hls_segment_type",
            "fmp4",
            "-hls_fmp4_init_filename",
            "init.mp4",
            "-hls_flags",
            "independent_segments+temp_file",
            "-hls_segment_filename",
        ]
        .map(String::from),
    );
    args.push(format!("{dir}/seg%05d.m4s"));
    args.push(format!("{dir}/index.m3u8"));
}

struct Session {
    child: Child,
    dir: PathBuf,
    device_id: i64,
    last_access: Instant,
}

pub struct TranscodeManager {
    root: PathBuf,
    ffmpeg: PathBuf,
    sessions: Mutex<HashMap<String, Session>>,
}

impl TranscodeManager {
    pub fn new(root: PathBuf, ffmpeg: PathBuf) -> Self {
        Self {
            root,
            ffmpeg,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Deletes leftovers of a previous run (call once at startup).
    pub fn clean(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }

    /// Starts a session for `device_id`, stopping its previous one. Returns its id.
    pub fn start(
        &self,
        device_id: i64,
        job: &TranscodeJob,
        source_height: Option<i64>,
    ) -> anyhow::Result<String> {
        let previous: Vec<String> = self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| s.device_id == device_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in previous {
            self.stop(&id);
        }

        // The id is the only credential for the segments: make it unguessable.
        let id = crate::auth::crypto::random_token("");
        let dir = self.root.join(&id);
        std::fs::create_dir_all(&dir)?;
        let log = std::fs::File::create(dir.join("ffmpeg.log"))?;
        let args = ffmpeg_args(job, source_height, &dir);
        tracing::info!(session = %&id[..8], method = ?job.plan.method, "starting transcode");
        let child = Command::new(&self.ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .kill_on_drop(true)
            .spawn()?;
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Session {
                child,
                dir,
                device_id,
                last_access: Instant::now(),
            },
        );
        Ok(id)
    }

    /// The session's folder, marking it as in use.
    pub fn touch(&self, id: &str) -> Option<PathBuf> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions.get_mut(id)?;
        session.last_access = Instant::now();
        Some(session.dir.clone())
    }

    /// Whether ffmpeg is still producing segments for this session.
    pub fn is_running(&self, id: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        sessions
            .get_mut(id)
            .is_some_and(|s| matches!(s.child.try_wait(), Ok(None)))
    }

    pub fn stop(&self, id: &str) {
        if let Some(mut session) = self.sessions.lock().unwrap().remove(id) {
            let _ = session.child.start_kill();
            let _ = std::fs::remove_dir_all(&session.dir);
        }
    }

    pub fn owner(&self, id: &str) -> Option<i64> {
        self.sessions.lock().unwrap().get(id).map(|s| s.device_id)
    }

    /// Stops sessions nobody fetches from anymore.
    pub fn spawn_reaper(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let idle: Vec<String> = self
                    .sessions
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(_, s)| s.last_access.elapsed() > IDLE_TIMEOUT)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in idle {
                    tracing::info!(session = %&id[..8], "stopping idle transcode");
                    self.stop(&id);
                }
            }
        });
    }
}

/// Whether `name` is a file an HLS session may serve.
pub fn valid_hls_file(name: &str) -> bool {
    name == "index.m3u8"
        || name == "init.mp4"
        || (name.starts_with("seg")
            && name.ends_with(".m4s")
            && name[3..name.len() - 4].chars().all(|c| c.is_ascii_digit())
            && name.len() == 3 + 5 + 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(method: Method) -> TranscodeJob {
        TranscodeJob {
            input: "/media/Movie.mkv".into(),
            plan: Plan {
                method,
                audio_index: Some(2),
                copy_audio: method == Method::Remux,
                audio_kbps: 192,
                burn_subtitle: None,
                max_height: None,
                video_kbps: None,
                tonemap: false,
            },
            video_index: Some(0),
            start_ms: 0,
            encoder: VideoEncoder::Software,
            gpu: None,
            source_width: Some(1920),
        }
    }

    fn joined(job: &TranscodeJob) -> String {
        ffmpeg_args(job, Some(1080), Path::new("/cache/s1")).join(" ")
    }

    #[test]
    fn remux_copies_both_streams() {
        let args = joined(&job(Method::Remux));
        assert!(args.contains("-map 0:0 -c:v copy"));
        assert!(args.contains("-map 0:2 -c:a copy"));
        assert!(args.contains("-hls_segment_type fmp4"));
        assert!(args.ends_with("/cache/s1/seg%05d.m4s /cache/s1/index.m3u8"));
        assert!(!args.contains("-ss"));
    }

    #[test]
    fn audio_transcode_keeps_video() {
        let args = joined(&job(Method::TranscodeAudio));
        assert!(args.contains("-c:v copy"));
        assert!(args.contains("-c:a aac -ac 2 -b:a 192k"));
    }

    #[test]
    fn quality_level_caps_the_bitrate() {
        let mut j = job(Method::Transcode);
        j.plan.max_height = Some(480);
        j.plan.video_kbps = Some(1_000);
        j.plan.audio_kbps = 96;
        let args = joined(&j);
        assert!(args.contains("scale=-2:480"));
        assert!(args.contains("-maxrate 1000k -bufsize 2000k"));
        assert!(args.contains("-c:a aac -ac 2 -b:a 96k"));
    }

    #[test]
    fn full_transcode_with_nvenc_scale_burn_and_seek() {
        let mut j = job(Method::Transcode);
        j.encoder = VideoEncoder::Nvenc;
        j.plan.max_height = Some(720);
        j.plan.burn_subtitle = Some(6);
        j.start_ms = 90_500;
        let args = joined(&j);
        assert!(args.starts_with(
            "-hide_banner -loglevel warning -nostdin -y -ss 90.500 -i /media/Movie.mkv"
        ));
        assert!(args.contains(
            "[0:0][0:6]overlay=eof_action=pass[ov];[ov]scale=-2:720,format=yuv420p[vout]"
        ));
        assert!(args.contains("-map [vout]"));
        assert!(args.contains("-c:v h264_nvenc"));
        assert!(args.contains("-maxrate 5000k"));
        assert!(args.contains("-force_key_frames expr:gte(t,n_forced*6)"));
    }

    #[test]
    fn hdr_is_tone_mapped_before_burning_subtitles() {
        let mut j = job(Method::Transcode);
        j.plan.tonemap = true;
        j.plan.burn_subtitle = Some(6);
        let args = joined(&j);
        let tonemap = args.find("tonemap=").unwrap();
        let overlay = args.find("overlay").unwrap();
        assert!(tonemap < overlay);
        assert!(args.contains("-c:v libx264"));
    }

    #[test]
    fn cpu_scales_before_tone_mapping() {
        let mut j = job(Method::Transcode);
        j.plan.tonemap = true;
        j.plan.max_height = Some(720);
        assert!(joined(&j).contains(&format!("[0:0]scale=-2:720,{TONEMAP},format=yuv420p[vout]")));
    }

    fn gpu_job() -> TranscodeJob {
        let mut j = job(Method::Transcode);
        j.encoder = VideoEncoder::Nvenc;
        j.gpu = Some(gpu::Source {
            decode: true,
            deep: true,
        });
        j.plan.tonemap = true;
        j.plan.max_height = Some(720);
        j.start_ms = 5_000;
        j
    }

    #[test]
    fn gpu_decodes_scales_and_tone_maps() {
        let args = joined(&gpu_job());
        assert!(args.starts_with(
            "-hide_banner -loglevel warning -nostdin -y -init_hw_device cuda=cu:0 \
             -filter_hw_device cu -hwaccel cuda -hwaccel_device cu -hwaccel_output_format cuda \
             -ss 5.000 -i /media/Movie.mkv"
        ));
        assert!(args.contains(&format!(
            "-filter_complex [0:0]scale_cuda=w=-2:h=720:format=p010le,{}[vout]",
            gpu::TONEMAP
        )));
        assert!(args.contains("-c:v h264_nvenc"));
    }

    #[test]
    fn gpu_burns_subtitles_on_the_scaled_picture() {
        let mut j = gpu_job();
        j.plan.burn_subtitle = Some(6);
        let args = joined(&j);
        assert!(args.contains(&format!(
            "[0:0]scale_cuda=w=1280:h=720:format=p010le,{},hwdownload,format=yuv420p[main];\
             [0:6]scale=1280:720[sub];[main][sub]overlay=eof_action=pass[vout]",
            gpu::TONEMAP
        )));
        // Without the source size, the CPU filters burn them in.
        j.source_width = None;
        assert!(!joined(&j).contains("cuda"));
    }

    #[test]
    fn gpu_filters_need_nvenc() {
        let mut j = gpu_job();
        j.encoder = VideoEncoder::Software;
        assert!(!joined(&j).contains("cuda"));
        assert_eq!(scaled_size(3840, 1606, 720), (1722, 720));
    }

    #[test]
    fn only_session_files_are_served() {
        assert!(valid_hls_file("index.m3u8"));
        assert!(valid_hls_file("init.mp4"));
        assert!(valid_hls_file("seg00042.m4s"));
        assert!(!valid_hls_file("ffmpeg.log"));
        assert!(!valid_hls_file("../index.m3u8"));
        assert!(!valid_hls_file("seg1.m4s"));
    }
}
