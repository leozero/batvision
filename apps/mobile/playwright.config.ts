import { defineConfig } from "@playwright/test";

/** Android e2e (D64): Playwright runs the scenario, Maestro drives the app on an emulator. */
export default defineConfig({
  testDir: "./e2e",
  workers: 1,
  timeout: 300_000,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
});
