import { expect, test } from "@playwright/test";

test("web app is served by the server and reaches the API", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "batvision" })).toBeVisible();
  await expect(page.getByTestId("server-status")).toContainText("ok");
});

test("client-side routes survive a reload", async ({ page }) => {
  await page.goto("/series/42");
  await expect(page.getByRole("heading", { name: "batvision" })).toBeVisible();
});

test("unknown API routes return 404, not the SPA", async ({ request }) => {
  const res = await request.get("/api/does-not-exist");
  expect(res.status()).toBe(404);
});
