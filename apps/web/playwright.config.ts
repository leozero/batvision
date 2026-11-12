import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { defineConfig } from "@playwright/test";

const port = 8421;
const tmdbPort = 8422;
export const SETUP_TOKEN = "E2E-SETUP-TOKEN-0123456789";

// One fresh server state per run. Set once in the main process; workers inherit it.
process.env.BV_E2E_DIR ??= mkdtempSync(join(tmpdir(), "batvision-e2e-"));
const root = process.env.BV_E2E_DIR;

// A small library of placeholder files (browsing only)…
for (const file of [
  "media/Movies/Ronin (1998)/Ronin (1998).mkv",
  "media/Shows/Dark (2017)/Season 01/Dark - S01E01.mkv",
  "media/Shows/Dark (2017)/Season 01/Dark - S01E02.mkv",
]) {
  const path = join(root, file);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, "placeholder");
}

// …and one real 720p movie for the player (ffmpeg makes it).
const heat = join(root, "media/Movies/Heat (1995)/Heat (1995).mkv");
mkdirSync(dirname(heat), { recursive: true });
execFileSync("ffmpeg", [
  ...["-loglevel", "error", "-y"],
  ...["-f", "lavfi", "-i", "testsrc2=duration=20:size=1280x720:rate=10"],
  ...["-f", "lavfi", "-i", "sine=frequency=440:duration=20"],
  ...[
    "-c:v",
    "libx264",
    "-preset",
    "ultrafast",
    "-b:v",
    "1200k",
    "-g",
    "20",
    "-pix_fmt",
    "yuv420p",
    "-c:a",
    "aac",
  ],
  heat,
]);

/**
 * End-to-end tests run the real Rust server serving the built web app (`pnpm build` first),
 * exactly like the production container does. Tests share one server: run them in order.
 */
export default defineConfig({
  testDir: "e2e",
  fullyParallel: false,
  workers: 1,
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    locale: "fr-FR",
    viewport: { width: 1440, height: 900 },
  },
  webServer: [
    {
      // Metadata and ratings for the placeholder library.
      command: "node e2e/fake-tmdb.mjs",
      url: `http://127.0.0.1:${tmdbPort}/health`,
      reuseExistingServer: false,
      env: { FAKE_TMDB_PORT: String(tmdbPort) },
    },
    {
      command: "cargo run --quiet --manifest-path ../../server/Cargo.toml",
      url: `http://127.0.0.1:${port}/api/health`,
      reuseExistingServer: false,
      timeout: 300_000,
      env: {
        BATVISION_BIND: `127.0.0.1:${port}`,
        BATVISION_WEB_DIR: "dist",
        BATVISION_DATA_DIR: join(root, "data"),
        BATVISION_CACHE_DIR: join(root, "cache"),
        BATVISION_MEDIA_DIR: join(root, "media"),
        BATVISION_SCAN_INTERVAL_MINUTES: "0",
        BATVISION_SECURE_COOKIES: "false",
        BATVISION_SETUP_TOKEN: SETUP_TOKEN,
        BATVISION_TMDB_TOKEN: "e2e-token",
        BATVISION_TMDB_API_URL: `http://127.0.0.1:${tmdbPort}`,
      },
    },
  ],
});
