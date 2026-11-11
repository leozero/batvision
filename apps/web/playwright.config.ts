import { defineConfig } from "@playwright/test";

const port = 8421;

/**
 * End-to-end tests run the real Rust server serving the built web app (`pnpm build` first),
 * exactly like the production container does.
 */
export default defineConfig({
  testDir: "e2e",
  use: { baseURL: `http://127.0.0.1:${port}` },
  webServer: {
    command: "cargo run --quiet --manifest-path ../../server/Cargo.toml",
    url: `http://127.0.0.1:${port}/api/health`,
    reuseExistingServer: !process.env.CI,
    timeout: 300_000,
    env: {
      BATVISION_BIND: `127.0.0.1:${port}`,
      BATVISION_WEB_DIR: "dist",
      BATVISION_DATA_DIR: "test-results/server/data",
      BATVISION_CACHE_DIR: "test-results/server/cache",
    },
  },
});
