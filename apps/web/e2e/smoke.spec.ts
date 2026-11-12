import { expect, type Page, test } from "@playwright/test";
import { SETUP_TOKEN } from "../playwright.config";
import { totp } from "./totp";

const ADMIN = { username: "admin", password: "e2e-password-123" };

// The scenario builds on itself (setup → 2FA → libraries → browsing): run in order.
test.describe.configure({ mode: "serial" });

let totpSecret = "";
// One browser session for the whole scenario: a TOTP code is accepted only once,
// so signing in again and again within 30 s would (rightly) be refused.
let page: Page;

test.beforeAll(async ({ browser }) => {
  const context = await browser.newContext({
    locale: "fr-FR",
    viewport: { width: 1440, height: 900 },
  });
  page = await context.newPage();
});

async function signIn(page: Page) {
  await page.goto("/login");
  await page.getByLabel("Identifiant").fill(ADMIN.username);
  await page.getByLabel("Mot de passe").fill(ADMIN.password);
  await page.getByRole("button", { name: "Se connecter" }).click();
  if (totpSecret) {
    await page.getByLabel("Code").fill(totp(totpSecret, 1));
    await page.getByRole("button", { name: "Vérifier" }).click();
  }
  await expect(page.getByRole("link", { name: "Accueil" })).toBeVisible();
}

test("server basics: health, private system info, API 404", async ({ request }) => {
  expect((await request.get("/api/health")).ok()).toBeTruthy();
  expect((await request.get("/api/system/info")).status()).toBe(401);
  expect((await request.get("/api/does-not-exist")).status()).toBe(404);
});

test("first launch creates the admin and asks for 2FA", async () => {
  await page.goto("/");
  await expect(page).toHaveURL(/\/setup$/);
  await page.getByLabel("Jeton d'installation").fill(SETUP_TOKEN);
  await page.getByLabel("Identifiant").fill(ADMIN.username);
  await page.getByLabel("Mot de passe", { exact: true }).fill(ADMIN.password);
  await page.getByLabel("Confirmer le mot de passe").fill(ADMIN.password);
  await page.getByRole("button", { name: "Créer le compte" }).click();
  await expect(page).toHaveURL(/\/account\?tab=security/);
  await expect(page.getByText("Inactive")).toBeVisible();
});

test("admin enables 2FA with the QR code secret", async () => {
  await page.goto("/account?tab=security");
  await page.getByRole("button", { name: "Activer la double authentification" }).click();
  await page.getByLabel("Confirmez votre mot de passe").fill(ADMIN.password);
  await page.getByRole("button", { name: "Continuer" }).click();
  totpSecret = (await page.locator("code").first().textContent())?.trim() ?? "";
  expect(totpSecret).toMatch(/^[A-Z2-7]{16,}$/);
  await page.getByLabel("Code").fill(totp(totpSecret));
  await page.getByRole("button", { name: "Activer", exact: true }).click();
  await expect(page.getByText("Active", { exact: true })).toBeVisible();
  await expect(page.getByText(/^[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}$/).first()).toBeVisible();
});

test("sign-in now asks for the second factor", async () => {
  await page.getByRole("button", { name: "Mon compte" }).click();
  await page.getByRole("menuitem", { name: "Se déconnecter" }).click();
  await expect(page).toHaveURL(/\/login/);
  await signIn(page);
  await expect(page.getByText("Votre bibliothèque est vide")).toBeVisible();
});

test("admin adds libraries and the scan fills the catalog", async () => {
  await page.goto("/admin?tab=libraries");
  const add = async (name: string, folder: string, kind: string) => {
    await page.getByLabel("Nom").fill(name);
    await page.getByLabel("Dossier").fill(folder);
    await page.getByLabel("Type").selectOption(kind);
    await page.getByRole("button", { name: "Ajouter une bibliothèque" }).click();
    await expect(page.getByText(`/media/${folder}`)).toBeVisible();
  };
  await add("Films", "Movies", "movies");
  await add("Séries", "Shows", "series");
  await expect(page.getByText("2 films")).toBeVisible();
  await expect(page.getByText("1 séries · 2 épisodes")).toBeVisible();

  await page.getByRole("link", { name: "Films" }).click();
  await expect(page.getByText("2 titres")).toBeVisible();
  await page.getByRole("link", { name: /^Heat/ }).first().click();
  await expect(page.getByRole("heading", { name: "Heat" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Lecture" })).toBeVisible();
});

test("series page lists episodes and marks them watched", async () => {
  await page.goto("/series");
  await page.getByRole("link", { name: /^Dark/ }).first().click();
  await expect(page.getByRole("heading", { name: "Dark" })).toBeVisible();
  await expect(page.getByRole("link", { name: /Épisode 1\b/ })).toBeVisible();
  await expect(page.getByRole("link", { name: /Épisode 2\b/ })).toBeVisible();
  await page.getByRole("button", { name: "Tout marquer comme vu" }).click();
  await expect(page.getByRole("button", { name: "Marquer comme non vu" })).toBeVisible();
});

test("TMDB ratings show on cards, detail pages, seasons and episodes", async () => {
  await page.goto("/movies");
  await page.getByLabel("Trier").selectOption("rating");
  const cards = page.getByRole("main").getByRole("link");
  await expect(cards.first()).toContainText("Heat");
  await expect(cards.first().getByTestId("rating")).toHaveText(/^7,9/);
  await expect(cards.nth(1).getByTestId("rating")).toHaveText(/^6,9/);

  await cards.first().click();
  const movieRating = page.getByTestId("rating");
  await expect(movieRating).toHaveText(/^7,9/);
  await expect(movieRating).toHaveAttribute("title", /^7,9\/10 sur TMDB · 7\s000 votes$/);
  await expect(movieRating).toHaveClass(/text-success/);

  await page.goto("/series");
  await page.getByRole("link", { name: /^Dark/ }).first().click();
  // Season rating next to the season picker, then one per episode with enough votes.
  const episodes = page.getByRole("listitem");
  await expect(page.getByTestId("rating")).toHaveText([/^8,4/, /^8,2/, /^7,9/]);
  await expect(episodes.nth(1).getByTestId("rating")).toHaveCount(0);
});

test("the player switches picture quality and remembers it on the device", async () => {
  await page.goto("/movies");
  await page.getByRole("link", { name: /^Heat/ }).first().click();
  const started = page.waitForResponse((r) => r.url().endsWith("/api/playback"));
  await page.getByRole("button", { name: "Lecture" }).click();
  // Auto starts at Max: the 720p original, remuxed for the browser.
  expect((await started).request().postDataJSON().quality).toBe("max");

  await page.mouse.move(700, 450);
  await page.getByRole("button", { name: "Qualité de l'image" }).click();
  await expect(page.getByRole("button", { name: /^Auto \(Max\)/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Basse\s*Original · 720p/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^SOS\s*480p · 1,1 Mbit\/s/ })).toBeVisible();

  // SOS is lighter than the file: a new session transcodes it to 480p.
  const switched = page.waitForResponse((r) => r.url().endsWith("/api/playback"));
  await page.getByRole("button", { name: /^SOS/ }).click();
  const info = await (await switched).json();
  expect([info.quality, info.method]).toEqual(["sos", "transcode"]);

  // The choice sticks on this device.
  const again = page.waitForResponse((r) => r.url().endsWith("/api/playback"));
  await page.reload();
  expect((await again).request().postDataJSON().quality).toBe("sos");
  await page.mouse.move(600, 400);
  await page.getByRole("button", { name: "Qualité de l'image" }).click();
  await page.getByRole("button", { name: /^Auto/ }).click();
  expect(await page.evaluate(() => localStorage.getItem("bv.quality"))).toBe("auto");
});

test("Auto lowers the quality when the network can't keep up", async () => {
  test.setTimeout(60_000);
  await page.evaluate(() => localStorage.setItem("bv.quality.auto", "max"));
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Network.enable");
  // About half the movie's bitrate: the first segment takes over 10 s.
  await cdp.send("Network.emulateNetworkConditions", {
    offline: false,
    latency: 20,
    downloadThroughput: (700 * 1000) / 8,
    uploadThroughput: -1,
  });
  try {
    const lowered = page.waitForRequest(
      (r) => r.url().endsWith("/api/playback") && r.postDataJSON()?.quality === "sos",
      { timeout: 40_000 },
    );
    await page.goto("/movies");
    await page.getByRole("link", { name: /^Heat/ }).first().click();
    await page.getByRole("button", { name: "Lecture" }).click();
    await lowered;
    expect(await page.evaluate(() => localStorage.getItem("bv.quality.auto"))).toBe("sos");
  } finally {
    await cdp.send("Network.emulateNetworkConditions", {
      offline: false,
      latency: 0,
      downloadThroughput: -1,
      uploadThroughput: -1,
    });
    await cdp.detach();
  }
});

test("search finds titles", async () => {
  await page.goto("/search");
  await page.getByPlaceholder("Films, séries…").fill("ron");
  await expect(page.getByRole("link", { name: /^Ronin/ })).toBeVisible();
  await expect(page.getByRole("link", { name: /^Heat/ })).toHaveCount(0);
});

test("invitation link creates an account", async ({ browser }) => {
  await page.goto("/admin?tab=users");
  await page.getByRole("button", { name: "Créer un lien d'invitation" }).click();
  const link = await page.locator("input[readonly]").inputValue();
  expect(link).toContain("/invite/");

  const guest = await (await browser.newContext({ locale: "fr-FR" })).newPage();
  await guest.goto(new URL(link).pathname);
  await guest.getByLabel("Identifiant").fill("alice");
  await guest.getByLabel("Mot de passe", { exact: true }).fill("alice-password-123");
  await guest.getByLabel("Confirmer le mot de passe").fill("alice-password-123");
  await guest.getByRole("button", { name: "Créer mon compte" }).click();
  await expect(guest.getByRole("link", { name: "Accueil" })).toBeVisible();
  await expect(guest.getByText("Heat").first()).toBeVisible();
});

test("client-side routes survive a reload and unknown pages say so", async () => {
  await page.goto("/movies");
  await page.reload();
  await expect(page.getByRole("heading", { name: "Films" })).toBeVisible();
  await page.goto("/nope/nothing");
  await expect(page.getByText("Page introuvable")).toBeVisible();
});

test("sign out returns to the sign-in page", async () => {
  await page.goto("/");
  await page.getByRole("button", { name: "Mon compte" }).click();
  await page.getByRole("menuitem", { name: "Se déconnecter" }).click();
  await expect(page).toHaveURL(/\/login/);
  await page.goto("/");
  await expect(page).toHaveURL(/\/login/);
});

test("watchlist and series downloads show up for a download device", async ({ browser }) => {
  // Sign in an "Android phone" through the API: browsers can't download.
  const phone = await browser.newContext();
  const login = await phone.request.post("/api/auth/login", {
    data: {
      username: "alice",
      password: "alice-password-123",
      device: { name: "Pixel e2e", platform: "android" },
    },
  });
  expect(login.ok()).toBeTruthy();
  const token = (await login.json()).token as string;

  const alice = await (await browser.newContext({ locale: "fr-FR" })).newPage();
  await alice.goto("/login");
  await alice.getByLabel("Identifiant").fill("alice");
  await alice.getByLabel("Mot de passe").fill("alice-password-123");
  await alice.getByRole("button", { name: "Se connecter" }).click();
  await expect(alice.getByRole("link", { name: "Accueil" })).toBeVisible();

  // Movie → watchlist.
  await alice.goto("/movies");
  await alice.getByRole("link", { name: /^Heat/ }).first().click();
  await alice.getByRole("button", { name: "Watchlist" }).click();
  await alice.getByRole("button", { name: "Ajouter à la watchlist" }).click();
  await expect(alice.getByRole("button", { name: "Dans la watchlist" })).toBeVisible();

  // Series → next 2 episodes on the phone.
  await alice.goto("/series");
  await alice.getByRole("link", { name: /^Dark/ }).first().click();
  await alice.getByRole("button", { name: "Télécharger" }).click();
  await alice.getByLabel("Combien").selectOption("2");
  await alice.getByRole("dialog").getByRole("button", { name: "Télécharger" }).click();

  await alice.goto("/downloads");
  await expect(alice.getByRole("heading", { name: "Pixel e2e" })).toBeVisible();
  await expect(alice.getByText("Les 2 prochains épisodes")).toBeVisible();
  await expect(alice.getByText("Heat").first()).toBeVisible();

  // The phone sees the same expected set.
  const plan = await phone.request.post("/api/downloads/plan", {
    data: {},
    headers: { authorization: `Bearer ${token}` },
  });
  const items = (await plan.json()).items as { kind: string; source: string }[];
  expect(items.filter((i) => i.kind === "episode")).toHaveLength(2);
  expect(items.every((i) => i.source === "rolling")).toBeTruthy();
});
