import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Config } from "./config";
import { Engine } from "./engine";
import { PARTIAL } from "./library";
import type { PlanEntry } from "./planner";
import type { ServerClient } from "./server";

vi.mock("electron", () => ({ net: {} }));

const movie: PlanEntry = {
  kind: "movie",
  id: 1,
  profile: "max",
  watched: false,
  status: "ready",
  variant_id: 10,
  progress: 1,
  size_bytes: 8,
  url: "/api/downloads/file/t1",
  subtitles: [],
  title: "Movie",
};

let folder: string;
let engine: Engine;

/** An engine over a temporary folder and a server that answers `file` for the video. */
function setup(file: () => Response) {
  folder = mkdtempSync(join(tmpdir(), "bv-engine-"));
  const calls = { plan: 0, file: 0 };
  const config = { folder, paused: false, av1: null, pendingProgress: [] } as unknown as Config;
  const server = {
    ready: true,
    json: async (path: string) => {
      if (path === "/api/downloads/plan") {
        calls.plan++;
        return { settings: {}, items: [movie] };
      }
      return undefined;
    },
    fetch: async () => {
      calls.file++;
      return file();
    },
  } as unknown as ServerClient;
  engine = new Engine(config, server);
  return calls;
}

const settle = () => new Promise((r) => setTimeout(r, 100));

afterEach(() => {
  engine.stop();
  rmSync(folder, { recursive: true, force: true });
});

describe("a file that answers 404", () => {
  it("gets one fresh plan, then fails until the next sync", async () => {
    const calls = setup(() => new Response("gone", { status: 404 }));
    await engine.sync();
    await settle();
    expect(calls).toEqual({ plan: 2, file: 2 });
    expect(engine.state().items[0]).toMatchObject({ status: "failed", error: "file gone" });

    // The next sync tries again, with one fresh plan again.
    await engine.sync();
    await settle();
    expect(calls).toEqual({ plan: 4, file: 4 });
    expect(engine.state().items[0]?.status).toBe("failed");
  });
});

describe("a connection lost in the middle of a file", () => {
  it("is the server going away, not a failed item", async () => {
    const calls = setup(() => {
      let sent = false;
      const body = new ReadableStream<Uint8Array>({
        pull(controller) {
          if (sent) return controller.error(new TypeError("network error"));
          sent = true;
          controller.enqueue(new Uint8Array(4));
        },
      });
      return new Response(body, { status: 200 });
    });
    await engine.sync();
    await settle();
    expect(calls.file).toBe(1);
    const state = engine.state();
    expect(state.online).toBe(false);
    expect(state.items[0]).toMatchObject({ status: "waiting", error: null });
    // What arrived stays, for the resume.
    expect(existsSync(join(folder, "movie-1", PARTIAL))).toBe(true);
  });
});
