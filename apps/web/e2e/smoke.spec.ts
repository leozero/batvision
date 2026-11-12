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
  await expect(page.getByRole("link", { name: /Épisode 1$/ })).toBeVisible();
  await expect(page.getByRole("link", { name: /Épisode 2$/ })).toBeVisible();
  await page.getByRole("button", { name: "Tout marquer comme vu" }).click();
  await expect(page.getByRole("button", { name: "Marquer comme non vu" })).toBeVisible();
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
