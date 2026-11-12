import type { Schemas } from "../lib/api";

type Probe = (type: string) => boolean;

/** What this browser can play, to let the server pick direct play or a transcode. */
export function detectCapabilities(
  probe: Probe = defaultProbe,
  screenHeight = typeof window === "undefined"
    ? 1080
    : window.screen.height * (window.devicePixelRatio || 1),
  hdr = typeof window !== "undefined" && window.matchMedia?.("(dynamic-range: high)").matches,
): Schemas["Capabilities"] {
  const video: [string, string][] = [
    ["h264", 'video/mp4; codecs="avc1.640028"'],
    ["hevc", 'video/mp4; codecs="hvc1.1.6.L120.90"'],
    ["av1", 'video/mp4; codecs="av01.0.08M.08"'],
    ["vp9", 'video/webm; codecs="vp9"'],
  ];
  const audio: [string, string][] = [
    ["aac", 'audio/mp4; codecs="mp4a.40.2"'],
    ["mp3", "audio/mpeg"],
    ["opus", 'audio/webm; codecs="opus"'],
    ["flac", 'audio/mp4; codecs="flac"'],
    ["ac3", 'audio/mp4; codecs="ac-3"'],
    ["eac3", 'audio/mp4; codecs="ec-3"'],
  ];
  const containers = ["mp4"];
  if (probe('video/webm; codecs="vp9"') || probe('video/webm; codecs="vp8"'))
    containers.push("webm");

  // Round to a standard height so the server doesn't downscale for nothing.
  const maxHeight = screenHeight > 1440 ? 2160 : screenHeight > 1080 ? 1440 : 1080;

  return {
    containers,
    video_codecs: video.filter(([, type]) => probe(type)).map(([codec]) => codec),
    audio_codecs: audio.filter(([, type]) => probe(type)).map(([codec]) => codec),
    max_height: maxHeight,
    max_audio_channels: 6,
    hdr: Boolean(hdr),
  };
}

function defaultProbe(type: string): boolean {
  const MS = typeof window === "undefined" ? undefined : window.MediaSource;
  if (MS?.isTypeSupported(type)) return true;
  const el = typeof document === "undefined" ? null : document.createElement("video");
  return el?.canPlayType(type) === "probably";
}
