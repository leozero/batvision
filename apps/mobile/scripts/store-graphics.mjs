// Renders the Android icons and the Play Store graphics from the logo (same drawing as
// the desktop icons) with Playwright's Chromium. Run after changing the logo:
// `pnpm --filter @batvision/mobile graphics`.
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const BAT =
  "M16 11.2c.6-.9 1.1-1.7 1.3-2.5.5 1 .6 2 .4 3 1.6-.5 3.6-.4 5.6.4-1.6.5-2.4 1.5-2.3 2.8 1.7-.6 3.3-.5 4.7.3-2.3.6-3.6 2-3.9 4-1.9-1.3-3.7-1.4-5.3-.3-.3.4-.4 1-.5 1.6-.1-.6-.2-1.2-.5-1.6-1.6-1.1-3.4-1-5.3.3-.3-2-1.6-3.4-3.9-4 1.4-.8 3-.9 4.7-.3.1-1.3-.7-2.3-2.3-2.8 2-.8 4-.9 5.6-.4-.2-1-.1-2 .4-3 .2.8.7 1.6 1.3 2.5z";
const GRADIENT = `<radialGradient id="g" cx="50%" cy="40%" r="60%">
  <stop offset="0%" stop-color="#ffd77a"/><stop offset="100%" stop-color="#f4b740"/>
</radialGradient>`;
const disc = (x, y, scale) =>
  `<g transform="translate(${x} ${y}) scale(${scale})"><circle cx="16" cy="16" r="14" fill="url(#g)"/><path fill="#0b0b0f" d="${BAT}"/></g>`;

// Full-bleed square: Android launchers and Play round the corners themselves.
const square = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs>${GRADIENT}</defs><rect width="1024" height="1024" fill="#09090c"/>${disc(192, 192, 20)}</svg>`;

// Adaptive icon foreground: the disc inside the central safe zone (66 %), transparent around.
const foreground = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs>${GRADIENT}</defs>${disc(272, 272, 15)}</svg>`;

// Play feature graphic: logo, name and tagline on the app's background.
const feature = `<div style="width:1024px;height:500px;display:flex;align-items:center;gap:56px;padding:0 80px;box-sizing:border-box;
  background:radial-gradient(circle at 20% 30%, #26262f 0%, #09090c 70%);font-family:Inter,system-ui,-apple-system,Roboto,sans-serif">
  <svg width="260" height="260" viewBox="0 0 32 32"><defs>${GRADIENT}</defs>${disc(0, 0, 1)}</svg>
  <div>
    <div style="color:#f4b740;font-size:92px;font-weight:800;letter-spacing:-2px;line-height:1">BatVision</div>
    <div style="color:#f4f4f6;font-size:34px;font-weight:600;margin-top:22px;line-height:1.25">Your movies and series,<br>from your own server.</div>
    <div style="color:#a2a2b0;font-size:24px;margin-top:16px">Stream anywhere. Download for offline.</div>
  </div>
</div>`;

const outputs = [
  { html: square, size: [1024, 1024], file: "assets/icon.png" },
  {
    html: foreground,
    size: [1024, 1024],
    file: "assets/android-icon-foreground.png",
    transparent: true,
  },
  { html: square, size: [512, 512], file: "store/en-US/icon.png" },
  { html: feature, size: [1024, 500], file: "store/en-US/feature_graphic.png" },
];

const browser = await chromium.launch();
const page = await browser.newPage();
for (const { html, size, file, transparent } of outputs) {
  const [width, height] = size;
  await page.setViewportSize({ width, height });
  const content = html.startsWith("<svg ")
    ? html.replace("<svg ", `<svg width="${width}" height="${height}" `)
    : html;
  await page.setContent(
    `<html><body style="margin:0;background:transparent">${content}</body></html>`,
  );
  const out = join(root, file);
  mkdirSync(dirname(out), { recursive: true });
  await page.screenshot({ path: out, omitBackground: Boolean(transparent) });
  console.log(file);
}
await browser.close();
