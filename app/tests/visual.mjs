// HUD visual + behaviour tests for S2.
//
// Renders the component gallery in Chromium (WebView2's engine) and WebKit (WebKitGTK's
// engine), fully offline, and checks:
//   1. each engine matches its committed snapshot within 1 % of pixels;
//   2. the two engines match each other within 1 %;
//   3. bundled fonts load with every external request blocked;
//   4. reduced motion (media query and the platform-driven .hud-reduced class) stops
//      rotation, flicker and scan-line;
//   5. forced colours drop the glow;
//   6. the idle ring's frame time is reported (the glow is dropped when over budget).
//
//   node tests/visual.mjs            run
//   node tests/visual.mjs --update   rewrite snapshots

import { chromium, webkit } from 'playwright';
import { PNG } from 'pngjs';
import pixelmatch from 'pixelmatch';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const uiRoot = path.resolve(here, '../ui');
const snapDir = path.join(here, 'snapshots');
const update = process.argv.includes('--update');
const MAX_DIFF = 0.01;
const VIEWPORT = { width: 1400, height: 900 };

const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  const file = path.join(uiRoot, decodeURIComponent(url.pathname));
  if (!file.startsWith(uiRoot) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

let failures = 0;
const check = (ok, msg) => { console.log(`${ok ? '✓' : '✗'} ${msg}`); if (!ok) failures++; };

async function openGallery(browser, query, ctxOpts = {}) {
  const ctx = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, ...ctxOpts });
  const page = await ctx.newPage();
  const external = [];
  await page.route('**/*', (route) => {
    const u = route.request().url();
    if (u.startsWith(base)) return route.continue();
    external.push(u);
    return route.abort();
  });
  await page.goto(`${base}/gallery.html${query}`);
  await page.waitForFunction(() => document.documentElement.dataset.ready === '1');
  await page.evaluate(() => document.fonts.ready);
  return { ctx, page, external };
}

function diff(a, b) {
  if (a.width !== b.width || a.height !== b.height) return 1;
  const out = new PNG({ width: a.width, height: a.height });
  const n = pixelmatch(a.data, b.data, out.data, a.width, a.height, { threshold: 0.15, includeAA: false });
  return n / (a.width * a.height);
}

const shots = {};
for (const [name, type] of [['chromium', chromium], ['webkit', webkit]]) {
  const browser = await type.launch();
  try {
    // 1. Snapshot
    const { ctx, page, external } = await openGallery(browser, '?static');
    const fontsOk = await page.evaluate(() =>
      ['600 14px "Chakra Petch"', '700 24px "Chakra Petch"', '400 10px "JetBrains Mono"'].every((f) => document.fonts.check(f)));
    check(fontsOk, `${name}: bundled fonts load offline`);
    check(external.length === 0, `${name}: no external requests${external.length ? ' (' + external.join(', ') + ')' : ''}`);

    const buf = await page.screenshot({ fullPage: true });
    const png = PNG.sync.read(buf);
    shots[name] = png;
    const snap = path.join(snapDir, `gallery-${name}-${process.platform}.png`);
    if (update || !fs.existsSync(snap)) {
      fs.mkdirSync(snapDir, { recursive: true });
      fs.writeFileSync(snap, buf);
      console.log(`• ${name}: snapshot written`);
    } else {
      const d = diff(png, PNG.sync.read(fs.readFileSync(snap)));
      check(d < MAX_DIFF, `${name}: matches snapshot (${(d * 100).toFixed(2)} % differs)`);
    }

    // 5. Forced colours
    await page.emulateMedia({ forcedColors: 'active' });
    const glow = await page.evaluate(() => getComputedStyle(document.querySelector('.hud-glow')).filter);
    check(glow === 'none', `${name}: forced colours drop the glow (filter: ${glow})`);
    await ctx.close();

    // 4. Reduced motion via the media query…
    const rm = await openGallery(browser, '', { reducedMotion: 'reduce' });
    const anims = await rm.page.evaluate(() =>
      ['.hud-spin-slow', '.hud-spin-mid', '.hud-spin-fast', '.hud-scan', '.hud-flicker'].map((s) => getComputedStyle(document.querySelector(s)).animationName));
    check(anims.every((a) => !['hud-cw', 'hud-ccw', 'hud-scan', 'hud-flicker'].includes(a)), `${name}: prefers-reduced-motion stops rotation and scan-line (${anims.join(',')})`);
    await rm.ctx.close();

    // …and via the class the app sets from the OS setting (GNOME enable-animations, Windows UISettings).
    const rc = await openGallery(browser, '?reduced');
    const anims2 = await rc.page.evaluate(() =>
      [...document.querySelectorAll('.hud-reduced .hud-spin, .hud-reduced .hud-scan, .hud-reduced .hud-flicker')]
        .map((n) => getComputedStyle(n).animationName));
    // A 100 ms fade is allowed; rotation, flicker and scan-line are not.
    const banned = ['hud-cw', 'hud-ccw', 'hud-scan', 'hud-flicker'];
    check(anims2.length > 0 && anims2.every((a) => !banned.includes(a)), `${name}: .hud-reduced stops rotation, flicker and scan-line`);

    // Normal motion: decorative rings rotate after boot.
    const spinning = await openGallery(browser, '');
    const names = await spinning.page.evaluate(() =>
      ['.hud-spin-slow', '.hud-spin-mid', '.hud-spin-fast'].map((s) => getComputedStyle(document.querySelector(s)).animationName));
    check(names.join() === 'hud-cw,hud-ccw,hud-cw', `${name}: idle rings rotate (${names.join(',')})`);
    await spinning.ctx.close();

    // 6. Idle frame time (the gallery measures it and drops the glow when over budget).
    await rc.ctx.close();
    const live = await openGallery(browser, '');
    await live.page.waitForFunction(() => document.title.includes('ms/frame'), null, { timeout: 10_000 });
    const title = await live.page.title();
    console.log(`• ${name}: idle ${title.split('·')[1].trim()}`);
    await live.ctx.close();
  } finally {
    await browser.close();
  }
}

// 2. Cross-engine parity.
const cross = diff(shots.chromium, shots.webkit);
const dims = (p) => `${p.width}×${p.height}`;
check(cross < MAX_DIFF, `chromium vs webkit: ${(cross * 100).toFixed(2)} % of pixels differ (${dims(shots.chromium)} vs ${dims(shots.webkit)})`);

server.close();
if (failures) { console.error(`\n${failures} check(s) failed`); process.exit(1); }
console.log('\nall HUD checks passed');
