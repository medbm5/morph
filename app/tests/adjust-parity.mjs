// S6: the editor's WebGL2 preview matches the Rust export within ΔE < 2.
//
// Renders a photo-like fixture with several adjustment sets twice — once through
// app/ui/editors/adjust-shader.js in headless Chromium (WebView2's engine), once
// through `morph tool edit` — and compares them pixel by pixel in CIE Lab.
//
//   node tests/adjust-parity.mjs   (builds the CLI with cargo if needed)

import { chromium } from 'playwright';
import { PNG } from 'pngjs';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const ui = path.resolve(here, '../ui');
const exe = path.join(root, 'target', 'debug', process.platform === 'win32' ? 'morph.exe' : 'morph');
execFileSync('cargo', ['build', '-q', '-p', 'morph-cli'], { cwd: root, stdio: 'inherit' });

// Fixture: gradients, edges and texture, 320×240.
const work = fs.mkdtempSync(path.join(os.tmpdir(), 'morph-parity-'));
const W = 320, H = 240;
const fx = new PNG({ width: W, height: H });
let seed = 7;
const rnd = () => { seed = (seed * 1103515245 + 12345) >>> 0; return (seed >>> 16) / 65536; };
for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
  const i = (y * W + x) * 4;
  const sky = y < H * 0.45;
  const n = rnd() * 18;
  fx.data[i] = sky ? 120 + y * 0.5 + n : 60 + (x % 40 < 20 ? 70 : 20) + n;
  fx.data[i + 1] = sky ? 170 + y * 0.3 + n : 90 + Math.sin(x / 9) * 40 + n;
  fx.data[i + 2] = sky ? 230 - y * 0.2 : 50 + (y % 30) * 2 + n;
  fx.data[i + 3] = 255;
}
const fixture = path.join(work, 'fixture.png');
fs.writeFileSync(fixture, PNG.sync.write(fx));

const CASES = [
  { exposure: 0.8 },
  { contrast: 0.6, saturation: 0.4 },
  { temperature: 0.7, tint: -0.4, vibrance: 0.5 },
  { detail: 0.8, noise_reduction: 0.3 },
  { clarity: 0.9, dehaze: 0.6 },
  { grain: 0.7, exposure: -0.5, contrast: -0.4 },
  { exposure: 0.3, contrast: 0.2, saturation: -0.3, temperature: -0.3, detail: 0.4, clarity: 0.5, dehaze: 0.3, grain: 0.2, noise_reduction: 0.2 },
];

// sRGB → Lab (D65).
function lab(r, g, b) {
  const lin = (c) => { c /= 255; return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
  const [R, G, B] = [lin(r), lin(g), lin(b)];
  const X = (0.4124 * R + 0.3576 * G + 0.1805 * B) / 0.95047;
  const Y = 0.2126 * R + 0.7152 * G + 0.0722 * B;
  const Z = (0.0193 * R + 0.1192 * G + 0.9505 * B) / 1.08883;
  const f = (t) => (t > 216 / 24389 ? Math.cbrt(t) : (24389 / 27 * t + 16) / 116);
  return [116 * f(Y) - 16, 500 * (f(X) - f(Y)), 200 * (f(Y) - f(Z))];
}

const server = http.createServer((req, res) => {
  const u = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  const file = u.startsWith('/work/') ? path.join(work, u.slice(6)) : path.join(ui, u);
  if (!fs.existsSync(file)) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': file.endsWith('.js') ? 'text/javascript' : file.endsWith('.png') ? 'image/png' : 'text/html' });
  fs.createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;
fs.writeFileSync(path.join(work, 'harness.html'), `<!doctype html><canvas id=c></canvas><script src="${base}/editors/adjust-shader.js"></script>`);

const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
const page = await browser.newPage();
await page.goto(`${base}/work/harness.html`);
let failed = 0;

for (const params of CASES) {
  const label = Object.entries(params).map(([k, v]) => `${k}=${v}`).join(' ');
  // Rust export.
  const args = ['tool', 'edit', fixture, ...Object.entries(params).flatMap(([k, v]) => [`--${k}`, String(v)])];
  const out = execFileSync(exe, args, { encoding: 'utf8' });
  const saved = out.match(/SAVED · (.*)/)[1].trim();
  const rust = PNG.sync.read(fs.readFileSync(path.isAbsolute(saved) ? saved : path.join(process.cwd(), saved)));
  fs.rmSync(path.isAbsolute(saved) ? saved : path.join(process.cwd(), saved));

  // WebGL preview.
  const gl = await page.evaluate(async ({ src, params }) => {
    const img = new Image(); img.src = src; await img.decode();
    const r = createAdjustRenderer(document.getElementById('c'));
    r.setImage(img); r.render(params);
    return Array.from(r.readPixels());
  }, { src: `${base}/work/fixture.png`, params });

  let sum = 0, max = 0, moved = 0;
  const des = [];
  for (let i = 0; i < W * H * 4; i += 4) {
    const a = lab(rust.data[i], rust.data[i + 1], rust.data[i + 2]);
    const b = lab(gl[i], gl[i + 1], gl[i + 2]);
    const de = Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    sum += de; max = Math.max(max, de); des.push(de);
    const o = lab(fx.data[i], fx.data[i + 1], fx.data[i + 2]);
    moved += Math.hypot(a[0] - o[0], a[1] - o[1], a[2] - o[2]);
  }
  moved /= W * H;
  des.sort((x, y) => x - y);
  const mean = sum / (W * H), p99 = des[Math.floor(des.length * 0.99)];
  // Guard against two no-ops "matching": the adjustment must visibly change the photo.
  const ok = mean < 2 && p99 < 4 && moved > 1;
  if (!ok) failed++;
  console.log(`${ok ? '✓' : '✗'} ${label.padEnd(60)} ΔE mean ${mean.toFixed(3)}  p99 ${p99.toFixed(3)}  max ${max.toFixed(2)}  (change vs original ${moved.toFixed(1)})`);
}

await browser.close();
server.close();
fs.rmSync(work, { recursive: true, force: true });
if (failed) { console.error(`${failed} case(s) over budget`); process.exit(1); }
console.log('preview matches export within ΔE < 2');
