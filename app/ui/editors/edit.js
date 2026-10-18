/* Photo editor: sliders drive the WebGL2 preview; Save runs the identical Rust pipeline. */
(function () {
  'use strict';
  const GROUPS = [
    ['LIGHT', [['exposure', 'Exposure', -3, 3], ['contrast', 'Contrast', -1, 1]]],
    ['COLOUR', [['temperature', 'Temperature', -1, 1], ['tint', 'Tint', -1, 1], ['saturation', 'Saturation', -1, 1], ['vibrance', 'Vibrance', -1, 1]]],
    ['DETAIL', [['detail', 'Detail', 0, 1], ['clarity', 'Clarity', 0, 1], ['dehaze', 'Dehaze', 0, 1], ['noise_reduction', 'Noise reduction', 0, 1], ['grain', 'Grain', 0, 1]]],
  ];

  window.MorphEditors.edit = async (ctx) => {
    const { h, panel, params } = ctx;
    const path = ctx.paths[0];
    const wrap = h('div', { class: 'ed-wrap' });
    const stage = h('div', { class: 'ed-stage' });
    const canvas = h('canvas', { 'aria-label': 'Edited preview' });
    stage.append(canvas);
    const toolbar = h('div', { class: 'ed-toolbar' });
    const compare = h('button', { class: 'hud-chip', type: 'button', text: 'HOLD TO COMPARE' });
    const reset = h('button', { class: 'hud-chip', type: 'button', text: 'RESET' });
    toolbar.append(compare, reset);
    wrap.append(toolbar, stage);
    panel.centre.append(wrap);

    const img = new Image();
    // The asset protocol is another origin: request CORS so WebGL may read the pixels.
    img.crossOrigin = 'anonymous';
    img.src = ctx.src(path);
    await new Promise((r, j) => { img.onload = r; img.onerror = () => j(new Error('This format can\'t be previewed here; Save still works.')); }).catch((e) => {
      stage.append(h('p', { class: 'ed-note', text: e.message }));
    });

    let renderer = null;
    try {
      renderer = createAdjustRenderer(canvas);
      // Very large photos: preview a downscaled copy (export always runs at full size).
      let source = img;
      const max = Math.min(renderer.maxSize, 4096);
      if (img.naturalWidth > max || img.naturalHeight > max) {
        const s = max / Math.max(img.naturalWidth, img.naturalHeight);
        source = await createImageBitmap(img, { resizeWidth: Math.round(img.naturalWidth * s), resizeHeight: Math.round(img.naturalHeight * s), resizeQuality: 'high' });
      }
      if (img.naturalWidth) renderer.setImage(source);
    } catch (e) {
      stage.append(h('p', { class: 'ed-note', text: `Live preview unavailable (${e.message}). Values still apply on save.` }));
    }
    // Fill the stage, letterboxed: the canvas keeps the photo's full resolution.
    canvas.style.cssText = 'position:absolute;inset:8px;width:calc(100% - 16px);height:calc(100% - 16px);object-fit:contain';

    let raf = 0;
    const draw = (p) => { if (!renderer || !img.naturalWidth) return; cancelAnimationFrame(raf); raf = requestAnimationFrame(() => renderer.render(p)); };
    const zero = Object.fromEntries(ADJUST_KEYS.map((k) => [k, 0]));

    // Sliders go in the right column under the actions.
    const inputs = {};
    GROUPS.forEach(([title, items]) => {
      panel.right.append(h('h2', { class: 'hud-panel', style: 'font-size:11px;letter-spacing:3px;color:var(--hud-line);margin:22px 0 8px', text: title }));
      items.forEach(([key, label, min, max]) => {
        const range = h('input', { type: 'range', min, max, step: 0.01, 'aria-label': label });
        range.value = params[key] || 0;
        const val = h('span', { class: 'hud-mono', text: (+range.value).toFixed(2) });
        range.addEventListener('input', () => { params[key] = +range.value; val.textContent = (+range.value).toFixed(2); draw(params); });
        range.addEventListener('dblclick', () => { range.value = 0; range.dispatchEvent(new Event('input')); });
        inputs[key] = range;
        panel.right.append(h('label', { class: 'hud-field' }, h('span', { class: 'hud-row', style: 'justify-content:space-between' }, h('span', { text: label }), val), range));
      });
    });
    reset.addEventListener('click', () => Object.values(inputs).forEach((r) => { r.value = 0; r.dispatchEvent(new Event('input')); }));
    compare.addEventListener('pointerdown', () => draw(zero));
    ['pointerup', 'pointerleave'].forEach((ev) => compare.addEventListener(ev, () => draw(params)));
    const save = h('button', { class: 'hud-btn primary', type: 'button', text: 'SAVE EDITED' });
    save.addEventListener('click', () => ctx.run());
    panel.right.append(h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save));
    draw(params);
  };
})();
