/* Job history: Show in folder, Undo, Run again, and Save a drag-chain as a recipe. */
(function (global) {
  'use strict';
  const T = global.__TAURI__;
  const h = global.hudEl;
  const base = (p) => String(p).split(/[\\/]/).pop();
  const when = (t) => new Date(t * 1000).toLocaleString();

  async function show(root, version) {
    root.classList.add('hud-grid');
    const frame = h('div', { class: 'hud-frame', style: 'grid-template-columns:1fr;max-width:1000px;margin:0 auto' });
    ['tl', 'tr', 'bl', 'br'].forEach((c) => frame.append(h('span', { class: `hud-corner ${c}`, 'aria-hidden': 'true' })));
    frame.append(h('header', { class: 'hud-header' },
      h('span', { class: 'hud-blink', 'aria-hidden': 'true' }), h('span', { class: 'hud-brand', text: 'METAMORPHOSIS' }),
      h('span', { class: 'hud-mono', text: `// HISTORY · v${version}` })));
    const body = h('section', { class: 'hud-panel' }, h('h2', { text: 'RECENT JOBS' }));
    const list = h('div', { class: 'hud-list' });
    body.append(list);
    frame.append(body);
    root.append(frame);

    async function render() {
      const items = await T.core.invoke('history');
      const byId = Object.fromEntries(items.map((e) => [e.id, e]));
      list.textContent = '';
      if (!items.length) list.append(h('p', { class: 'ed-note', text: 'Nothing yet. Drag a file, tap Shift, drop on a segment.' }));
      items.forEach((e) => {
        const depth = (() => { let n = 1, c = e; while (c.parent && byId[c.parent]) { n++; c = byId[c.parent]; } return n; })();
        const row = h('div', { style: `border:1px solid var(--hud-line-35);padding:12px 14px;display:grid;grid-template-columns:1fr auto;gap:6px 14px;align-items:center;${e.undone ? 'opacity:.45' : ''}` });
        row.append(h('div', {},
          h('div', { style: 'font-weight:700;letter-spacing:2px;text-transform:uppercase', text: `${e.label || e.op}${depth > 1 ? ` · CHAIN OF ${depth}` : ''}${e.undone ? ' · UNDONE' : ''}` }),
          h('div', { class: 'hud-mono', style: 'margin-top:4px', text: `${e.inputs.map(base).join(', ')} → ${e.outputs.map(base).join(', ')}` }),
          h('div', { class: 'hud-mono', style: 'margin-top:2px;opacity:.7', text: when(e.at) })));
        const actions = h('div', { class: 'hud-row' });
        const btn = (label, fn) => { const b = h('button', { class: 'hud-btn small', type: 'button', text: label }); b.addEventListener('click', fn); actions.append(b); return b; };
        if (!e.undone && e.outputs[0]) btn('SHOW', () => T.core.invoke('reveal', { path: e.outputs[0] }));
        if (!e.undone) btn('RUN AGAIN', () => T.core.invoke('run_op', { op: e.op, paths: e.inputs, params: e.params }));
        if (depth > 1) {
          btn('SAVE AS RECIPE', async () => {
            const name = actionsName.value.trim() || `Chain ${e.id}`;
            try {
              const path = await T.core.invoke('save_recipe', { job: e.id, name });
              row.append(h('div', { class: 'hud-mono', style: 'grid-column:1/-1;color:var(--hud-accent)', text: `✓ saved ${path} — it now appears in the ring` }));
            } catch (err) { row.append(h('div', { class: 'hud-warn', style: 'grid-column:1/-1', text: String(err) })); }
          });
          var actionsName = h('input', { type: 'text', placeholder: 'Recipe name', 'aria-label': 'Recipe name', style: 'width:150px;background:var(--hud-core);color:var(--hud-text);border:1px solid var(--hud-line-35);padding:5px 8px;font:12px var(--font-readout)' });
          actions.prepend(actionsName);
        }
        row.append(actions);
        list.append(row);
      });
    }
    const undo = h('button', { class: 'hud-btn', type: 'button', text: 'UNDO LAST JOB' });
    undo.addEventListener('click', async () => { await T.core.invoke('undo_last'); render(); });
    body.insertBefore(h('div', { class: 'ed-actions', style: 'margin-bottom:14px' }, undo), list);
    T.event.listen('history://recorded', () => render());
    await render();
  }

  global.MorphHistory = { show };
})(window);
