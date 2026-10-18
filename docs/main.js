(() => {
  'use strict';

  /* ---------- Theme toggle ---------- */
  const root = document.documentElement;
  const toggle = document.querySelector('.theme-toggle');
  const systemDark = () => window.matchMedia('(prefers-color-scheme: dark)').matches;
  const currentTheme = () => root.dataset.theme || (systemDark() ? 'dark' : 'light');
  if (toggle) {
    const label = () => toggle.setAttribute('aria-label', `Switch to ${currentTheme() === 'dark' ? 'light' : 'dark'} theme`);
    label();
    toggle.addEventListener('click', () => {
      const next = currentTheme() === 'dark' ? 'light' : 'dark';
      root.dataset.theme = next;
      try { localStorage.setItem('morph-theme', next); } catch (e) {}
      label();
    });
  }

  /* ---------- HUD ring demo ---------- */
  const FILE = { name: 'Page 001.jpg', stem: 'Page 001', size: '1.1 MB', type: 'JPG' };
  const conv = (fmt, ext) => ({ label: fmt, out: (f) => `${f.stem}.${ext}` });
  const tool = (label, suffix, ext = 'jpg') => ({ label, out: (f) => `${f.stem} ${suffix}.${ext}` });
  const act = (label, msg) => ({ label, verb: 'DONE', out: () => msg });
  const RINGS = {
    convert: [conv('PNG', 'png'), conv('WEBP', 'webp'), conv('AVIF', 'avif'), conv('TIFF', 'tiff'), conv('BMP', 'bmp'), conv('PDF', 'pdf')],
    tools: [tool('Compress', 'Compressed'), tool('Metadata', 'Clean'), tool('Edit', 'Edited'), tool('Annotate', 'Annotated'),
            tool('Add BG', 'With BG', 'png'), tool('Crop', 'Cropped'), tool('Redact', 'Redacted')],
    actions: [act('Share', 'SHARE SHEET OPEN'), act('Float', 'PARKED ON SHELF'), act('Copy', 'COPIED AS FILE'),
              act('Move', 'CHOOSE A FOLDER'), act('Copy path', 'PATH COPIED'), act('Terminal', 'TERMINAL OPEN')],
  };

  const host = document.getElementById('hud-demo');
  const outLine = document.getElementById('hud-output');
  const statusLine = document.getElementById('hud-status');
  const keys = [...document.querySelectorAll('.key')];
  const swatches = [...document.querySelectorAll('.swatch')];
  if (!host || !window.createHudRing) return;

  let ringName = 'convert';
  const hud = window.createHudRing(host, {
    items: RINGS.convert,
    file: FILE,
    label: 'Demo ring. Press 1 to 7 to apply, arrows to move, Escape to cancel.',
    onHover: (item) => { if (item && outLine) outLine.textContent = item.verb ? item.out(FILE) : `→ ${item.out(FILE)}`; },
    onDrop: (item, i, out) => { if (statusLine) statusLine.textContent = `${item.verb === 'DONE' ? '✓ DONE' : '✓ SAVED'} · ${out}`; },
  });

  function setRing(name) {
    if (!RINGS[name] || name === ringName) return;
    ringName = name;
    keys.forEach((k) => k.setAttribute('aria-selected', String(k.dataset.ring === name)));
    hud.setItems(RINGS[name], FILE);
    if (statusLine) statusLine.textContent = 'AWAITING DROP';
  }
  keys.forEach((k) => k.addEventListener('click', () => setRing(k.dataset.ring)));

  swatches.forEach((sw) => sw.addEventListener('click', () => {
    hud.setAccent(sw.dataset.accent);
    const panel = document.querySelector('.hud-panel');
    if (panel) panel.style.setProperty('--hud-accent', sw.dataset.accent);
    swatches.forEach((x) => x.setAttribute('aria-pressed', String(x === sw)));
  }));

  // Tap Shift / Ctrl / Alt alone to switch rings, like the app.
  const KEYMAP = { Shift: 'convert', Control: 'tools', Alt: 'actions' };
  let tapped = null;
  window.addEventListener('keydown', (e) => { tapped = KEYMAP[e.key] && !e.repeat ? e.key : null; });
  window.addEventListener('keyup', (e) => {
    if (tapped && e.key === tapped) { if (e.key === 'Alt') e.preventDefault(); setRing(KEYMAP[e.key]); }
    tapped = null;
  });

  /* ---------- Install tabs ---------- */
  document.querySelectorAll('[data-tabs]').forEach((tabs) => {
    const list = [...tabs.querySelectorAll('[role="tab"]')];
    const panels = list.map((t) => document.getElementById(t.getAttribute('aria-controls')));
    const select = (idx, focus) => {
      list.forEach((t, i) => {
        const on = i === idx;
        t.setAttribute('aria-selected', String(on));
        t.tabIndex = on ? 0 : -1;
        panels[i].hidden = !on;
      });
      if (focus) list[idx].focus();
    };
    // Default to Linux tab for Linux visitors.
    const isLinux = /Linux/.test(navigator.userAgent) && !/Android/.test(navigator.userAgent);
    select(isLinux ? 1 : 0, false);
    list.forEach((t, i) => {
      t.addEventListener('click', () => select(i, false));
      t.addEventListener('keydown', (e) => {
        if (e.key === 'ArrowRight') select((i + 1) % list.length, true);
        if (e.key === 'ArrowLeft') select((i - 1 + list.length) % list.length, true);
      });
    });
  });

  /* ---------- Copy buttons ---------- */
  document.querySelectorAll('.copy').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const text = btn.parentElement.querySelector('code').innerText;
      try {
        await navigator.clipboard.writeText(text);
        btn.textContent = 'Copied';
      } catch (e) {
        btn.textContent = 'Select and copy';
      }
      setTimeout(() => { btn.textContent = 'Copy'; }, 1600);
    });
  });
})();
