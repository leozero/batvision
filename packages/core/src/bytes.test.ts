import { describe, expect, it } from "vitest";
import { formatBytes } from "./bytes";

describe("formatBytes", () => {
  it("uses decimal units per locale", () => {
    expect(formatBytes(1_536_000_000, "fr")).toBe("1,5 Go");
    expect(formatBytes(1_536_000_000, "en")).toBe("1.5 GB");
    expect(formatBytes(512, "en")).toBe("512 B");
    expect(formatBytes(50_000_000_000, "en")).toBe("50 GB");
    expect(formatBytes(null, "fr")).toBe("—");
  });
});
