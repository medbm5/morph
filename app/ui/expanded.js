/* Metamorphosis — expanded mode controller: editors, generic parameter forms. */
(() => {
  'use strict';
  const T = window.__TAURI__;
  const invoke = T.core.invoke;
  const h = window.hudEl;
  const root = document.getElementById('root');
  const base = (p) => String(p).split(/[\\/]/).pop();

  let current = null;

  /** A failed job's full message, e.g. a recipe command's stderr. */
  function showError(req, version) {
    const L = window.MorphLook;
    root.classList.add('hud-grid');
    const frame = h('div', { class: 'hud-frame', style: 'grid-template-columns:1fr;max-width:900px;margin:0 auto' });
    ['tl', 'tr', 'bl', 'br'].forEach((c) => frame.append(h('span', { class: `hud-corner ${c}`, 'aria-hidden': 'true' })));
    frame.append(h('header', { class: 'hud-header' },
      h('span', { class: 'hud-blink', 'aria-hidden': 'true' }), h('span', { class: 'hud-brand', text: 'METAMORPHOSIS' }),
      h('span', { class: 'hud-mono', text: `// ${L.t('error.title', 'FAILED')} · v${version}` })));
    const close = h('button', { class: 'hud-btn', type: 'button', text: L.t('error.close', 'CLOSE') });
    close.addEventListener('click', () => T.window.getCurrentWindow().hide());
    frame.append(h('section', { class: 'hud-panel' },
      h('h2', { text: req.op || 'JOB' }),
      h('pre', { class: 'hud-mono', style: 'white-space:pre-wrap;max-height:60vh;overflow:auto;border:1px solid var(--hud-line-35);padding:12px;user-select:text', text: req.text || '' }),
      h('div', { class: 'hud-row', style: 'margin-top:12px' }, close)));
    root.append(frame);
  }

  async function show(req) {
    if (!req) return;
    root.textContent = '';
    const version = await T.app.getVersion();
    if (req.mode === 'components') { window.MorphComponents.show(root, version, req.op); return; }
    if (req.mode === 'history') { window.MorphHistory.show(root, version); return; }
    if (req.mode === 'error') { showError(req, version); return; }
    if (req.mode === 'settings') { window.MorphSettings.show(root, version); return; }
    if (req.mode === 'wayland') { window.MorphWayland.show(root, version); return; }
    if (req.mode === 'ring') { showRing(req, version); return; }
    const panel = createHudPanel(root, { version, footer: false, title: 'OPERATION', wide: true });
    if (req.mode !== 'editor') return;

    const spec = await invoke('op_spec', { op: req.op });
    const info = await invoke('describe', { paths: req.paths });
    const isImage = /IMAGE/.test(info.type);
    panel.setTarget({
      name: info.name, type: info.type, size: info.size, dim: info.dim,
      thumb: isImage && req.paths.length === 1 ? T.core.convertFileSrc(req.paths[0]) : false,
    });
    panel.setActions([{ label: spec.label }], { onPick: () => run() });
    panel.setHot(0);
    panel.setOutput('');

    const params = MorphEditorKit.defaults(spec);
    const ctx = {
      panel, spec, paths: req.paths, info, params, h,
      setParam(k, v) { params[k] = v; },
      src: (p) => T.core.convertFileSrc(p),
      run: (extra) => run(extra),
      invoke,
    };
    current = ctx;

    const editor = spec.editor && window.MorphEditors[spec.editor];
    if (editor) {
      await editor(ctx);
    } else {
      genericForm(ctx);
    }

    async function run(extra) {
      const p = Object.assign({}, params, extra || {});
      panel.setStatus('WORKING…');
      const id = await invoke('run_op', { op: spec.id, paths: ctx.paths, params: p });
      const saved = [];
      const errors = [];
      const un = await T.event.listen('job://event', ({ payload: e }) => {
        if (e.job !== id) return;
        if (e.event === 'progress') panel.setStatus(`WORKING · ${Math.round(e.fraction * 100)} %`);
        if (e.event === 'saved') saved.push(e.path);
        if (e.event === 'failed') errors.push(e.error);
        if (e.event === 'finished') {
          un();
          if (errors.length && !saved.length) { panel.setStatus(`✗ ${errors[0]}`, 'error'); return; }
          panel.setStatus(`✓ SAVED · ${saved.map(base).join(', ')}`);
          panel.setOutput(saved.map(base).join(', '));
          if (saved[0]) {
            panel.right.textContent = '';
            const b = h('button', { class: 'hud-btn small', type: 'button', text: 'SHOW IN FOLDER' });
            b.addEventListener('click', () => invoke('reveal', { path: saved[0] }));
            panel.right.append(b);
          }
        }
      });
    }
  }

  /** The ring in a window (file-manager entries, clipboard images, keyboard use). */
  async function showRing(req, version) {
    const panel = createHudPanel(root, { version, footer: false });
    const info = await invoke('describe', { paths: req.paths });
    const isImage = /IMAGE/.test(info.type);
    panel.setTarget({ name: info.name, type: info.type, size: info.size, dim: info.dim,
      thumb: isImage && req.paths.length === 1 ? T.core.convertFileSrc(req.paths[0]) : false });
    const tabs = h('div', { class: 'hud-row', role: 'tablist', style: 'margin-bottom:10px' });
    const mount = h('div');
    panel.centre.append(h('div', { style: 'width:min(600px,100%)' }, tabs, mount));
    let kind = req.ring || 'convert';
    let items = [];
    const ring = createHudRing(mount, {
      items: [], ringName: '', label: 'Operations',
      onHover: (it, i) => { panel.setHot(i); panel.setOutput(it && it.out ? it.out : ''); },
      onSelect: (it) => pick(it),
    });
    async function load() {
      tabs.querySelectorAll('.hud-chip').forEach((b) => b.classList.toggle('on', b.dataset.k === kind));
      const r = await invoke('ring_items', { paths: req.paths, ring: kind });
      items = r.items;
      ring.setItems(items, { ringName: kind.toUpperCase(), file: { size: r.file.size, type: r.file.type }, animate: 'switch' });
      panel.setActions(items, { onPick: (it) => pick(it), onHover: (it, i) => ring.setHot(i) });
      ring.focus();
    }
    ['convert', 'tools', 'actions'].forEach((k, i) => {
      const b = h('button', { class: 'hud-chip', type: 'button', role: 'tab', text: `${['SHIFT', 'CTRL', 'ALT'][i]} · ${k.toUpperCase()}` });
      b.dataset.k = k;
      b.addEventListener('click', () => { kind = k; load(); });
      tabs.append(b);
    });
    async function pick(it) {
      if (it.disabled) { invoke('open_components', { focus: it.component || null }); return; }
      if (it.editor) { show({ mode: 'editor', op: it.id, paths: req.paths }); return; }
      panel.setStatus('WORKING…');
      const id = await invoke('run_op', { op: it.id, paths: req.paths });
      const saved = [];
      const un = await T.event.listen('job://event', ({ payload: e }) => {
        if (e.job !== id) return;
        if (e.event === 'progress') panel.setStatus(`WORKING · ${Math.round(e.fraction * 100)} %`);
        if (e.event === 'saved') saved.push(e.path);
        if (e.event === 'failed') panel.setStatus(`✗ ${e.error}`, 'error');
        if (e.event === 'finished') {
          un();
          if (saved.length) { panel.setStatus(`✓ SAVED · ${saved.map(base).join(', ')}`); ring.saved(base(saved[0])); }
          else if (!e.failed) { panel.setStatus('✓ DONE'); ring.saved(it.label, { verb: 'DONE' }); }
        }
      });
    }
    // Keys: Shift/Ctrl/Alt switch rings; 1–7, arrows and Enter work on the focused ring.
    window.addEventListener('keydown', (e) => {
      const k = { Shift: 'convert', Control: 'tools', Alt: 'actions' }[e.key];
      if (k && k !== kind) { e.preventDefault(); kind = k; load(); }
    });
    await load();
  }

  function genericForm(ctx) {
    const form = h('div', { class: 'ed-form' });
    ctx.spec.params.forEach((p) => {
      const f = MorphEditorKit.paramField(p, ctx.params[p.key], (v) => ctx.setParam(p.key, v));
      if (f) form.append(f);
    });
    if (!ctx.spec.params.length) form.append(h('p', { class: 'ed-note', text: 'This operation has no options.' }));
    const go = h('button', { class: 'hud-btn primary', type: 'button', text: `APPLY · ${ctx.spec.label.toUpperCase()}` });
    go.addEventListener('click', () => ctx.run());
    form.append(h('div', { class: 'ed-actions' }, go));
    ctx.panel.centre.append(form);
  }

  T.event.listen('expanded://request', ({ payload }) => show(payload));
  invoke('expanded_request').then(show);
  window.MorphExpanded = { current: () => current };
})();
