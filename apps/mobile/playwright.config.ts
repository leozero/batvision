import { defineConfig } from "@playwright/test";

/** Mobile e2e (D64, D91): Playwright runs the scenario, Maestro drives the app on an Android
 * emulator, or an iOS simulator with PLATFORM=ios. */
export default defineConfig({
  testDir: "./e2e",
  workers: 1,
  timeout: 300_000,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
});
