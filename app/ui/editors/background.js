/* Add BG: solid, gradient, picture or blurred backgrounds; padding, radius, shadow, size presets. */
(function () {
  'use strict';
  const SWATCHES = [
    ['#4FE3FF', '#FFB347'], ['#FF5EA8', '#5856D6'], ['#9CFF6E', '#4FE3FF'], ['#0F2027', '#2C5364'],
    ['#F4F1E8', '#CFD8DC'], ['#FFFFFF', '#FFFFFF'], ['#000000', '#1C1C1E'], ['#FFE600', '#FF3B30'],
  ];
  const SIZES = ['auto', '1:1', '4:5', '16:9', '9:16', '1080x1080', '1080x1350', '1920x1080'];

  window.MorphEditors.background = async (ctx) => {
    const { h, panel, params } = ctx;
    const wrap = h('div', { class: 'ed-wrap' });
    const stage = h('div', { class: 'ed-stage' });
    wrap.append(stage);
    panel.centre.append(wrap);
    const pv = createComposePreview(stage, ctx, 'image.background');
    const refresh = () => pv.update(params);

    const sw = h('div', { class: 'hud-row', role: 'group', 'aria-label': 'Swatches' });
    SWATCHES.forEach(([a, b]) => {
      const s = h('button', { class: 'hud-swatch', type: 'button', style: `background:linear-gradient(135deg,${a},${b})`, 'aria-label': `${a} to ${b}` });
      s.addEventListener('click', () => {
        params.color = a; params.color2 = b; params.kind = a === b ? 'solid' : 'gradient';
        form.querySelector('[data-k=kind]').value = params.kind;
        form.querySelector('[data-k=color]').value = a;
        form.querySelector('[data-k=color2]').value = b;
        refresh();
      });
      sw.append(s);
    });
    const sizes = h('div', { class: 'hud-row' });
    SIZES.forEach((sz) => {
      const b = h('button', { class: 'hud-chip' + (params.size === sz ? ' on' : ''), type: 'button', text: sz.toUpperCase() });
      b.addEventListener('click', () => { params.size = sz; sizes.querySelectorAll('.hud-chip').forEach((c) => c.classList.toggle('on', c === b)); refresh(); });
      sizes.append(b);
    });

    const form = h('div');
    ctx.spec.params.filter((p) => !['size', 'image'].includes(p.key)).forEach((p) => {
      const f = MorphEditorKit.paramField(p, params[p.key], (v) => { params[p.key] = v; refresh(); });
      const ctl = f && f.querySelector('input,select');
      if (ctl) ctl.dataset.k = p.key;
      if (f) form.append(f);
    });
    const pick = h('label', { class: 'hud-field' }, h('span', { text: 'Background picture (path)' }));
    const pickInput = h('input', { type: 'text', placeholder: 'C:\\Pictures\\texture.jpg' });
    pickInput.addEventListener('change', () => { params.image = pickInput.value.trim(); if (params.image) { params.kind = 'image'; form.querySelector('[data-k=kind]').value = 'image'; } refresh(); });
    pick.append(pickInput);

    panel.right.append(h('h2', { class: 'hud-panel', style: 'font-size:11px;letter-spacing:3px;color:var(--hud-line);margin:22px 0 8px', text: 'STYLE' }), sw, h('div', { style: 'height:12px' }), sizes, h('div', { style: 'height:12px' }), form, pick);
    const save = h('button', { class: 'hud-btn primary', type: 'button', text: 'SAVE WITH BG' });
    save.addEventListener('click', () => ctx.run());
    panel.right.append(h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save));
    refresh();
  };
})();
