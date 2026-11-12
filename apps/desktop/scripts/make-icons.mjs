// Renders the app and tray icons from the logo SVG with Playwright's Chromium.
// Run once after changing the logo: `pnpm --filter @batvision/desktop icons`.
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const BAT =
  "M16 11.2c.6-.9 1.1-1.7 1.3-2.5.5 1 .6 2 .4 3 1.6-.5 3.6-.4 5.6.4-1.6.5-2.4 1.5-2.3 2.8 1.7-.6 3.3-.5 4.7.3-2.3.6-3.6 2-3.9 4-1.9-1.3-3.7-1.4-5.3-.3-.3.4-.4 1-.5 1.6-.1-.6-.2-1.2-.5-1.6-1.6-1.1-3.4-1-5.3.3-.3-2-1.6-3.4-3.9-4 1.4-.8 3-.9 4.7-.3.1-1.3-.7-2.3-2.3-2.8 2-.8 4-.9 5.6-.4-.2-1-.1-2 .4-3 .2.8.7 1.6 1.3 2.5z";

// App icon: the amber disc on a dark rounded square (macOS-style margins).
const appIcon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs><radialGradient id="g" cx="50%" cy="40%" r="60%">
    <stop offset="0%" stop-color="#ffd77a"/><stop offset="100%" stop-color="#f4b740"/>
  </radialGradient></defs>
  <rect x="100" y="100" width="824" height="824" rx="185" fill="#0b0b0f"/>
  <g transform="translate(192 192) scale(20)">
    <circle cx="16" cy="16" r="14" fill="url(#g)"/>
    <path fill="#0b0b0f" d="${BAT}"/>
  </g>
</svg>`;

// Tray: a disc with the bat cut out. Black for the macOS template, amber elsewhere.
const trayIcon = (color) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
  <mask id="m"><rect width="32" height="32" fill="#fff"/><path fill="#000" d="${BAT}"/></mask>
  <circle cx="16" cy="16" r="14" fill="${color}" mask="url(#m)"/>
</svg>`;

const outputs = [
  { svg: appIcon, size: 1024, file: "build/icon.png" },
  { svg: appIcon, size: 512, file: "resources/icon.png" },
  { svg: trayIcon("#000"), size: 16, file: "resources/trayTemplate.png" },
  { svg: trayIcon("#000"), size: 32, file: "resources/trayTemplate@2x.png" },
  { svg: trayIcon("#f4b740"), size: 32, file: "resources/tray.png" },
];

const browser = await chromium.launch();
const page = await browser.newPage();
for (const { svg, size, file } of outputs) {
  await page.setViewportSize({ width: size, height: size });
  await page.setContent(
    `<html><body style="margin:0;background:transparent">${svg.replace("<svg ", `<svg width="${size}" height="${size}" `)}</body></html>`,
  );
  const out = join(root, file);
  mkdirSync(dirname(out), { recursive: true });
  await page.screenshot({ path: out, omitBackground: true });
  console.log(file);
}
await browser.close();
