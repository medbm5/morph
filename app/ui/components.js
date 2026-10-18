/* Optional components screen: local AI, OCR, background removal, FFmpeg, pandoc…
   Everything downloads from pinned URLs and is SHA-256 verified before use. */
(function (global) {
  'use strict';
  const T = global.__TAURI__;
  const h = global.hudEl;
  const mb = (n) => `${(n / 1e6).toFixed(1)} MB`;

  async function show(root, version, focus) {
    root.classList.add('hud-grid');
    const frame = h('div', { class: 'hud-frame', style: 'grid-template-columns:1fr;max-width:920px;margin:0 auto' });
    ['tl', 'tr', 'bl', 'br'].forEach((c) => frame.append(h('span', { class: `hud-corner ${c}`, 'aria-hidden': 'true' })));
    frame.append(h('header', { class: 'hud-header' },
      h('span', { class: 'hud-blink', 'aria-hidden': 'true' }), h('span', { class: 'hud-brand', text: 'METAMORPHOSIS' }),
      h('span', { class: 'hud-mono', text: `// COMPONENTS · v${version}` })));
    const body = h('section', { class: 'hud-panel' },
      h('h2', { text: 'OPTIONAL COMPONENTS' }),
      h('p', { class: 'ed-note', style: 'margin:0 0 18px', text: 'Downloaded once from pinned releases, checked against SHA-256, then used fully offline.' }));
    const list = h('div', { class: 'hud-list' });
    body.append(list);
    frame.append(body);
    root.append(frame);

    const rows = {};
    async function render() {
      const items = await T.core.invoke('components_status');
      list.textContent = '';
      items.forEach((c) => {
        const bar = h('span', { class: 'hud-bar', style: 'display:block;height:2px;background:var(--hud-line-25);margin-top:8px' });
        const fill = h('i', { style: 'display:block;height:100%;width:0;background:var(--hud-accent)' });
        bar.append(fill);
        const btn = h('button', { class: 'hud-btn' + (c.installed ? '' : ' primary'), type: 'button', text: c.installed ? 'INSTALLED' : `INSTALL · ${mb(c.downloadSize)}` });
        btn.disabled = c.installed || !!c.systemHint;
        const card = h('div', { style: `border:1px solid ${c.name === focus ? 'var(--hud-accent)' : 'var(--hud-line-35)'};padding:14px 16px;display:grid;grid-template-columns:1fr auto;gap:6px 18px;align-items:center` },
          h('div', {},
            h('div', { style: 'font-weight:700;letter-spacing:2px;text-transform:uppercase', text: c.title }),
            h('div', { class: 'ed-note', style: 'margin-top:4px', text: c.description }),
            h('div', { class: 'hud-mono', style: 'margin-top:4px', text: `${c.license}${c.requires.length ? ' · needs ' + c.requires.join(', ') : ''}` }),
            c.systemHint ? h('div', { class: 'hud-warn', style: 'margin-top:4px', text: c.systemHint }) : null,
            bar),
          btn);
        btn.addEventListener('click', async () => {
          btn.disabled = true; btn.textContent = 'DOWNLOADING…';
          try {
            await T.core.invoke('install_component', { name: c.name });
            btn.textContent = 'INSTALLED';
          } catch (e) {
            btn.disabled = false; btn.textContent = 'RETRY';
            card.append(h('div', { class: 'hud-warn', style: 'grid-column:1/-1', text: String(e) }));
          }
        });
        rows[c.name] = fill;
        list.append(card);
      });
    }
    T.event.listen('component://progress', ({ payload }) => {
      const f = rows[payload.name];
      if (f) f.style.width = `${Math.round((payload.done / Math.max(payload.total, 1)) * 100)}%`;
    });
    T.event.listen('component://changed', () => render());
    await render();
  }

  global.MorphComponents = { show };
})(window);
