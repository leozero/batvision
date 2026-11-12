import { defineConfig } from "electron-vite";

// The interface is the web app's own build (apps/web/dist), served by the main
// process: only main and preload are built here.
export default defineConfig({
  main: {
    build: { externalizeDeps: true },
  },
  preload: {
    build: { externalizeDeps: true },
  },
});
