// Renders the website's demo media from the real HUD ring (app/ui/hud) in scripted
// scenes: animated WebP per scene, a still PNG (used for reduced motion), and the
// Open Graph card. Needs the bundled FFmpeg (vendor/ffmpeg, scripts/fetch_ffmpeg.py).
//
//   node tools/site-media.mjs            → ../docs/assets/

import { chromium } from 'playwright';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(here, '..');
const out = path.resolve(appRoot, '../docs/assets');
const exe = process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg';
const ffmpeg = [path.resolve(appRoot, '../vendor/ffmpeg', process.platform === 'win32' ? 'windows' : 'linux', exe), exe].find((p) => p === exe || fs.existsSync(p));

const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2' };
const server = http.createServer((req, res) => {
  const file = path.join(appRoot, decodeURIComponent(new URL(req.url, 'http://x').pathname));
  if (!file.startsWith(appRoot) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}/tools/site-media/scene.html`;
const size = { width: 960, height: 600 };
const browser = await chromium.launch();

async function still(scene, file, viewport = size) {
  const page = await browser.newPage({ viewport, deviceScaleFactor: 1 });
  await page.goto(`${base}?scene=${scene}&still`);
  await page.waitForFunction(() => window.__still === true, null, { timeout: 15000 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(out, file) });
  await page.close();
}

async function animated(scene, file) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'morph-media-'));
  const ctx = await browser.newContext({ viewport: size, deviceScaleFactor: 1, recordVideo: { dir, size } });
  const page = await ctx.newPage();
  const t0 = Date.now();
  await page.goto(`${base}?scene=${scene}`);
  await page.waitForFunction(() => document.fonts.status === 'loaded');
  const start = (Date.now() - t0) / 1000;
  await page.waitForFunction(() => window.__done === true, null, { timeout: 60000 });
  await ctx.close();
  const webm = fs.readdirSync(dir).find((f) => f.endsWith('.webm'));
  execFileSync(ffmpeg, ['-y', '-loglevel', 'error', '-ss', String(start + 0.2), '-i', path.join(dir, webm),
    '-vf', 'fps=20', '-c:v', 'libwebp_anim', '-quality', '72', '-compression_level', '6', '-loop', '0', '-an',
    path.join(out, file)]);
  fs.rmSync(dir, { recursive: true, force: true });
  console.log(`${file}: ${(fs.statSync(path.join(out, file)).size / 1024).toFixed(0)} KB`);
}

for (const scene of ['convert', 'tools', 'chain', 'actions', 'wayland']) {
  const name = scene === 'wayland' ? 'wayland-dock' : `demo-${scene}`;
  await still(scene, `${name}.png`);
  await animated(scene, `${name}.webp`);
}
await still('og', 'og.png', { width: 1200, height: 630 });
console.log('og.png');
await browser.close();
server.close();
