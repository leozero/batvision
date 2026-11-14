import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

const repo = resolve(__dirname, "../../..");
export const SETUP_TOKEN = "DESKTOP-E2E-SETUP-TOKEN-0123";

/** A real server on its own data, with a short real movie (ffmpeg makes it). */
export class TestServer {
  readonly root = mkdtempSync(join(tmpdir(), "batvision-desktop-e2e-"));
  readonly port = 8422;
  readonly origin = `http://127.0.0.1:${this.port}`;
  private child: ChildProcess | null = null;

  constructor() {
    const movie = join(this.root, "media/Movies/Test Movie (2020)/Test Movie (2020).mp4");
    mkdirSync(dirname(movie), { recursive: true });
    execFileSync("ffmpeg", [
      ...["-loglevel", "error", "-y"],
      ...["-f", "lavfi", "-i", "testsrc2=duration=100:size=320x180:rate=10"],
      ...["-f", "lavfi", "-i", "sine=frequency=440:duration=100"],
      ...["-f", "lavfi", "-i", "sine=frequency=880:duration=100"],
      // Two audio tracks: downloads carry both (D40).
      ...["-map", "0", "-map", "1", "-map", "2"],
      ...[
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-metadata:s:a:0",
        "language=fre",
        "-metadata:s:a:1",
        "language=eng",
      ],
      movie,
    ]);
    writeFileSync(
      join(dirname(movie), "Test Movie (2020).fr.srt"),
      "1\n00:00:00,500 --> 00:00:05,000\nBonjour depuis le disque\n",
    );
    execFileSync(
      "cargo",
      ["build", "--quiet", "--manifest-path", join(repo, "server/Cargo.toml")],
      {
        stdio: "inherit",
      },
    );
  }

  async start() {
    this.child = spawn(join(repo, "server/target/debug/batvision-server"), [], {
      env: {
        ...process.env,
        BATVISION_BIND: `127.0.0.1:${this.port}`,
        BATVISION_DATA_DIR: join(this.root, "data"),
        BATVISION_CACHE_DIR: join(this.root, "cache"),
        BATVISION_MEDIA_DIR: join(this.root, "media"),
        BATVISION_SCAN_INTERVAL_MINUTES: "0",
        BATVISION_SECURE_COOKIES: "false",
        BATVISION_SETUP_TOKEN: SETUP_TOKEN,
        RUST_LOG: process.env.RUST_LOG ?? "warn",
      },
      stdio: ["ignore", "inherit", "inherit"],
    });
    for (let i = 0; i < 100; i++) {
      try {
        if ((await fetch(`${this.origin}/api/health`)).ok) return;
      } catch {
        // not up yet
      }
      await new Promise((r) => setTimeout(r, 100));
    }
    throw new Error("server did not start");
  }

  async stop() {
    const child = this.child;
    if (!child || child.exitCode !== null) return;
    const exited = new Promise((r) => child.once("exit", r));
    child.kill("SIGTERM");
    await exited;
    this.child = null;
  }
}
