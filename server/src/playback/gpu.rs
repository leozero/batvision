//! Video processing on the NVIDIA GPU: decode (NVDEC), scale and tone-map (CUDA)
//! so frames reach NVENC without going through the CPU. Used when the startup probe
//! found these filters working; otherwise the CPU filters apply.

use crate::library::media_info::VideoStream;

/// HDR to SDR on the GPU (BT.2390, the curve Jellyfin uses), honouring Dolby Vision metadata.
pub const TONEMAP: &str =
    "tonemap_cuda=format=yuv420p:p=bt709:t=bt709:m=bt709:tonemap=bt2390:peak=100";

/// What the GPU chain needs to know about the source video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    /// NVDEC decodes it; otherwise the CPU decodes and frames are uploaded.
    pub decode: bool,
    /// More than 8 bits per sample.
    pub deep: bool,
}

impl Source {
    pub fn of(video: &VideoStream) -> Self {
        Self {
            decode: nvdec_decodes(video),
            deep: video.bit_depth.is_some_and(|b| b > 8),
        }
    }
}

/// Codecs and formats every NVDEC generation since Turing handles. Anything else
/// (4:2:2, 4:4:4, 10-bit H.264, old codecs) is decoded on the CPU.
fn nvdec_decodes(video: &VideoStream) -> bool {
    let depth = video.bit_depth.unwrap_or(8);
    let profile = video.profile.as_deref().unwrap_or("");
    let chroma_420 = !(profile.contains("4:2:2") || profile.contains("4:4:4"));
    match video.codec.as_str() {
        "h264" => depth <= 8 && chroma_420,
        "hevc" => depth <= 10 && chroma_420 && !profile.contains("Rext"),
        "av1" => depth <= 10 && (profile.is_empty() || profile == "Main"),
        "vp9" => matches!(profile, "Profile 0" | "Profile 2" | ""),
        "mpeg2video" | "vc1" | "vp8" => depth <= 8,
        _ => false,
    }
}

/// Options placed before `-i`: the CUDA device, and NVDEC when it decodes the source.
pub fn input_args(source: Source) -> Vec<String> {
    let mut args: Vec<String> = ["-init_hw_device", "cuda=cu:0", "-filter_hw_device", "cu"]
        .map(String::from)
        .to_vec();
    if source.decode {
        args.extend(
            [
                "-hwaccel",
                "cuda",
                "-hwaccel_device",
                "cu",
                "-hwaccel_output_format",
                "cuda",
            ]
            .map(String::from),
        );
    }
    args
}

/// Filter chain leaving 8-bit 4:2:0 frames on the GPU, scaled to `size`
/// (`w=…:h=…`, or `None` for the source size) and tone-mapped when `tonemap`.
pub fn chain(source: Source, size: Option<&str>, tonemap: bool) -> String {
    let mut filters = Vec::new();
    if !source.decode {
        let upload = if source.deep { "p010le" } else { "yuv420p" };
        filters.push(format!("format={upload},hwupload_cuda"));
    }
    // The tone mapper reads 10-bit frames and writes the 8-bit output.
    let format = if tonemap { "p010le" } else { "yuv420p" };
    filters.push(match size {
        Some(size) => format!("scale_cuda={size}:format={format}"),
        None => format!("scale_cuda=format={format}"),
    });
    if tonemap {
        filters.push(TONEMAP.to_owned());
    }
    filters.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(codec: &str, profile: &str, depth: i64) -> VideoStream {
        VideoStream {
            index: 0,
            codec: codec.into(),
            profile: Some(profile.into()),
            width: Some(3840),
            height: Some(2160),
            bit_depth: Some(depth),
            hdr: false,
        }
    }

    #[test]
    fn nvdec_takes_common_formats_only() {
        assert!(Source::of(&video("hevc", "Main 10", 10)).decode);
        assert!(Source::of(&video("h264", "High", 8)).decode);
        assert!(Source::of(&video("av1", "Main", 10)).decode);
        assert!(!Source::of(&video("h264", "High 10", 10)).decode);
        assert!(!Source::of(&video("h264", "High 4:2:2", 8)).decode);
        assert!(!Source::of(&video("hevc", "Rext", 10)).decode);
        assert!(!Source::of(&video("mpeg4", "Advanced Simple Profile", 8)).decode);
    }

    #[test]
    fn decoded_hdr_is_scaled_then_tone_mapped_on_the_gpu() {
        let source = Source::of(&video("hevc", "Main 10", 10));
        assert_eq!(
            input_args(source).join(" "),
            "-init_hw_device cuda=cu:0 -filter_hw_device cu \
             -hwaccel cuda -hwaccel_device cu -hwaccel_output_format cuda"
        );
        assert_eq!(
            chain(source, Some("w=-2:h=480"), true),
            format!("scale_cuda=w=-2:h=480:format=p010le,{TONEMAP}")
        );
    }

    #[test]
    fn cpu_decoded_frames_are_uploaded() {
        let source = Source::of(&video("h264", "High 10", 10));
        assert!(!input_args(source).contains(&"-hwaccel".to_owned()));
        assert_eq!(
            chain(source, None, false),
            "format=p010le,hwupload_cuda,scale_cuda=format=yuv420p"
        );
    }
}
