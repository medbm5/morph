/* Collage: grid, row, column or featured layout over the whole selection. */
(function () {
  'use strict';
  const LAYOUTS = ['grid', 'row', 'column', 'featured'];

  window.MorphEditors.collage = async (ctx) => {
    const { h, panel, params } = ctx;
    const wrap = h('div', { class: 'ed-wrap' });
    const toolbar = h('div', { class: 'ed-toolbar' });
    const stage = h('div', { class: 'ed-stage' });
    wrap.append(toolbar, stage);
    panel.centre.append(wrap);
    const pv = createComposePreview(stage, ctx, 'image.collage');
    const refresh = () => pv.update(params);

    LAYOUTS.forEach((l) => {
      const b = h('button', { class: 'hud-chip' + (params.layout === l ? ' on' : ''), type: 'button', text: l.toUpperCase() });
      b.addEventListener('click', () => { params.layout = l; toolbar.querySelectorAll('.hud-chip').forEach((c) => c.classList.toggle('on', c === b)); refresh(); });
      toolbar.append(b);
    });
    ctx.spec.params.filter((p) => p.key !== 'layout').forEach((p) => {
      const f = MorphEditorKit.paramField(p, params[p.key], (v) => { params[p.key] = v; refresh(); });
      if (f) panel.right.append(f);
    });
    if (ctx.paths.length < 2) panel.right.prepend(h('p', { class: 'ed-note', text: 'Drag two or more images to build a collage.' }));
    const save = h('button', { class: 'hud-btn primary', type: 'button', text: 'SAVE COLLAGE' });
    save.addEventListener('click', () => ctx.run());
    panel.right.append(h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save));
    refresh();
  };
})();
