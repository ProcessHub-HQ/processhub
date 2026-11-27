#!/usr/bin/env node
/**
 * generate-icons.mjs
 *
 * Converts public/icon.svg into every icon asset the project needs:
 *
 *   src-tauri/icons/
 *     icon.png               — 1024×1024 master PNG (used by `tauri icon` and as source)
 *     32x32.png
 *     128x128.png
 *     128x128@2x.png         — 256×256 actual pixels
 *     Square30x30Logo.png    — Windows Store / UWP (each at exact pixel size)
 *     Square44x44Logo.png
 *     Square71x71Logo.png
 *     Square89x89Logo.png
 *     Square107x107Logo.png
 *     Square142x142Logo.png
 *     Square150x150Logo.png
 *     Square284x284Logo.png
 *     Square310x310Logo.png
 *     StoreLogo.png          — 50×50
 *     icon.ico               — multi-size Windows icon (16,24,32,48,64,128,256)
 *     icon.icns              — macOS bundle icon (all required sizes)
 *
 *   public/
 *     favicon.ico            — 32×32 web favicon
 *     icon.svg               — (already exists, not touched)
 *
 *   src/assets/
 *     icon.png               — 512×512 for use in React UI (e.g. splash, about)
 *
 * Requirements:
 *   sharp      — raster conversion and resizing
 *   png2icons  — .icns and .ico generation from a PNG
 *
 * Install once:  pnpm add -D sharp png2icons
 * Run:           node scripts/generate-icons.mjs
 */

import { createRequire } from "module";
import { readFileSync, writeFileSync, mkdirSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const require = createRequire(import.meta.url);
const sharp = require("sharp");
const png2icons = require("png2icons");

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const SVG_SRC = resolve(ROOT, "public/icon.svg");
const TRAY_SVG_SRC = resolve(ROOT, "public/tray-icon.svg");

const TAURI_ICONS = resolve(ROOT, "src-tauri/icons");
const PUBLIC = resolve(ROOT, "public");
const REACT_ASSETS = resolve(ROOT, "src/assets");

// Ensure output directories exist
[TAURI_ICONS, PUBLIC, REACT_ASSETS].forEach((d) =>
  mkdirSync(d, { recursive: true })
);

// ─── Helper ──────────────────────────────────────────────────────────────────

/**
 * Rasterise the SVG at a given size and return a PNG Buffer.
 * We always render from the original SVG to avoid resampling artefacts.
 */
async function svgToPng(size, source = SVG_SRC) {
  return sharp(source)
    .resize(size, size, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .png({ compressionLevel: 9 })
    .toBuffer();
}

/** Write a PNG buffer to a file and log it. */
function writePng(buf, filePath) {
  writeFileSync(filePath, buf);
  const rel = filePath.replace(ROOT + "/", "");
  console.log(`  ✓  ${rel}`);
}

// ─── Main ─────────────────────────────────────────────────────────────────────

async function main() {
  console.log("\nProcessHub Icon Generator");
  console.log("Source:", SVG_SRC.replace(ROOT + "/", ""));
  console.log("─".repeat(50));

  // 1. Master 1024×1024 PNG  ─────────────────────────────────────────────────
  console.log("\n[1/5] Master PNG (1024×1024)");
  const master1024 = await svgToPng(1024);
  writePng(master1024, resolve(TAURI_ICONS, "icon.png"));

  // 2. Standard numbered PNGs  ───────────────────────────────────────────────
  console.log("\n[2/5] Standard PNGs");
  const px32 = await svgToPng(32);
  writePng(px32, resolve(TAURI_ICONS, "32x32.png"));

  const px128 = await svgToPng(128);
  writePng(px128, resolve(TAURI_ICONS, "128x128.png"));

  const px256 = await svgToPng(256); // 128@2x — 256 actual pixels
  writePng(px256, resolve(TAURI_ICONS, "128x128@2x.png"));

  // 3. Windows Store / UWP Square logos  ─────────────────────────────────────
  console.log("\n[3/5] Windows Store / UWP logos");
  const squareSizes = [30, 44, 71, 89, 107, 142, 150, 284, 310];
  for (const s of squareSizes) {
    const buf = await svgToPng(s);
    writePng(buf, resolve(TAURI_ICONS, `Square${s}x${s}Logo.png`));
  }
  // StoreLogo is 50×50
  const storeLogo = await svgToPng(50);
  writePng(storeLogo, resolve(TAURI_ICONS, "StoreLogo.png"));

  // 4. icon.ico + icon.icns  ─────────────────────────────────────────────────
  console.log("\n[4/5] icon.ico and icon.icns");
  // png2icons needs a 1024×1024 source for best quality
  const icoOut = png2icons.createICO(master1024, png2icons.BILINEAR, 0, true);
  if (!icoOut) throw new Error("ICO generation failed");
  writeFileSync(resolve(TAURI_ICONS, "icon.ico"), icoOut);
  console.log("  ✓  src-tauri/icons/icon.ico");

  const icnsOut = png2icons.createICNS(master1024, png2icons.BILINEAR, 0);
  if (!icnsOut) throw new Error("ICNS generation failed");
  writeFileSync(resolve(TAURI_ICONS, "icon.icns"), icnsOut);
  console.log("  ✓  src-tauri/icons/icon.icns");

  // 5. Web / React assets  ───────────────────────────────────────────────────
  console.log("\n[5/5] Web / React assets");

  // public/favicon.ico  (32×32 ICO is fine for browsers)
  const faviconPng = await svgToPng(32);
  const faviconIco = png2icons.createICO(faviconPng, png2icons.BILINEAR, 0, false);
  if (!faviconIco) throw new Error("Favicon ICO generation failed");
  writeFileSync(resolve(PUBLIC, "favicon.ico"), faviconIco);
  console.log("  ✓  public/favicon.ico");

  // src/assets/icon.png  (512×512, used in React UI e.g. About modal / splash)
  const px512 = await svgToPng(512);
  writePng(px512, resolve(REACT_ASSETS, "icon.png"));

  // src-tauri/icons/tray-icon.png  (64×64 white rounded-rect tray icon)
  const trayPng = await svgToPng(64, TRAY_SVG_SRC);
  writePng(trayPng, resolve(TAURI_ICONS, "tray-icon.png"));

  // ─── Done ──────────────────────────────────────────────────────────────────
  console.log("\n" + "─".repeat(50));
  console.log("Done! All icons generated from public/icon.svg\n");
}

main().catch((err) => {
  console.error("\nError:", err.message);
  process.exit(1);
});
