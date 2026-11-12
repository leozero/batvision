import { describe, expect, it } from "vitest";
import { decide, type LocalRecord, type PlanEntry } from "./planner";

const GB = 1_000_000_000;

function entry(id: number, status: string, extra: Partial<PlanEntry> = {}): PlanEntry {
  return {
    kind: "episode",
    id,
    profile: "max",
    audio_language: null,
    watched: false,
    status,
    progress: 0,
    subtitles: [],
    title: "Show",
    variant_id: status === "not_requested" ? null : id * 10,
    size_bytes: status === "ready" ? GB : null,
    url: status === "ready" ? `/api/downloads/file/t${id}` : null,
    ...extra,
  };
}

function local(id: number, extra: Partial<LocalRecord> = {}): [string, LocalRecord] {
  const key = `episode-${id}`;
  return [
    key,
    {
      key,
      complete: true,
      variantId: id * 10,
      profile: "max",
      audioLanguage: null,
      sizeBytes: GB,
      ...extra,
    },
  ];
}

describe("decide", () => {
  it("requests, waits for and downloads expected items in order", () => {
    const d = decide(
      [entry(1, "ready"), entry(2, "encoding"), entry(3, "not_requested")],
      new Map(),
      null,
    );
    expect(d.actions.map((a) => `${a.type}:${a.key}`)).toEqual([
      "request:episode-3",
      "download:episode-1",
    ]);
    expect(d.remove).toEqual([]);
  });

  it("keeps what it has and deletes what is no longer expected", () => {
    const d = decide([entry(1, "downloaded")], new Map([local(1), local(2)]), null);
    expect(d.actions).toEqual([]);
    expect(d.remove).toEqual(["episode-2"]);
    expect(d.usedBytes).toBe(GB);
  });

  it("keeps the longest prefix that fits the quota", () => {
    const d = decide(
      [entry(1, "downloaded"), entry(2, "ready"), entry(3, "downloaded")],
      new Map([local(1), local(3)]),
      2.5 * GB,
    );
    // 1 and 2 fit (2 GB); 3 would make 3 GB: deleted, even though it's on disk.
    expect(d.actions.map((a) => a.key)).toEqual(["episode-2"]);
    expect(d.remove).toEqual(["episode-3"]);
    expect([...d.overQuota]).toEqual(["episode-3"]);
  });

  it("stops at the first item that doesn't fit, even if a later one would", () => {
    const d = decide(
      [entry(1, "ready", { size_bytes: 3 * GB }), entry(2, "ready")],
      new Map(),
      2 * GB,
    );
    expect(d.actions).toEqual([]);
    expect([...d.overQuota]).toEqual(["episode-1", "episode-2"]);
  });

  it("requests items of unknown size only while there is room", () => {
    const d = decide([entry(1, "downloaded"), entry(2, "not_requested")], new Map([local(1)]), GB);
    expect(d.actions).toEqual([]);
  });

  it("requests again an item confirmed earlier but missing on disk", () => {
    const d = decide([entry(1, "downloaded")], new Map(), null);
    expect(d.actions).toEqual([{ type: "request", key: "episode-1", kind: "episode", id: 1 }]);
  });

  it("confirms a complete copy the server still offers", () => {
    const d = decide([entry(1, "ready")], new Map([local(1)]), null);
    expect(d.actions).toEqual([{ type: "confirm", key: "episode-1", variantId: 10 }]);
  });

  it("replaces a copy made with another profile", () => {
    const d = decide(
      [entry(1, "ready", { profile: "mobile" })],
      new Map([local(1, { profile: "max" })]),
      null,
    );
    expect(d.remove).toEqual(["episode-1"]);
    expect(d.actions.map((a) => a.type)).toEqual(["download"]);
  });

  it("drops a partial file from another variant, keeps one from the same", () => {
    const same = decide(
      [entry(1, "ready")],
      new Map([local(1, { complete: false, sizeBytes: 100 })]),
      null,
    );
    expect(same.remove).toEqual([]);
    const other = decide(
      [entry(1, "ready")],
      new Map([local(1, { complete: false, variantId: 99, sizeBytes: 100 })]),
      null,
    );
    expect(other.remove).toEqual(["episode-1"]);
  });

  it("ignores failed and unavailable items", () => {
    const d = decide([entry(1, "failed"), entry(2, "unavailable")], new Map(), null);
    expect(d.actions).toEqual([]);
  });
});
