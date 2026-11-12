//! Subtitles: external files next to videos, conversion to WebVTT, time shifting.

use std::{path::Path, time::Duration};

use tokio::process::Command;

use super::normalize_language;
use crate::library::media_info::TrackKind;

pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    pub language: Option<String>,
    pub forced: bool,
    pub sdh: bool,
    pub format: String,
}

impl Sidecar {
    pub fn kind(&self) -> TrackKind {
        if self.forced {
            TrackKind::Forced
        } else if self.sdh {
            TrackKind::Sdh
        } else {
            TrackKind::Normal
        }
    }
}

/// Reads `Movie.fr.forced.srt` as a subtitle of `Movie.mkv`. Returns `None` if
/// `subtitle` isn't a subtitle file belonging to the video with stem `video_stem`.
pub fn parse_sidecar(video_stem: &str, subtitle: &Path) -> Option<Sidecar> {
    let name = subtitle.file_name()?.to_str()?;
    let (stem, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    if !SUBTITLE_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    let rest = stem.strip_prefix(video_stem)?;
    if !(rest.is_empty() || rest.starts_with('.') || rest.starts_with('_') || rest.starts_with('-'))
    {
        return None; // `Movie 2.srt` doesn't belong to `Movie.mkv`
    }
    let mut sidecar = Sidecar {
        language: None,
        forced: false,
        sdh: false,
        format: ext,
    };
    for token in rest.split(['.', '_', '-']).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "forced" | "force" => sidecar.forced = true,
            "sdh" | "cc" | "hi" => sidecar.sdh = true,
            "default" => {}
            other if sidecar.language.is_none() && (2..=3).contains(&other.len()) => {
                sidecar.language = normalize_language(other);
            }
            _ => {}
        }
    }
    Some(sidecar)
}

/// Converts a subtitle (embedded stream or file) to WebVTT with ffmpeg.
pub async fn to_webvtt(
    ffmpeg: &Path,
    input: &Path,
    stream_index: Option<i64>,
) -> anyhow::Result<String> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(input);
    if let Some(index) = stream_index {
        cmd.args(["-map", &format!("0:{index}")]);
    }
    cmd.args(["-f", "webvtt", "-"]).kill_on_drop(true);
    // Embedded subtitles mean reading the whole file: allow time.
    let out = tokio::time::timeout(Duration::from_secs(300), cmd.output()).await??;
    anyhow::ensure!(
        out.status.success(),
        "ffmpeg: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_timestamp(ts: &str) -> Option<i64> {
    let ts = ts.trim();
    let (hms, millis) = ts.split_once('.')?;
    let parts: Vec<i64> = hms
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let seconds = match parts.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        _ => return None,
    };
    Some(seconds * 1000 + millis.get(..3)?.parse::<i64>().ok()?)
}

fn format_timestamp(ms: i64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

/// Shifts every cue back by `offset_ms` (playback started at that position) and
/// drops cues that end before 0.
pub fn shift_webvtt(vtt: &str, offset_ms: i64) -> String {
    if offset_ms == 0 {
        return vtt.to_owned();
    }
    let mut out = String::with_capacity(vtt.len());
    let mut skipping = false;
    for line in vtt.lines() {
        if let Some((start, rest)) = line.split_once("-->") {
            let mut end_and_settings = rest.trim().splitn(2, ' ');
            let end = end_and_settings.next().unwrap_or_default();
            let settings = end_and_settings.next();
            if let (Some(start), Some(end)) = (parse_timestamp(start), parse_timestamp(end)) {
                let (start, end) = (start - offset_ms, end - offset_ms);
                skipping = end <= 0;
                if !skipping {
                    out.push_str(&format_timestamp(start.max(0)));
                    out.push_str(" --> ");
                    out.push_str(&format_timestamp(end));
                    if let Some(settings) = settings {
                        out.push(' ');
                        out.push_str(settings);
                    }
                    out.push('\n');
                }
                continue;
            }
        }
        if line.trim().is_empty() {
            skipping = false;
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sidecar_names() {
        let p = |name: &str| parse_sidecar("Movie (2010)", Path::new(name));
        assert_eq!(
            p("Movie (2010).fr.srt"),
            Some(Sidecar {
                language: Some("fre".into()),
                forced: false,
                sdh: false,
                format: "srt".into()
            })
        );
        let forced = p("Movie (2010).fre.forced.ass").unwrap();
        assert!(forced.forced);
        assert_eq!(forced.kind(), TrackKind::Forced);
        assert_eq!(p("Movie (2010).en.sdh.SRT").unwrap().kind(), TrackKind::Sdh);
        assert_eq!(p("Movie (2010).srt").unwrap().language, None);
        assert_eq!(p("Movie (2010) 2.srt"), None);
        assert_eq!(p("Movie (2010).fr.nfo"), None);
        assert_eq!(p("Other.fr.srt"), None);
    }

    const VTT: &str = "WEBVTT\n\n1\n00:00:05.000 --> 00:00:07.500\nHello\n\n2\n00:01:00.000 --> 00:01:02.000 line:90%\nWorld\nagain\n\n";

    #[test]
    fn shifts_cues_and_drops_past_ones() {
        let shifted = shift_webvtt(VTT, 30_000);
        assert!(!shifted.contains("Hello"));
        assert!(shifted.contains("00:00:30.000 --> 00:00:32.000 line:90%\nWorld\nagain"));
        assert!(shifted.starts_with("WEBVTT"));
        assert_eq!(shift_webvtt(VTT, 0), VTT);
    }

    #[test]
    fn cue_straddling_the_offset_starts_at_zero() {
        let shifted = shift_webvtt(VTT, 6_000);
        assert!(shifted.contains("00:00:00.000 --> 00:00:01.500\nHello"));
    }

    #[test]
    fn timestamps_roundtrip() {
        assert_eq!(parse_timestamp("01:02:03.456"), Some(3_723_456));
        assert_eq!(parse_timestamp("02:03.456"), Some(123_456));
        assert_eq!(format_timestamp(3_723_456), "01:02:03.456");
    }
}
