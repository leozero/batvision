//! Playback-relevant facts extracted from `ffprobe` JSON.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct MediaInfo {
    pub duration_ms: Option<i64>,
    /// ffprobe format names, e.g. `matroska,webm` or `mov,mp4,m4a,3gp,3g2,mj2`.
    pub container: Option<String>,
    pub size_bytes: Option<i64>,
    pub bitrate: Option<i64>,
    pub video: Option<VideoStream>,
    pub audio: Vec<AudioTrack>,
    pub subtitles: Vec<SubtitleTrack>,
}

#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct VideoStream {
    pub index: i64,
    pub codec: String,
    pub profile: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub bit_depth: Option<i64>,
    /// HDR10/HLG/Dolby Vision, detected from the transfer characteristics.
    pub hdr: bool,
}

/// What a track is for, used to carry a track choice from one episode to the next.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Normal,
    Commentary,
    /// Subtitles shown only when characters speak another language.
    Forced,
    /// Subtitles for the deaf and hard of hearing.
    Sdh,
}

#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct AudioTrack {
    pub index: i64,
    pub codec: String,
    /// ISO 639-2 code as tagged in the file (`fre`, `eng`…), if any.
    pub language: Option<String>,
    pub title: Option<String>,
    pub channels: Option<i64>,
    pub default: bool,
    pub kind: TrackKind,
}

#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct SubtitleTrack {
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub kind: TrackKind,
    /// Text subtitles can be converted for the client; image ones must be burned in.
    pub text: bool,
}

#[derive(Deserialize)]
struct Probe {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Deserialize)]
struct Format {
    format_name: Option<String>,
    duration: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Deserialize, Default)]
struct Disposition {
    #[serde(default)]
    default: i64,
    #[serde(default)]
    forced: i64,
    #[serde(default)]
    hearing_impaired: i64,
    #[serde(default)]
    comment: i64,
    #[serde(default)]
    attached_pic: i64,
}

#[derive(Deserialize, Default)]
struct Tags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize)]
struct Stream {
    index: i64,
    codec_type: Option<String>,
    codec_name: Option<String>,
    profile: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    bits_per_raw_sample: Option<String>,
    pix_fmt: Option<String>,
    color_transfer: Option<String>,
    channels: Option<i64>,
    #[serde(default)]
    disposition: Disposition,
    #[serde(default)]
    tags: Tags,
}

const TEXT_SUBTITLE_CODECS: &[&str] =
    &["subrip", "srt", "ass", "ssa", "webvtt", "mov_text", "text"];

fn kind_of(disposition: &Disposition, title: Option<&str>) -> TrackKind {
    let title = title.unwrap_or_default().to_lowercase();
    if disposition.comment == 1 || title.contains("commentary") || title.contains("commentaire") {
        TrackKind::Commentary
    } else if disposition.forced == 1 || title.contains("forced") || title.contains("forcé") {
        TrackKind::Forced
    } else if disposition.hearing_impaired == 1 || title.contains("sdh") || title.contains("sme") {
        TrackKind::Sdh
    } else {
        TrackKind::Normal
    }
}

fn language(tags: &Tags) -> Option<String> {
    tags.language
        .clone()
        .filter(|l| !l.is_empty() && l != "und")
}

pub fn parse(probe_json: &str) -> anyhow::Result<MediaInfo> {
    let probe: Probe = serde_json::from_str(probe_json)?;
    let format = probe.format;
    let parse_i64 = |v: Option<&String>| v.and_then(|s| s.parse::<f64>().ok());

    let mut info = MediaInfo {
        duration_ms: format
            .as_ref()
            .and_then(|f| parse_i64(f.duration.as_ref()))
            .map(|secs| (secs * 1000.0) as i64),
        container: format.as_ref().and_then(|f| f.format_name.clone()),
        size_bytes: format
            .as_ref()
            .and_then(|f| parse_i64(f.size.as_ref()))
            .map(|v| v as i64),
        bitrate: format
            .as_ref()
            .and_then(|f| parse_i64(f.bit_rate.as_ref()))
            .map(|v| v as i64),
        video: None,
        audio: Vec::new(),
        subtitles: Vec::new(),
    };

    for stream in probe.streams {
        let codec = stream
            .codec_name
            .clone()
            .unwrap_or_else(|| "unknown".into());
        match stream.codec_type.as_deref() {
            // Cover art is stored as a video stream: skip it.
            Some("video") if info.video.is_none() && stream.disposition.attached_pic == 0 => {
                let hdr = matches!(
                    stream.color_transfer.as_deref(),
                    Some("smpte2084" | "arib-std-b67")
                );
                let bit_depth = stream
                    .bits_per_raw_sample
                    .as_deref()
                    .and_then(|b| b.parse().ok())
                    .or_else(|| {
                        stream
                            .pix_fmt
                            .as_deref()
                            .map(|f| if f.contains("10") { 10 } else { 8 })
                    });
                info.video = Some(VideoStream {
                    index: stream.index,
                    codec,
                    profile: stream.profile,
                    width: stream.width,
                    height: stream.height,
                    bit_depth,
                    hdr,
                });
            }
            Some("audio") => info.audio.push(AudioTrack {
                index: stream.index,
                codec,
                language: language(&stream.tags),
                kind: kind_of(&stream.disposition, stream.tags.title.as_deref()),
                title: stream.tags.title,
                channels: stream.channels,
                default: stream.disposition.default == 1,
            }),
            Some("subtitle") => info.subtitles.push(SubtitleTrack {
                index: stream.index,
                text: TEXT_SUBTITLE_CODECS.contains(&codec.as_str()),
                codec,
                language: language(&stream.tags),
                kind: kind_of(&stream.disposition, stream.tags.title.as_deref()),
                title: stream.tags.title,
                default: stream.disposition.default == 1,
            }),
            _ => {}
        }
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "streams": [
        { "index": 0, "codec_type": "video", "codec_name": "hevc", "profile": "Main 10",
          "width": 3840, "height": 2160, "pix_fmt": "yuv420p10le", "color_transfer": "smpte2084",
          "disposition": { "default": 1 } },
        { "index": 1, "codec_type": "audio", "codec_name": "eac3", "channels": 6,
          "disposition": { "default": 1 }, "tags": { "language": "fre", "title": "VFF" } },
        { "index": 2, "codec_type": "audio", "codec_name": "aac", "channels": 2,
          "disposition": { "comment": 1 }, "tags": { "language": "eng", "title": "Director's commentary" } },
        { "index": 3, "codec_type": "subtitle", "codec_name": "subrip",
          "disposition": { "forced": 1 }, "tags": { "language": "fre" } },
        { "index": 4, "codec_type": "subtitle", "codec_name": "hdmv_pgs_subtitle",
          "tags": { "language": "eng", "title": "English SDH" } },
        { "index": 5, "codec_type": "subtitle", "codec_name": "ass", "tags": { "language": "und" } },
        { "index": 6, "codec_type": "video", "codec_name": "mjpeg", "disposition": { "attached_pic": 1 } }
      ],
      "format": { "format_name": "matroska,webm", "duration": "5400.250000",
                  "size": "4000000000", "bit_rate": "5925000" }
    }"#;

    #[test]
    fn parses_streams_and_format() {
        let info = parse(SAMPLE).unwrap();
        assert_eq!(info.duration_ms, Some(5_400_250));
        assert_eq!(info.container.as_deref(), Some("matroska,webm"));
        let video = info.video.unwrap();
        assert_eq!(
            (video.codec.as_str(), video.width, video.height),
            ("hevc", Some(3840), Some(2160))
        );
        assert!(video.hdr);
        assert_eq!(video.bit_depth, Some(10));

        assert_eq!(info.audio.len(), 2);
        assert_eq!(info.audio[0].language.as_deref(), Some("fre"));
        assert_eq!(info.audio[0].kind, TrackKind::Normal);
        assert!(info.audio[0].default);
        assert_eq!(info.audio[1].kind, TrackKind::Commentary);

        assert_eq!(info.subtitles.len(), 3);
        assert_eq!(info.subtitles[0].kind, TrackKind::Forced);
        assert!(info.subtitles[0].text);
        assert_eq!(info.subtitles[1].kind, TrackKind::Sdh);
        assert!(!info.subtitles[1].text, "PGS is an image format");
        assert_eq!(info.subtitles[2].language, None, "und means unknown");
    }

    #[test]
    fn tolerates_minimal_output() {
        let info = parse(r#"{ "streams": [] }"#).unwrap();
        assert_eq!(info.video, None);
        assert_eq!(info.duration_ms, None);
    }
}
