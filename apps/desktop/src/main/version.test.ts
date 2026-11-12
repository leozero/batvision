import { describe, expect, it } from "vitest";
import { isNewer } from "./version";

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
