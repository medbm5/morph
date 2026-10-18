/* Metamorphosis — expanded mode controller: editors, generic parameter forms. */
(() => {
  'use strict';
  const T = window.__TAURI__;
  const invoke = T.core.invoke;
  const h = window.hudEl;
  const root = document.getElementById('root');
  const base = (p) => String(p).split(/[\\/]/).pop();

  let current = null;

  async function show(req) {
    if (!req) return;
    root.textContent = '';
    const version = await T.app.getVersion();
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
      const id = await invoke('run_op', { op: spec.id, paths: req.paths, params: p });
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
