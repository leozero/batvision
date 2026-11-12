import { describe, expect, it } from "vitest";
import { detectCapabilities } from "./capabilities";

describe("detectCapabilities", () => {
  it("lists only what the probe accepts", () => {
    const supported = new Set([
      'video/mp4; codecs="avc1.640028"',
      'audio/mp4; codecs="mp4a.40.2"',
      'video/webm; codecs="vp9"',
    ]);
    const caps = detectCapabilities((t) => supported.has(t), 1080, false);
    expect(caps.video_codecs).toEqual(["h264", "vp9"]);
    expect(caps.audio_codecs).toEqual(["aac"]);
    expect(caps.containers).toEqual(["mp4", "webm"]);
    expect(caps.max_height).toBe(1080);
    expect(caps.hdr).toBe(false);
  });

  it("rounds the screen height to a standard size", () => {
    expect(detectCapabilities(() => true, 2880, true).max_height).toBe(2160);
    expect(detectCapabilities(() => true, 1600, false).max_height).toBe(2160);
    expect(detectCapabilities(() => true, 1440, false).max_height).toBe(1440);
    expect(detectCapabilities(() => true, 900, false).max_height).toBe(1080);
  });
});
