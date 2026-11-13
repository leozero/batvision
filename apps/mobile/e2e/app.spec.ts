import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { expect, test } from "@playwright/test";
import { SETUP_TOKEN, TestServer } from "../../desktop/e2e/server";
import { totp } from "../../web/e2e/totp";

/**
 * The app on an Android emulator or an iOS simulator against a real server (D64, D91):
 * Playwright drives the scenario, Maestro the app. `PLATFORM=ios` for the simulator.
 * The Android emulator reaches the host at 10.0.2.2, the simulator at 127.0.0.1.
 */
test.describe.configure({ mode: "serial" });

const IOS = process.env.PLATFORM === "ios";
const PLATFORM = IOS ? "ios" : "android";
const APP_ID = IOS ? "fr.batvision.ios" : "fr.batvision.android";
const ADMIN = { username: "admin", password: "mobile-e2e-admin-password" };
const USER = { username: "lea", password: "mobile-e2e-user-password" };
const APP =
  process.env.BATVISION_APP ??
  (IOS
    ? resolve(__dirname, "../ios/build/dd/Build/Products/Release-iphonesimulator/BatVision.app")
    : resolve(__dirname, "../android/app/build/outputs/apk/release/app-release.apk"));
const ADB = process.env.ADB ?? "adb";

let server: TestServer;
let movieId = 0;
let userToken = "";

async function api(path: string, token: string | null, body?: unknown, method?: string) {
  const response = await fetch(`${server.origin}${path}`, {
    method: method ?? (body === undefined ? "GET" : "POST"),
    headers: {
      "content-type": "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  if (!response.ok) throw new Error(`${path}: ${response.status} ${text}`);
  return text ? JSON.parse(text) : null;
}

/**
 * Runs a flow, up to 3 times: the emulator's adb link sometimes drops ("device offline")
 * on busy machines. Flows start by (re)launching the app, so a retry starts clean.
 */
function maestro(flow: string) {
  for (let attempt = 1; ; attempt++) {
    try {
      runFlow(flow);
      return;
    } catch (e) {
      if (attempt === 3) throw e;
      if (!IOS) execFileSync(ADB, ["reconnect", "offline"]);
      execFileSync("sleep", ["5"]);
    }
  }
}

function runFlow(flow: string) {
  if (!IOS) execFileSync(ADB, ["wait-for-device"], { timeout: 60_000 });
  execFileSync(
    process.env.MAESTRO ?? "maestro",
    [
      "test",
      ...["-e", `APP_ID=${APP_ID}`],
      ...["-e", `SERVER=http://${IOS ? "127.0.0.1" : "10.0.2.2"}:${server.port}`],
      ...["-e", `USERNAME=${USER.username}`],
      ...["-e", `PASSWORD=${USER.password}`],
      ...["-e", `MOVIE_ID=${movieId}`],
      ...(process.env.MAESTRO_OUTPUT ? ["--test-output-dir", process.env.MAESTRO_OUTPUT] : []),
      join(__dirname, "flows", flow),
    ],
    { stdio: "inherit" },
  );
}

test.beforeAll(async () => {
  test.setTimeout(600_000);
  expect(existsSync(APP), `build the app first: ${APP}`).toBe(true);
  if (IOS) {
    // A fresh install: the keychain token of an earlier run is dropped at first launch.
    try {
      execFileSync("xcrun", ["simctl", "uninstall", "booted", APP_ID]);
    } catch {
      // not installed yet
    }
    execFileSync("xcrun", ["simctl", "install", "booted", APP], { stdio: "inherit" });
  } else {
    execFileSync(ADB, ["install", "-r", APP], { stdio: "inherit" });
    execFileSync(ADB, [
      "shell",
      "pm",
      "grant",
      "fr.batvision.android",
      "android.permission.POST_NOTIFICATIONS",
    ]);
    // Android's one-time "Viewing full screen" hint would cover the player's controls.
    execFileSync(ADB, [
      "shell",
      "settings",
      "put",
      "secure",
      "immersive_mode_confirmations",
      "confirmed",
    ]);
  }

  server = new TestServer();
  await server.start();
  // An admin with 2FA (required for administration), a library, and a user.
  const admin = await api("/api/setup", null, {
    setup_token: SETUP_TOKEN,
    ...ADMIN,
    device: { name: `${PLATFORM} e2e`, platform: "other" },
  });
  const { secret } = await api("/api/me/totp/setup", admin.token, { password: ADMIN.password });
  await api("/api/me/totp/enable", admin.token, { code: totp(secret) });
  await api("/api/admin/libraries", admin.token, { name: "Films", kind: "movies", path: "Movies" });
  await api("/api/admin/users", admin.token, { ...USER, role: "user" });
  await expect
    .poll(async () => (await api("/api/movies", admin.token)).items.length, { timeout: 30_000 })
    .toBe(1);
  movieId = (await api("/api/movies", admin.token)).items[0].id;
});

test.afterAll(async () => {
  await server?.stop();
});

test("first launch connects and signs in", () => {
  maestro("sign-in.yaml");
});

test("a movie downloads to the device in the background engine", async () => {
  test.setTimeout(400_000);
  const login = await api("/api/auth/login", null, {
    ...USER,
    device: { name: "check", platform: "other" },
  });
  userToken = login.token;
  // CI emulators sometimes come up on mobile data only: let the test device use it,
  // or the download waits for Wi-Fi (D68).
  const before = await api("/api/downloads/overview", userToken);
  const device = before.devices.find((d: { platform: string }) => d.platform === PLATFORM);
  await api(
    `/api/downloads/devices/${device.id}/settings`,
    userToken,
    { ...device.settings, mobile_data: "all" },
    "PUT",
  );
  maestro("download.yaml");
  // The engine confirmed it: the server lists it as downloaded on the device.
  const overview = await api("/api/downloads/overview", userToken);
  const phone = overview.devices.find((d: { platform: string }) => d.platform === PLATFORM);
  expect(phone.items[0].status).toBe("downloaded");
});

test("without the server, the offline library plays the local copy", async () => {
  test.setTimeout(200_000);
  await server.stop();
  maestro("offline.yaml");
});

test("offline progress reaches the server when it's back", async () => {
  test.setTimeout(200_000);
  await server.start();
  maestro("reopen.yaml");
  await expect
    .poll(async () => (await api(`/api/movies/${movieId}`, userToken)).position_ms, {
      timeout: 60_000,
    })
    .toBeGreaterThan(60_000);
});

test("the watchlist tab and the settings sheets reach the server", async () => {
  test.setTimeout(200_000);
  maestro("watchlist.yaml");
  const overview = await api("/api/downloads/overview", userToken);
  const phone = overview.devices.find((d: { platform: string }) => d.platform === PLATFORM);
  expect(phone.settings.auto_download).toBe(true);
  expect(phone.settings.mobile_data).toBe("light");
  expect(phone.settings.mobile_cap_bytes).toBe(5_000_000_000);
  expect((await api("/api/me/preferences", userToken)).watchlist_episodes).toBe(3);
  expect(overview.watchlist.map((w: { id: number }) => w.id)).toEqual([movieId]);
});

test("deleting the movie on the device removes what asked for it", async () => {
  test.setTimeout(200_000);
  maestro("cancel.yaml");
  const overview = await api("/api/downloads/overview", userToken);
  const phone = overview.devices.find((d: { platform: string }) => d.platform === PLATFORM);
  expect(phone.manual).toEqual([]);
  expect(overview.watchlist).toEqual([]);
  expect(phone.items).toEqual([]);
});
