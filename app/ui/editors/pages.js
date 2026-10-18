/* PDF page organizer (reorder by dragging, rotate, duplicate, delete) and merge order. */
(function () {
  'use strict';
  const T = window.__TAURI__;
  const base = (p) => String(p).split(/[\\/]/).pop();

  function grid(h) {
    return h('div', { style: 'display:grid;grid-template-columns:repeat(auto-fill,minmax(130px,1fr));gap:14px;align-content:start;overflow:auto;padding:12px;position:absolute;inset:0;box-sizing:border-box' });
  }

  /** A draggable card list: `items` is mutated in place on reorder. */
  function sortable(container, items, renderCard, onChange) {
    let from = -1;
    function render() {
      container.textContent = '';
      items.forEach((it, i) => {
        const card = renderCard(it, i);
        card.draggable = true;
        card.addEventListener('dragstart', () => { from = i; card.style.opacity = '.4'; });
        card.addEventListener('dragend', () => { card.style.opacity = ''; });
        card.addEventListener('dragover', (e) => e.preventDefault());
        card.addEventListener('drop', (e) => {
          e.preventDefault();
          if (from < 0 || from === i) return;
          const [moved] = items.splice(from, 1);
          items.splice(i, 0, moved);
          from = -1; render(); onChange();
        });
        container.append(card);
      });
    }
    return render;
  }

  window.MorphEditors.pages = async (ctx) => {
    const { h, panel, params } = ctx;
    const thumbs = await ctx.invoke('pdf_thumbs', { path: ctx.paths[0], width: 220 });
    const pages = thumbs.map((t, i) => ({ page: i + 1, rotate: 0, thumb: t }));
    const box = grid(h);
    panel.centre.append(h('div', { class: 'ed-wrap' }, h('div', { class: 'ed-stage', style: 'display:block' }, box)));
    const sync = () => {
      params.pages = pages.map((p) => `${p.page}${p.rotate ? ':' + p.rotate : ''}`).join(',');
      panel.setOutput(`${pages.length} page(s) · ${params.pages}`);
    };
    const render = sortable(box, pages, (p, i) => {
      const img = h('img', { src: T.core.convertFileSrc(p.thumb), alt: `Page ${p.page}`, style: `width:100%;display:block;transform:rotate(${p.rotate}deg);transition:transform .15s` });
      const tools = h('div', { class: 'hud-row', style: 'justify-content:center;margin-top:6px;gap:4px' });
      const tb = (label, title, fn) => { const b = h('button', { class: 'hud-chip', type: 'button', text: label, title }); b.addEventListener('click', fn); tools.append(b); };
      tb('⟲', 'Rotate left', () => { p.rotate = (p.rotate + 270) % 360; render(); sync(); });
      tb('⟳', 'Rotate right', () => { p.rotate = (p.rotate + 90) % 360; render(); sync(); });
      tb('⧉', 'Duplicate', () => { pages.splice(i + 1, 0, { ...p }); render(); sync(); });
      tb('✕', 'Delete', () => { if (pages.length > 1) { pages.splice(i, 1); render(); sync(); } });
      return h('div', { style: 'border:1px solid var(--hud-line-35);padding:8px;background:rgba(4,10,18,.6);cursor:grab' },
        h('div', { style: 'aspect-ratio:3/4;display:grid;place-items:center;overflow:hidden;background:#fff' }, img),
        h('div', { class: 'hud-mono', style: 'text-align:center;margin-top:6px', text: `${String(i + 1).padStart(2, '0')} · page ${p.page}` }),
        tools);
    }, sync);
    render(); sync();
    const save = h('button', { class: 'hud-btn primary', type: 'button', text: 'SAVE ORGANIZED' });
    save.addEventListener('click', () => ctx.run());
    panel.right.append(h('p', { class: 'ed-note', text: 'Drag pages to reorder. Rotate, duplicate or delete with the buttons under each page.' }),
      h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save));
  };

  window.MorphEditors['pdf-merge'] = async (ctx) => {
    const { h, panel } = ctx;
    const docs = await Promise.all(ctx.paths.map(async (p) => ({ path: p, thumbs: await ctx.invoke('pdf_thumbs', { path: p, width: 220 }) })));
    const box = grid(h);
    panel.centre.append(h('div', { class: 'ed-wrap' }, h('div', { class: 'ed-stage', style: 'display:block' }, box)));
    const sync = () => {
      ctx.paths.splice(0, ctx.paths.length, ...docs.map((d) => d.path));
      const total = docs.reduce((n, d) => n + d.thumbs.length, 0);
      panel.setOutput(`${docs.length} documents · ${total} pages`);
    };
    const render = sortable(box, docs, (d, i) => h('div', { style: 'border:1px solid var(--hud-line-35);padding:8px;background:rgba(4,10,18,.6);cursor:grab' },
      h('div', { style: 'aspect-ratio:3/4;display:grid;place-items:center;overflow:hidden;background:#fff' },
        d.thumbs[0] ? h('img', { src: T.core.convertFileSrc(d.thumbs[0]), alt: base(d.path), style: 'width:100%' }) : null),
      h('div', { class: 'hud-mono', style: 'margin-top:6px;overflow-wrap:anywhere', text: `${String(i + 1).padStart(2, '0')} · ${base(d.path)}` }),
      h('div', { class: 'hud-mono', style: 'opacity:.7', text: `${d.thumbs.length} page(s)` })), sync);
    render(); sync();
    const save = h('button', { class: 'hud-btn primary', type: 'button', text: 'MERGE IN THIS ORDER' });
    save.addEventListener('click', () => ctx.run());
    panel.right.append(h('p', { class: 'ed-note', text: 'Drag documents to change the order.' }), h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save));
  };
})();
