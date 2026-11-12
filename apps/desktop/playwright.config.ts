import { defineConfig } from "@playwright/test";

/**
 * Drives the built app (`electron-vite build`, and the web app's `pnpm build`)
 * against a real server the tests start and stop themselves (to go offline).
 */
export default defineConfig({
  testDir: "e2e",
  fullyParallel: false,
  workers: 1,
  timeout: 240_000,
  expect: { timeout: 20_000 },
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
});
