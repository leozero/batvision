//! Playback decisions: which tracks to play, and whether the client can play the
//! file as is or needs a remux or a transcode. Everything here is pure.

pub mod gpu;
pub mod signing;
pub mod subtitles;
pub mod transcode;
pub mod trickplay;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::library::media_info::{MediaInfo, TrackKind};

// ---- Languages ---------------------------------------------------------------------------

/// Canonical ISO 639-2/B code for a language tag found in files or settings:
/// `fra`/`fr`/`fre` → `fre`, `deu`/`de` → `ger`…  Unknown 3-letter codes pass through.
pub fn normalize_language(tag: &str) -> Option<String> {
    let tag = tag.trim().to_ascii_lowercase();
    let base = tag.split(['-', '_']).next().unwrap_or_default();
    let code = match base {
        "fr" | "fra" | "fre" => "fre",
        "en" | "eng" => "eng",
        "de" | "deu" | "ger" => "ger",
        "es" | "spa" => "spa",
        "it" | "ita" => "ita",
        "pt" | "por" => "por",
        "nl" | "nld" | "dut" => "dut",
        "ja" | "jpn" => "jpn",
        "ko" | "kor" => "kor",
        "zh" | "zho" | "chi" => "chi",
        "ru" | "rus" => "rus",
        "pl" | "pol" => "pol",
        "sv" | "swe" => "swe",
        "da" | "dan" => "dan",
        "no" | "nor" | "nob" | "nb" => "nor",
        "fi" | "fin" => "fin",
        "cs" | "ces" | "cze" => "cze",
        "el" | "ell" | "gre" => "gre",
        "ar" | "ara" => "ara",
        "he" | "heb" => "heb",
        "tr" | "tur" => "tur",
        "hu" | "hun" => "hun",
        "ro" | "ron" | "rum" => "rum",
        "uk" | "ukr" => "ukr",
        "hi" | "hin" => "hin",
        "und" | "" => return None,
        other if other.len() == 3 && other.chars().all(|c| c.is_ascii_lowercase()) => other,
        _ => return None,
    };
    Some(code.to_owned())
}

fn same_language(a: Option<&str>, b: Option<&str>) -> bool {
    match (
        a.and_then(normalize_language),
        b.and_then(normalize_language),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

// ---- Track selection ---------------------------------------------------------------------

/// A subtitle the user can pick: embedded stream or external file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleOption {
    /// `s<stream index>` for embedded, `x<subtitle file id>` for external.
    pub id: String,
    pub language: Option<String>,
    pub kind: TrackKind,
    /// Text subtitles are sent to the client; image ones are burned into the video.
    pub text: bool,
}

pub fn subtitle_options(info: &MediaInfo, external: &[ExternalSubtitle]) -> Vec<SubtitleOption> {
    let mut options: Vec<SubtitleOption> = external
        .iter()
        .map(|x| SubtitleOption {
            id: format!("x{}", x.id),
            language: x.language.clone(),
            kind: x.kind,
            text: true,
        })
        .collect();
    options.extend(info.subtitles.iter().map(|s| SubtitleOption {
        id: format!("s{}", s.index),
        language: s.language.clone(),
        kind: s.kind,
        text: s.text,
    }));
    options
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSubtitle {
    pub id: i64,
    pub language: Option<String>,
    pub kind: TrackKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum SubtitleMode {
    /// Always show subtitles in the preferred language.
    Always,
    /// Only when the audio isn't in the subtitle language (plus "forced" tracks).
    Foreign,
    Never,
}

/// The user's general preferences, languages already normalized.
#[derive(Debug, Clone, Default)]
pub struct GeneralPreferences {
    pub audio_language: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_mode: Option<SubtitleMode>,
}

/// What the user picked earlier in the same series.
#[derive(Debug, Clone, Default)]
pub struct SeriesPreference {
    pub audio_language: Option<String>,
    pub audio_kind: Option<TrackKind>,
    pub subtitle_language: Option<String>,
    pub subtitle_kind: Option<TrackKind>,
    pub subtitles_off: bool,
}

/// Picks the audio stream index and the subtitle id (`None` = off).
pub fn select_tracks(
    info: &MediaInfo,
    subtitles: &[SubtitleOption],
    general: &GeneralPreferences,
    series: Option<&SeriesPreference>,
) -> (Option<i64>, Option<String>) {
    // Audio: series choice, then general language, then the file's default, then the first.
    let find_audio = |language: Option<&str>, kind: TrackKind| {
        info.audio
            .iter()
            .find(|a| a.kind == kind && same_language(a.language.as_deref(), language))
    };
    let audio = series
        .and_then(|p| {
            find_audio(
                p.audio_language.as_deref(),
                p.audio_kind.unwrap_or(TrackKind::Normal),
            )
        })
        .or_else(|| find_audio(general.audio_language.as_deref(), TrackKind::Normal))
        .or_else(|| {
            info.audio
                .iter()
                .find(|a| a.default && a.kind != TrackKind::Commentary)
        })
        .or_else(|| info.audio.iter().find(|a| a.kind != TrackKind::Commentary))
        .or(info.audio.first());
    let audio_language = audio.and_then(|a| a.language.clone());

    let find_subtitle = |language: Option<&str>, kinds: &[TrackKind]| {
        kinds.iter().find_map(|kind| {
            subtitles
                .iter()
                .find(|s| s.kind == *kind && same_language(s.language.as_deref(), language))
        })
    };

    // Subtitles: the series choice wins when it can be honored.
    if let Some(pref) = series {
        if pref.subtitles_off {
            return (audio.map(|a| a.index), None);
        }
        if let Some(sub) = find_subtitle(
            pref.subtitle_language.as_deref(),
            &[pref.subtitle_kind.unwrap_or(TrackKind::Normal)],
        ) {
            return (audio.map(|a| a.index), Some(sub.id.clone()));
        }
    }

    let wanted = general.subtitle_language.as_deref();
    let subtitle = match general.subtitle_mode.unwrap_or(SubtitleMode::Foreign) {
        SubtitleMode::Never => None,
        SubtitleMode::Always => find_subtitle(wanted, &[TrackKind::Normal, TrackKind::Sdh]),
        SubtitleMode::Foreign => {
            if wanted.is_some() && !same_language(audio_language.as_deref(), wanted) {
                find_subtitle(
                    wanted,
                    &[TrackKind::Normal, TrackKind::Sdh, TrackKind::Forced],
                )
            } else {
                // Same language: only the "forced" track for foreign-language lines.
                find_subtitle(wanted, &[TrackKind::Forced])
            }
        }
    };
    (audio.map(|a| a.index), subtitle.map(|s| s.id.clone()))
}

// ---- Direct play or transcode -------------------------------------------------------------

/// What a client says it can play.
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct Capabilities {
    /// `mp4`, `webm`, `mkv`, `ts`…
    pub containers: Vec<String>,
    /// ffprobe codec names: `h264`, `hevc`, `vp9`, `av1`.
    pub video_codecs: Vec<String>,
    /// ffprobe codec names: `aac`, `mp3`, `opus`, `flac`, `ac3`, `eac3`.
    pub audio_codecs: Vec<String>,
    pub max_height: Option<i64>,
    pub max_audio_channels: Option<i64>,
    /// Can display HDR as is. Otherwise HDR video is tone-mapped to SDR.
    #[serde(default)]
    pub hdr: bool,
    /// Shows subtitles of an HLS stream only from inside the stream (Apple's players):
    /// HLS streams then come with a master playlist that lists the subtitle (D88).
    #[serde(default)]
    pub hls_subtitles: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// The original file, as is.
    Direct,
    /// Same audio and video, new container (HLS).
    Remux,
    /// Video copied, audio converted.
    TranscodeAudio,
    /// Video converted (codec, size, HDR or burned-in subtitles).
    Transcode,
}

/// Picture quality asked by the player (D60). Below `Max`, a source heavier than
/// the level is transcoded down to it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// The best the client can play: the original file when possible.
    #[default]
    Max,
    /// 1080p, 6 Mbit/s.
    Medium,
    /// 720p, 2 Mbit/s, stereo audio.
    Low,
    /// 480p, 1 Mbit/s, stereo audio.
    Sos,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Max, Quality::Medium, Quality::Low, Quality::Sos];

    /// Height and video bitrate (kbit/s) of the level; `None` for `Max`.
    pub fn limit(self) -> Option<(i64, i64)> {
        match self {
            Quality::Max => None,
            Quality::Medium => Some((1080, 6_000)),
            Quality::Low => Some((720, 2_000)),
            Quality::Sos => Some((480, 1_000)),
        }
    }

    /// Stereo AAC bitrate (kbit/s) when the audio is converted.
    pub fn audio_kbps(self) -> i64 {
        match self {
            Quality::Max | Quality::Medium => 192,
            Quality::Low => 128,
            Quality::Sos => 96,
        }
    }

    /// The level forces stereo AAC audio.
    fn converts_audio(self) -> bool {
        matches!(self, Quality::Low | Quality::Sos)
    }
}

/// Room left for the audio when comparing a file's total bitrate to a level's video bitrate.
const AUDIO_ALLOWANCE_KBPS: i64 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub method: Method,
    pub audio_index: Option<i64>,
    /// The audio can be passed through (otherwise converted to stereo AAC).
    pub copy_audio: bool,
    /// Bitrate of the converted audio, kbit/s.
    pub audio_kbps: i64,
    /// Image subtitle stream burned into the video.
    pub burn_subtitle: Option<i64>,
    /// Output height when downscaling.
    pub max_height: Option<i64>,
    /// Video bitrate cap set by the quality level, kbit/s (otherwise from the height).
    pub video_kbps: Option<i64>,
    pub tonemap: bool,
}

impl Plan {
    /// Approximate bitrate of the stream the client receives, kbit/s.
    pub fn stream_kbps(&self, info: &MediaInfo) -> Option<i64> {
        if self.method != Method::Transcode {
            return info.bitrate.map(|b| b / 1000);
        }
        let height = self
            .max_height
            .or(info.video.as_ref().and_then(|v| v.height))
            .unwrap_or(1080);
        let video = self
            .video_kbps
            .unwrap_or_else(|| transcode::max_bitrate_kbps(height));
        let audio = if self.copy_audio {
            AUDIO_ALLOWANCE_KBPS
        } else {
            self.audio_kbps
        };
        Some(video + audio)
    }

    /// Height of the picture the client receives.
    pub fn output_height(&self, info: &MediaInfo) -> Option<i64> {
        self.max_height
            .or(info.video.as_ref().and_then(|v| v.height))
    }
}

/// Short container name from ffprobe's `format_name`.
pub fn container_name(format_name: &str) -> &'static str {
    let first = format_name.split(',').next().unwrap_or_default();
    match first {
        "matroska" => "mkv",
        "mov" => "mp4",
        "mpegts" => "ts",
        "avi" => "avi",
        "webm" => "webm",
        _ => "other",
    }
}

fn has(list: &[String], value: &str) -> bool {
    list.iter().any(|v| v.eq_ignore_ascii_case(value))
}

pub fn decide(
    info: &MediaInfo,
    caps: &Capabilities,
    audio_index: Option<i64>,
    subtitle: Option<&SubtitleOption>,
    quality: Quality,
) -> Plan {
    let burn_subtitle = subtitle
        .filter(|s| !s.text)
        .and_then(|s| s.id.strip_prefix('s'))
        .and_then(|i| i.parse().ok());

    let video = info.video.as_ref();
    let height = video.and_then(|v| v.height);
    let too_tall = height.zip(caps.max_height).is_some_and(|(h, max)| h > max);
    // The level only applies to a source heavier than it.
    let level = quality.limit().filter(|&(max_h, kbps)| {
        video.is_some()
            && (height.is_some_and(|h| h > max_h)
                || info
                    .bitrate
                    .is_some_and(|b| b > (kbps + AUDIO_ALLOWANCE_KBPS) * 1000))
    });
    let tonemap = video.is_some_and(|v| v.hdr) && !caps.hdr;
    let video_ok = video.is_none_or(|v| has(&caps.video_codecs, &v.codec))
        && !too_tall
        && !tonemap
        && burn_subtitle.is_none()
        && level.is_none();

    let audio = audio_index.and_then(|i| info.audio.iter().find(|a| a.index == i));
    let audio_ok = audio.is_none_or(|a| {
        has(&caps.audio_codecs, &a.codec)
            && caps
                .max_audio_channels
                .zip(a.channels)
                .is_none_or(|(max, ch)| ch <= max)
    }) && !(level.is_some() && quality.converts_audio());
    // A browser plays the first audio track of a file: another one needs a remux.
    let audio_is_first =
        audio.is_none_or(|a| info.audio.first().is_some_and(|f| f.index == a.index));
    let container_ok = info
        .container
        .as_deref()
        .is_some_and(|c| has(&caps.containers, container_name(c)));

    let method = if video_ok && audio_ok && audio_is_first && container_ok {
        Method::Direct
    } else if video_ok && audio_ok {
        Method::Remux
    } else if video_ok {
        Method::TranscodeAudio
    } else {
        Method::Transcode
    };
    let max_height = [
        caps.max_height.filter(|_| too_tall),
        level
            .map(|(h, _)| h)
            .filter(|&h| height.is_some_and(|source| source > h)),
    ]
    .into_iter()
    .flatten()
    .min();
    Plan {
        method,
        audio_index: audio.map(|a| a.index),
        copy_audio: audio_ok,
        audio_kbps: quality.audio_kbps(),
        burn_subtitle,
        max_height,
        video_kbps: level.map(|(_, kbps)| kbps),
        tonemap,
    }
}

/// What each quality level gives for this file and client, for the player's menu
/// and its automatic choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct QualityOption {
    pub quality: Quality,
    /// Picture height the client receives.
    pub height: Option<i64>,
    /// Approximate stream bitrate, kbit/s.
    pub bitrate_kbps: Option<i64>,
    /// The original video is sent as is (direct play or remux).
    pub original: bool,
}

pub fn quality_options(
    info: &MediaInfo,
    caps: &Capabilities,
    audio_index: Option<i64>,
    subtitle: Option<&SubtitleOption>,
) -> Vec<QualityOption> {
    Quality::ALL
        .iter()
        .map(|&quality| {
            let plan = decide(info, caps, audio_index, subtitle, quality);
            QualityOption {
                quality,
                height: plan.output_height(info),
                bitrate_kbps: plan.stream_kbps(info),
                original: plan.method != Method::Transcode,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::media_info::{AudioTrack, SubtitleTrack, VideoStream};

    fn audio(index: i64, codec: &str, lang: &str, kind: TrackKind, default: bool) -> AudioTrack {
        AudioTrack {
            index,
            codec: codec.into(),
            language: Some(lang.into()),
            title: None,
            channels: Some(2),
            default,
            kind,
        }
    }

    fn sub(index: i64, codec: &str, lang: &str, kind: TrackKind, text: bool) -> SubtitleTrack {
        SubtitleTrack {
            index,
            codec: codec.into(),
            language: Some(lang.into()),
            title: None,
            default: false,
            kind,
            text,
        }
    }

    fn info() -> MediaInfo {
        MediaInfo {
            duration_ms: Some(3_600_000),
            container: Some("matroska,webm".into()),
            size_bytes: None,
            bitrate: None,
            video: Some(VideoStream {
                index: 0,
                codec: "h264".into(),
                profile: None,
                width: Some(1920),
                height: Some(1080),
                bit_depth: Some(8),
                hdr: false,
            }),
            audio: vec![
                audio(1, "eac3", "eng", TrackKind::Normal, true),
                audio(2, "aac", "fre", TrackKind::Normal, false),
                audio(3, "aac", "eng", TrackKind::Commentary, false),
            ],
            subtitles: vec![
                sub(4, "subrip", "fre", TrackKind::Forced, true),
                sub(5, "subrip", "fre", TrackKind::Normal, true),
                sub(6, "hdmv_pgs_subtitle", "eng", TrackKind::Sdh, false),
            ],
        }
    }

    fn browser() -> Capabilities {
        Capabilities {
            containers: vec!["mp4".into(), "webm".into()],
            video_codecs: vec!["h264".into(), "vp9".into(), "av1".into()],
            audio_codecs: vec!["aac".into(), "mp3".into(), "opus".into()],
            max_height: Some(2160),
            max_audio_channels: Some(6),
            hdr: false,
            hls_subtitles: false,
        }
    }

    fn prefs(audio: Option<&str>, sub: Option<&str>, mode: SubtitleMode) -> GeneralPreferences {
        GeneralPreferences {
            audio_language: audio.map(Into::into),
            subtitle_language: sub.map(Into::into),
            subtitle_mode: Some(mode),
        }
    }

    #[test]
    fn normalizes_language_codes() {
        assert_eq!(normalize_language("fra").as_deref(), Some("fre"));
        assert_eq!(normalize_language("fr-FR").as_deref(), Some("fre"));
        assert_eq!(normalize_language("DEU").as_deref(), Some("ger"));
        assert_eq!(normalize_language("und"), None);
        assert_eq!(normalize_language("tlh").as_deref(), Some("tlh"));
        assert_eq!(normalize_language("x"), None);
    }

    #[test]
    fn audio_follows_language_preference_then_file_default() {
        let i = info();
        let subs = subtitle_options(&i, &[]);
        let (a, _) = select_tracks(
            &i,
            &subs,
            &prefs(Some("fre"), None, SubtitleMode::Never),
            None,
        );
        assert_eq!(a, Some(2));
        let (a, _) = select_tracks(&i, &subs, &prefs(None, None, SubtitleMode::Never), None);
        assert_eq!(a, Some(1), "file default");
        let (a, _) = select_tracks(
            &i,
            &subs,
            &prefs(Some("jpn"), None, SubtitleMode::Never),
            None,
        );
        assert_eq!(a, Some(1), "missing language falls back to the default");
    }

    #[test]
    fn foreign_mode_shows_full_subtitles_only_for_foreign_audio() {
        let i = info();
        let subs = subtitle_options(&i, &[]);
        // Original English audio, French subtitles wanted.
        let (a, s) = select_tracks(
            &i,
            &subs,
            &prefs(None, Some("fre"), SubtitleMode::Foreign),
            None,
        );
        assert_eq!((a, s.as_deref()), (Some(1), Some("s5")));
        // French audio: only the forced track.
        let (a, s) = select_tracks(
            &i,
            &subs,
            &prefs(Some("fre"), Some("fre"), SubtitleMode::Foreign),
            None,
        );
        assert_eq!((a, s.as_deref()), (Some(2), Some("s4")));
    }

    #[test]
    fn always_and_never_modes() {
        let i = info();
        let subs = subtitle_options(&i, &[]);
        let (_, s) = select_tracks(
            &i,
            &subs,
            &prefs(Some("fre"), Some("fre"), SubtitleMode::Always),
            None,
        );
        assert_eq!(s.as_deref(), Some("s5"));
        let (_, s) = select_tracks(
            &i,
            &subs,
            &prefs(None, Some("fre"), SubtitleMode::Never),
            None,
        );
        assert_eq!(s, None);
    }

    #[test]
    fn series_choice_is_matched_by_language_and_kind_not_index() {
        let i = info();
        let subs = subtitle_options(&i, &[]);
        let series = SeriesPreference {
            audio_language: Some("eng".into()),
            audio_kind: Some(TrackKind::Commentary),
            subtitle_language: Some("eng".into()),
            subtitle_kind: Some(TrackKind::Sdh),
            subtitles_off: false,
        };
        let (a, s) = select_tracks(
            &i,
            &subs,
            &prefs(Some("fre"), Some("fre"), SubtitleMode::Always),
            Some(&series),
        );
        assert_eq!((a, s.as_deref()), (Some(3), Some("s6")));
    }

    #[test]
    fn series_choice_falls_back_when_the_episode_lacks_the_track() {
        let i = info();
        let subs = subtitle_options(&i, &[]);
        let series = SeriesPreference {
            audio_language: Some("ita".into()),
            subtitle_language: Some("ita".into()),
            ..Default::default()
        };
        let (a, s) = select_tracks(
            &i,
            &subs,
            &prefs(Some("fre"), Some("fre"), SubtitleMode::Always),
            Some(&series),
        );
        assert_eq!((a, s.as_deref()), (Some(2), Some("s5")));
        let off = SeriesPreference {
            subtitles_off: true,
            ..Default::default()
        };
        let (_, s) = select_tracks(
            &i,
            &subs,
            &prefs(None, Some("fre"), SubtitleMode::Always),
            Some(&off),
        );
        assert_eq!(s, None);
    }

    #[test]
    fn external_subtitles_are_offered_first() {
        let i = info();
        let external = [ExternalSubtitle {
            id: 9,
            language: Some("fre".into()),
            kind: TrackKind::Normal,
        }];
        let subs = subtitle_options(&i, &external);
        assert_eq!(subs[0].id, "x9");
        let (_, s) = select_tracks(
            &i,
            &subs,
            &prefs(None, Some("fre"), SubtitleMode::Always),
            None,
        );
        assert_eq!(s.as_deref(), Some("x9"));
    }

    #[test]
    fn mp4_with_supported_codecs_plays_directly() {
        let mut i = info();
        i.container = Some("mov,mp4,m4a,3gp,3g2,mj2".into());
        i.audio = vec![audio(1, "aac", "eng", TrackKind::Normal, true)];
        let plan = decide(&i, &browser(), Some(1), None, Quality::Max);
        assert_eq!(plan.method, Method::Direct);
    }

    #[test]
    fn mkv_is_remuxed_for_browsers() {
        let mut i = info();
        i.audio = vec![audio(1, "aac", "eng", TrackKind::Normal, true)];
        assert_eq!(
            decide(&i, &browser(), Some(1), None, Quality::Max).method,
            Method::Remux
        );
        let mut mkv_player = browser();
        mkv_player.containers.push("mkv".into());
        assert_eq!(
            decide(&i, &mkv_player, Some(1), None, Quality::Max).method,
            Method::Direct
        );
    }

    #[test]
    fn unsupported_audio_is_transcoded_alone() {
        let plan = decide(&info(), &browser(), Some(1), None, Quality::Max);
        assert_eq!(plan.method, Method::TranscodeAudio, "eac3 not supported");
        assert_eq!(plan.audio_index, Some(1));
    }

    #[test]
    fn second_audio_track_needs_a_remux() {
        let mut i = info();
        i.container = Some("mov,mp4".into());
        i.audio = vec![
            audio(1, "aac", "eng", TrackKind::Normal, true),
            audio(2, "aac", "fre", TrackKind::Normal, false),
        ];
        assert_eq!(
            decide(&i, &browser(), Some(1), None, Quality::Max).method,
            Method::Direct
        );
        assert_eq!(
            decide(&i, &browser(), Some(2), None, Quality::Max).method,
            Method::Remux
        );
    }

    #[test]
    fn video_transcode_cases() {
        let i = info();
        let mut no_h264 = browser();
        no_h264.video_codecs = vec!["vp9".into()];
        assert_eq!(
            decide(&i, &no_h264, Some(2), None, Quality::Max).method,
            Method::Transcode
        );

        let mut small = browser();
        small.max_height = Some(720);
        let plan = decide(&i, &small, Some(2), None, Quality::Max);
        assert_eq!(
            (plan.method, plan.max_height),
            (Method::Transcode, Some(720))
        );

        let mut hdr = info();
        hdr.video.as_mut().unwrap().hdr = true;
        let plan = decide(&hdr, &browser(), Some(2), None, Quality::Max);
        assert!(plan.tonemap);
        assert_eq!(plan.method, Method::Transcode);
    }

    #[test]
    fn image_subtitles_are_burned_text_ones_are_not() {
        let i = info();
        let subs = subtitle_options(&i, &[]);
        let pgs = subs.iter().find(|s| s.id == "s6").unwrap();
        let plan = decide(&i, &browser(), Some(2), Some(pgs), Quality::Max);
        assert_eq!(
            (plan.method, plan.burn_subtitle),
            (Method::Transcode, Some(6))
        );
        let srt = subs.iter().find(|s| s.id == "s5").unwrap();
        assert_eq!(
            decide(&i, &browser(), Some(2), Some(srt), Quality::Max).burn_subtitle,
            None
        );
    }

    #[test]
    fn lower_quality_transcodes_a_heavier_source() {
        let mut i = info();
        i.container = Some("mov,mp4".into());
        i.bitrate = Some(12_000_000);
        i.audio = vec![audio(1, "aac", "eng", TrackKind::Normal, true)];
        assert_eq!(
            decide(&i, &browser(), Some(1), None, Quality::Max).method,
            Method::Direct
        );

        // Same height, but a heavier bitrate: transcoded at the level's bitrate.
        let plan = decide(&i, &browser(), Some(1), None, Quality::Medium);
        assert_eq!(
            (
                plan.method,
                plan.max_height,
                plan.video_kbps,
                plan.copy_audio
            ),
            (Method::Transcode, None, Some(6_000), true)
        );

        let plan = decide(&i, &browser(), Some(1), None, Quality::Low);
        assert_eq!(
            (
                plan.max_height,
                plan.video_kbps,
                plan.copy_audio,
                plan.audio_kbps
            ),
            (Some(720), Some(2_000), false, 128)
        );
        let plan = decide(&i, &browser(), Some(1), None, Quality::Sos);
        assert_eq!(
            (plan.max_height, plan.video_kbps, plan.audio_kbps),
            (Some(480), Some(1_000), 96)
        );
    }

    #[test]
    fn a_light_source_keeps_its_method_at_any_level() {
        let mut i = info();
        i.container = Some("mov,mp4".into());
        i.bitrate = Some(1_200_000);
        i.video.as_mut().unwrap().height = Some(480);
        i.audio = vec![audio(1, "aac", "eng", TrackKind::Normal, true)];
        for quality in Quality::ALL {
            let plan = decide(&i, &browser(), Some(1), None, quality);
            assert_eq!(
                (plan.method, plan.video_kbps),
                (Method::Direct, None),
                "{quality:?}"
            );
        }
    }

    #[test]
    fn the_smaller_of_screen_and_level_heights_wins() {
        let mut i = info();
        i.video.as_mut().unwrap().height = Some(2160);
        let mut small = browser();
        small.max_height = Some(720);
        let plan = decide(&i, &small, Some(2), None, Quality::Medium);
        assert_eq!(plan.max_height, Some(720));
        let plan = decide(&i, &browser(), Some(2), None, Quality::Medium);
        assert_eq!(plan.max_height, Some(1080));
    }

    #[test]
    fn quality_options_describe_each_level() {
        let mut i = info();
        i.container = Some("mov,mp4".into());
        i.bitrate = Some(10_000_000);
        i.audio = vec![audio(1, "aac", "eng", TrackKind::Normal, true)];
        let options = quality_options(&i, &browser(), Some(1), None);
        let summary: Vec<_> = options
            .iter()
            .map(|o| (o.quality, o.height, o.bitrate_kbps, o.original))
            .collect();
        assert_eq!(
            summary,
            vec![
                (Quality::Max, Some(1080), Some(10_000), true),
                (Quality::Medium, Some(1080), Some(6_500), false),
                (Quality::Low, Some(720), Some(2_128), false),
                (Quality::Sos, Some(480), Some(1_096), false),
            ]
        );
    }

    #[test]
    fn too_many_channels_means_audio_transcode() {
        let mut i = info();
        i.container = Some("mov,mp4".into());
        i.audio = vec![AudioTrack {
            channels: Some(8),
            ..audio(1, "aac", "eng", TrackKind::Normal, true)
        }];
        assert_eq!(
            decide(&i, &browser(), Some(1), None, Quality::Max).method,
            Method::TranscodeAudio
        );
    }
}
