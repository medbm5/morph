// Settings page smoke test: renders expanded mode in Chromium against a mocked Tauri
// bridge and checks the controls round-trip into `set_settings`, the contrast warning,
// the ring-layout editor and the French translation.
//
//   node tests/settings-page.mjs [--shot out.png]

import { chromium } from 'playwright';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const uiRoot = path.resolve(here, '../ui');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = http.createServer((req, res) => {
  const file = path.join(uiRoot, decodeURIComponent(new URL(req.url, 'http://x').pathname));
  if (!file.startsWith(uiRoot) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

let failures = 0;
const check = (ok, msg) => { console.log(`${ok ? '✓' : '✗'} ${msg}`); if (!ok) failures++; };

// A minimal stand-in for window.__TAURI__ with an in-memory settings store.
const mock = () => {
  const listeners = {};
  const settings = {
    keys: { convert: 'shift', tools: 'ctrl', actions: 'alt' }, accent: '#FFB347', motion: 'system',
    naming: { convert: '{stem}.{ext}', tool: '{stem} {op}.{ext}', collision: '{name} {n}', conflict: 'number' },
    layouts: {}, history: true, autostart: true, language: 'auto', watch: [],
  };
  window.__calls = [];
  const look = () => ({ accent: settings.accent, reduced: settings.motion === 'reduced', language: settings.language === 'fr' ? 'fr' : 'en' });
  const emit = (ev, payload) => (listeners[ev] || []).forEach((f) => f({ payload }));
  const handlers = {
    expanded_request: () => ({ mode: 'settings', paths: [] }),
    get_settings: () => JSON.parse(JSON.stringify(settings)),
    get_look: look,
    portable_status: () => ({ supported: true, on: false, folder: 'C:\\Apps\\Metamorphosis' }),
    layout_ops: ({ ring }) => ring === 'tools'
      ? [{ id: 'image.crop', label: 'Crop' }, { id: 'image.compress', label: 'Compress' }, { id: 'image.edit', label: 'Edit' }]
      : [{ id: 'image.to-jpg', label: 'JPG' }, { id: 'image.to-png', label: 'PNG' }, { id: 'image.to-webp', label: 'WEBP' }],
    watch_ops: () => [{ id: 'image.to-png', label: 'PNG', ring: 'convert' }, { id: 'image.compress', label: 'Compress', ring: 'tools' }],
    pick_folder: () => 'D:/Photos/Inbox',
    set_settings: ({ settings: s }) => { Object.assign(settings, JSON.parse(JSON.stringify(s))); emit('settings://changed', look()); },
  };
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => { window.__calls.push([cmd, args]); return handlers[cmd] ? handlers[cmd](args || {}) : null; },
      convertFileSrc: (p) => p,
    },
    event: { listen: async (ev, f) => { (listeners[ev] = listeners[ev] || []).push(f); return () => {}; } },
    app: { getVersion: async () => '1.0.0' },
    window: { getCurrentWindow: () => ({ hide() {} }) },
    webview: { getCurrentWebview: () => ({ onDragDropEvent() {} }) },
  };
  window.__settings = settings;
};

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1100, height: 1500 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
await page.addInitScript(mock);
await page.goto(`${base}/expanded.html`);
await page.waitForSelector('text=WHAT THE RING SHOWS');
await page.waitForSelector('text=1. JPG');

// Trigger keys: picking a duplicate is rejected by the backend; here we just check it's sent.
await page.selectOption('select[aria-label^="Tools"]', 'super');
await page.waitForTimeout(400);
check((await page.evaluate(() => window.__settings.keys.tools)) === 'super', 'trigger key change is saved');

// Accent: a dark custom colour triggers the contrast warning.
await page.$eval('input[type=color]', (i) => { i.value = '#203040'; i.dispatchEvent(new Event('input')); });
check(await page.isVisible('text=/hard to read/'), 'low-contrast accent shows a warning');
await page.click('button:has-text("Cyan")');
await page.waitForTimeout(400);
check((await page.evaluate(() => window.__settings.accent)) === '#4FE3FF', 'accent preset is saved');
check((await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--hud-accent').trim().toUpperCase())) === '#4FE3FF', 'accent applies to the page');

// Ring layout: move PNG up, hide WEBP, pin it back… then check the stored layout.
await page.click('button[aria-label="Move up: PNG"]');
await page.waitForTimeout(400);
await page.click('div.hud-row:has(> span:text("3. WEBP")) >> button:text("Hide")');
await page.waitForTimeout(400);
const lay = await page.evaluate(() => window.__settings.layouts['convert:image']);
check(lay && lay.order[0] === 'image.to-png' && lay.hidden.includes('image.to-webp'), `layout stored (${JSON.stringify(lay)})`);

// Naming preview and conflict policy.
await page.selectOption('select[aria-label="Name of edited files"]', 'custom');
await page.fill('input[aria-label="Name of edited files"]', '{stem} - {op}.{ext}');
check(await page.isVisible('text=/photo - Cropped\\.jpg/'), 'naming preview follows a custom name');
await page.selectOption('select[aria-label="Name of converted files"]', '{stem} {date}.{ext}');
await page.selectOption('select[aria-label="If a file with that name already exists"]', 'skip');
await page.waitForTimeout(400);
check((await page.evaluate(() => window.__settings.naming.conflict)) === 'skip', 'conflict policy is saved');
check((await page.evaluate(() => window.__settings.naming.convert)) === '{stem} {date}.{ext}', 'naming choice is saved');
check((await page.evaluate(() => window.__settings.naming.tool)) === '{stem} - {op}.{ext}', 'custom name is saved');

// Watch folders: add one (folder picker mocked), it defaults to Compress.
await page.click('button:has-text("Add a folder")');
await page.waitForTimeout(400);
const watch = await page.evaluate(() => window.__settings.watch);
check(watch && watch.length === 1 && watch[0].op === 'image.compress' && watch[0].out === 'Metamorphosis', `watch folder stored (${JSON.stringify(watch)})`);

const shot = process.argv.includes('--shot') ? process.argv[process.argv.indexOf('--shot') + 1] : null;
if (shot) await page.screenshot({ path: shot, fullPage: true });

// French.
await page.selectOption('select[aria-label="Language"]', 'fr');
await page.waitForSelector('text=CE QUE MONTRE L’ANNEAU', { timeout: 3000 }).catch(() => {});
check(await page.isVisible('text=CE QUE MONTRE L’ANNEAU'), 'switching to French re-renders the page');
check((await page.evaluate(() => document.documentElement.lang)) === 'fr', 'document language is fr');
if (shot) await page.screenshot({ path: shot.replace(/\.png$/, '-fr.png'), fullPage: true });

check(errors.length === 0, `no page errors ${errors.join(' | ')}`);
await browser.close();
server.close();
process.exit(failures ? 1 : 0);
