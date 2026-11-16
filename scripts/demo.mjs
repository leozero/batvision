// `pnpm demo`: the server and the web app on a small made-up library, with a stand-in for
// TMDB (the e2e one), so the app can be tried without media, a TMDB token or a network.
// Starts from a fresh state each time, in .dev/demo. Ctrl+C stops everything.
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { totp } from "../apps/web/e2e/totp.ts";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(repo, ".dev/demo");
const port = Number(process.env.DEMO_PORT ?? 8420);
const tmdbPort = Number(process.env.DEMO_TMDB_PORT ?? 8422);
const base = `http://127.0.0.1:${port}`;
const setupToken = "DEMO-SETUP-TOKEN-0123456789";
const admin = { username: "admin", password: "demo-admin-password" };
const user = { username: "demo", password: "demo-password" };

rmSync(root, { recursive: true, force: true });
// Placeholder files: browsing works, playback doesn't.
for (const file of [
  "media/Movies/Heat (1995)/Heat (1995).mkv",
  "media/Movies/Ronin (1998)/Ronin (1998).mkv",
  "media/Shows/Dark (2017)/Season 01/Dark - S01E01.mkv",
  "media/Shows/Dark (2017)/Season 01/Dark - S01E02.mkv",
]) {
  mkdirSync(dirname(join(root, file)), { recursive: true });
  writeFileSync(join(root, file), "placeholder");
}

const build = spawnSync("pnpm", ["--filter", "@batvision/web", "build"], {
  cwd: repo,
  stdio: "inherit",
});
if (build.status !== 0) process.exit(build.status ?? 1);

const children = [
  spawn("node", ["apps/web/e2e/fake-tmdb.mjs"], {
    cwd: repo,
    stdio: "inherit",
    env: { ...process.env, FAKE_TMDB_PORT: String(tmdbPort) },
  }),
  spawn("cargo", ["run", "--quiet", "--manifest-path", "server/Cargo.toml"], {
    cwd: repo,
    stdio: "inherit",
    env: {
      ...process.env,
      BATVISION_BIND: `127.0.0.1:${port}`,
      BATVISION_WEB_DIR: join(repo, "apps/web/dist"),
      BATVISION_DATA_DIR: join(root, "data"),
      BATVISION_CACHE_DIR: join(root, "cache"),
      BATVISION_MEDIA_DIR: join(root, "media"),
      BATVISION_SCAN_INTERVAL_MINUTES: "0",
      BATVISION_SECURE_COOKIES: "false",
      BATVISION_SETUP_TOKEN: setupToken,
      BATVISION_TMDB_TOKEN: "demo",
      BATVISION_TMDB_API_URL: `http://127.0.0.1:${tmdbPort}`,
      RUST_LOG: process.env.RUST_LOG ?? "warn",
    },
  }),
];
const stop = (code) => {
  for (const child of children) child.kill();
  process.exit(code);
};
process.on("SIGINT", () => stop(0));
process.on("SIGTERM", () => stop(0));
for (const child of children) child.on("exit", (code) => stop(code ?? 1));

async function post(path, body, token) {
  const headers = { "content-type": "application/json" };
  if (token) headers.authorization = `Bearer ${token}`;
  const res = await fetch(base + path, { method: "POST", headers, body: JSON.stringify(body) });
  const text = await res.text();
  if (!res.ok) throw new Error(`${path}: ${res.status} ${text}`);
  return text ? JSON.parse(text) : null;
}

// The first build of the server can take a few minutes.
for (;;) {
  try {
    if ((await fetch(`${base}/api/health`)).ok) break;
  } catch {}
  await new Promise((resolve) => setTimeout(resolve, 1000));
}
try {
  const device = { name: "Demo script", platform: "other" };
  const { token } = await post("/api/setup", { setup_token: setupToken, ...admin, device });
  // Admin features need 2FA (D17).
  const { secret } = await post("/api/me/totp/setup", { password: admin.password }, token);
  await post("/api/me/totp/enable", { code: totp(secret) }, token);
  await post("/api/admin/libraries", { name: "Films", kind: "movies", path: "Movies" }, token);
  await post("/api/admin/libraries", { name: "Séries", kind: "series", path: "Shows" }, token);
  await post("/api/admin/users", { ...user, role: "user" }, token);
  console.log(`
BatVision demo on ${base}
  Sign in as ${user.username} / ${user.password}
  (admin: ${admin.username} / ${admin.password}, 2FA secret ${secret})
Ctrl+C to stop.
`);
} catch (err) {
  console.error(err);
  stop(1);
}
