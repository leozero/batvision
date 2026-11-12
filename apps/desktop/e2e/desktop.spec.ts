import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
  type ElectronApplication,
  _electron as electron,
  expect,
  type Page,
  test,
} from "@playwright/test";
import { totp } from "../../web/e2e/totp";
import { SETUP_TOKEN, TestServer } from "./server";

const ADMIN = { username: "admin", password: "desktop-e2e-password" };

// One app and one server for the whole scenario: each step builds on the previous one.
test.describe.configure({ mode: "serial" });

let server: TestServer;
let app: ElectronApplication;
let page: Page;
let totpSecret = "";

/** Screenshots of the key screens, when `BATVISION_SHOTS` names a folder. */
async function shot(name: string) {
  const dir = process.env.BATVISION_SHOTS;
  if (!dir) return;
  await page.waitForTimeout(800); // let page transitions finish
  await page.screenshot({ path: join(dir, `${name}.png`) });
}
const data = mkdtempSync(join(tmpdir(), "batvision-desktop-app-"));
const videos = join(data, "videos");

test.beforeAll(async () => {
  server = new TestServer();
  await server.start();
  app = await electron.launch({
    args: [
      resolve(__dirname, ".."),
      // The assertions use the French interface, whatever the machine's language.
      "--lang=fr-FR",
      ...(process.env.CI && process.platform === "linux" ? ["--no-sandbox"] : []),
    ],
    env: {
      ...process.env,
      BATVISION_USER_DATA: join(data, "user-data"),
      BATVISION_VIDEOS_DIR: videos,
      BATVISION_SYNC_WAITING_MS: "1000",
    },
  });
  page = await app.firstWindow();
  await page.setViewportSize({ width: 1400, height: 900 });
});

test.afterAll(async () => {
  await app?.close();
  await server?.stop();
});

test("first launch asks for the server address", async () => {
  await expect(page).toHaveURL(/\/connect$/);
  await page.getByLabel("Adresse du serveur").fill("http://127.0.0.1:1");
  await page.getByRole("button", { name: "Continuer" }).click();
  await expect(page.getByText("Aucun serveur ne répond à cette adresse.")).toBeVisible();

  await page.getByLabel("Adresse du serveur").fill(server.origin);
  await page.getByRole("button", { name: "Continuer" }).click();
  await expect(page).toHaveURL(/\/setup$/);
});

test("creating the admin signs the app in with a device token", async () => {
  await page.getByLabel("Jeton d'installation").fill(SETUP_TOKEN);
  await page.getByLabel("Identifiant").fill(ADMIN.username);
  await page.getByLabel("Mot de passe", { exact: true }).fill(ADMIN.password);
  await page.getByLabel("Confirmer le mot de passe").fill(ADMIN.password);
  await page.getByRole("button", { name: "Créer le compte" }).click();
  await expect(page).toHaveURL(/\/account/);

  // The token stays in the main process, encrypted; the page has no cookie.
  expect(await page.evaluate(() => document.cookie)).toBe("");
  const config = JSON.parse(readFileSync(join(data, "user-data/config.json"), "utf8"));
  expect(config.token).toBeTruthy();
  expect(JSON.stringify(config)).not.toContain(ADMIN.password);
});

test("the admin enables 2FA (required for administration)", async () => {
  await page.getByRole("button", { name: "Activer la double authentification" }).click();
  await page.getByLabel("Confirmez votre mot de passe").fill(ADMIN.password);
  await page.getByRole("button", { name: "Continuer" }).click();
  totpSecret = (await page.locator("code").first().textContent())?.trim() ?? "";
  await page.getByLabel("Code").fill(totp(totpSecret));
  await page.getByRole("button", { name: "Activer", exact: true }).click();
  await expect(page.getByText("Active", { exact: true })).toBeVisible();
});

test("the admin adds a library", async () => {
  await page.goto("app://batvision/admin?tab=libraries");
  await expect(page.getByText("Aucune bibliothèque.")).toBeVisible();
  await page.getByLabel("Nom").fill("Films");
  await page.getByLabel("Dossier").fill("Movies");
  await page.getByLabel("Type").selectOption("movies");
  await page.getByRole("button", { name: "Ajouter une bibliothèque" }).click();
  await expect(page.getByText("Aucune bibliothèque.")).toBeHidden();
  await page.getByRole("link", { name: "Films" }).click();
  // The scan runs in the background: reload until the movie shows up.
  await expect(async () => {
    await page.reload();
    await expect(page.getByRole("link", { name: /Test Movie/ })).toBeVisible({ timeout: 2000 });
  }).toPass({ timeout: 30_000 });
});

test("downloading a movie encodes, downloads and marks it", async () => {
  await page.getByRole("link", { name: /Test Movie/ }).click();
  await page.getByRole("button", { name: "Télécharger" }).click();
  const dialog = page.getByRole("dialog");
  // This computer is offered as the target device.
  await expect(dialog.locator("select").first()).toContainText("·");
  await dialog.getByRole("button", { name: "Télécharger" }).click();

  await expect(page.getByText("Téléchargé", { exact: true })).toBeVisible({ timeout: 120_000 });
  await page.keyboard.press("Escape");
  await shot("movie");
  const folder = join(videos, "Batvision");
  const item = JSON.parse(readFileSync(join(folder, "batvision.json"), "utf8")).items[0];
  expect(item.complete).toBe(true);
  expect(existsSync(join(folder, item.key, "video.mp4"))).toBe(true);
  expect(item.subtitles).toHaveLength(1);
});

test("the downloads page shows what this computer holds", async () => {
  await page.getByRole("link", { name: "Téléchargements" }).click();
  const card = page.locator("section", { hasText: "Sur cet ordinateur" });
  await expect(card.getByText("Test Movie")).toBeVisible();
  await expect(card.getByText("Téléchargé")).toBeVisible();
  await shot("downloads");
});

test("without the server, the offline library plays the local copy", async () => {
  await server.stop();
  await page.goto("app://batvision/");
  await expect(page).toHaveURL(/\/offline$/);
  await expect(page.getByText("Hors ligne", { exact: true })).toBeVisible();
  await shot("offline");

  await page.getByRole("link", { name: /Test Movie/ }).click();
  await expect(page).toHaveURL(/\/offline\/watch\/movie\/\d+$/);
  const video = page.locator("video");
  await expect(video).toHaveAttribute("src", /^\/local\/movie-\d+\/video\.mp4$/);
  await expect
    .poll(() => video.evaluate((v: HTMLVideoElement) => v.currentTime), { timeout: 15_000 })
    .toBeGreaterThan(1.5);
  await shot("offline-player");
  await video.evaluate((v: HTMLVideoElement) => {
    v.pause();
    v.currentTime = 70;
  });
  // Resume positions start at 60 s (D36): 70 s is kept.
  await page.waitForTimeout(300);
  await page.goBack();
  await expect(page).toHaveURL(/\/offline$/);
});

test("progress made offline reaches the server when it's back", async () => {
  await server.start();
  await page.getByRole("button", { name: "Réessayer" }).click();
  await expect(page.getByRole("link", { name: "Accueil" })).toBeVisible();

  const login = await fetch(`${server.origin}/api/auth/login`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ ...ADMIN, device: { name: "e2e check", platform: "other" } }),
  });
  const { challenge } = (await login.json()) as { challenge: string };
  const second = await fetch(`${server.origin}/api/auth/login/totp`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ challenge, code: totp(totpSecret, 1) }),
  });
  const { token } = (await second.json()) as { token: string };
  const movies = (await (
    await fetch(`${server.origin}/api/movies`, { headers: { authorization: `Bearer ${token}` } })
  ).json()) as { items: { id: number }[] };
  await expect
    .poll(
      async () => {
        const movie = await fetch(`${server.origin}/api/movies/${movies.items[0]?.id}`, {
          headers: { authorization: `Bearer ${token}` },
        });
        return ((await movie.json()) as { position_ms: number }).position_ms;
      },
      { timeout: 30_000 },
    )
    .toBeGreaterThan(60_000);
});
