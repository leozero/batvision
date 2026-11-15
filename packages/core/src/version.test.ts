import { describe, expect, it } from "vitest";
import { isNewer, versionGap } from "./version";

describe("isNewer", () => {
  it("compares each number, not the text", () => {
    expect(isNewer("v0.10.0", "0.9.3")).toBe(true);
    expect(isNewer("1.0.0", "0.99.99")).toBe(true);
    expect(isNewer("v0.1.0", "0.1.0")).toBe(false);
    expect(isNewer("0.1.0", "0.2.0")).toBe(false);
  });

  it("ignores pre-release suffixes and missing parts", () => {
    expect(isNewer("0.2.0-beta.1", "0.1.0")).toBe(true);
    expect(isNewer("1.2", "1.2.0")).toBe(false);
  });
});

describe("versionGap", () => {
  it("compares major.minor only", () => {
    expect(versionGap("0.9.0", "0.8.0")).toBe("server-older");
    expect(versionGap("0.8.2", "0.9.0")).toBe("app-older");
    expect(versionGap("0.9.0", "0.9.3")).toBeNull();
    expect(versionGap("v1.0.0", "0.10.1")).toBe("server-older");
  });
});
