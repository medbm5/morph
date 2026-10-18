/* Annotate: arrows, rectangles, ellipses, text, numbered callouts, magnifier, pen, highlighter.
   Shapes live in image pixels; the preview is the exact SVG the export renders. */
(function () {
  'use strict';
  const TOOLS = [
    ['arrow', 'ARROW'], ['rect', 'BOX'], ['ellipse', 'ELLIPSE'], ['text', 'TEXT'], ['callout', '①'],
    ['magnifier', 'MAGNIFY'], ['pen', 'PEN'], ['highlight', 'HIGHLIGHT'],
  ];
  const COLORS = ['#FF3B30', '#FFB347', '#FFE600', '#4CD964', '#4FE3FF', '#5856D6', '#FFFFFF', '#000000'];

  window.MorphEditors.annotate = async (ctx) => {
    const { h, panel, params } = ctx;
    params.shapes = [];
    let tool = 'arrow';
    let color = COLORS[0];
    const wrap = h('div', { class: 'ed-wrap' });
    const toolbar = h('div', { class: 'ed-toolbar', role: 'toolbar', 'aria-label': 'Annotation tools' });
    const stage = h('div', { class: 'ed-stage', style: 'cursor:crosshair;touch-action:none' });
    wrap.append(toolbar, stage);
    panel.centre.append(wrap);
    const pv = createComposePreview(stage, ctx, 'image.annotate');

    TOOLS.forEach(([id, label]) => {
      const b = h('button', { class: 'hud-chip' + (id === tool ? ' on' : ''), type: 'button', text: label });
      b.addEventListener('click', () => { tool = id; toolbar.querySelectorAll('.tool').forEach((c) => c.classList.toggle('on', c === b)); });
      b.classList.add('tool');
      toolbar.append(b);
    });
    const swatches = h('span', { class: 'hud-row', style: 'margin-left:12px' });
    COLORS.forEach((c) => {
      const s = h('button', { class: 'hud-swatch' + (c === color ? ' on' : ''), type: 'button', style: `background:${c}`, 'aria-label': `Colour ${c}` });
      s.addEventListener('click', () => { color = c; swatches.querySelectorAll('.hud-swatch').forEach((x) => x.classList.toggle('on', x === s)); });
      swatches.append(s);
    });
    const textInput = h('input', { type: 'text', placeholder: 'Text to place', 'aria-label': 'Annotation text', style: 'width:160px;background:var(--hud-core);color:var(--hud-text);border:1px solid var(--hud-line-35);padding:5px 8px;font:12px var(--font-readout)' });
    const undo = h('button', { class: 'hud-chip', type: 'button', text: 'UNDO' });
    undo.addEventListener('click', () => { params.shapes.pop(); pv.update(params); });
    toolbar.append(swatches, textInput, undo);

    let drag = null;
    let callouts = 0;
    stage.addEventListener('pointerdown', (e) => {
      const p = pv.toImage(e.clientX, e.clientY);
      stage.setPointerCapture(e.pointerId);
      if (tool === 'text') {
        const text = textInput.value.trim();
        if (!text) { textInput.focus(); return; }
        params.shapes.push({ type: 'text', x: p.x, y: p.y, text, color });
        pv.update(params);
        return;
      }
      if (tool === 'callout') {
        callouts += 1;
        params.shapes.push({ type: 'callout', x: p.x, y: p.y, n: callouts, color });
        pv.update(params);
        return;
      }
      const shape = { type: tool, color };
      if (tool === 'arrow') Object.assign(shape, { x1: p.x, y1: p.y, x2: p.x, y2: p.y });
      if (tool === 'rect' || tool === 'ellipse') Object.assign(shape, { x: p.x, y: p.y, w: 1, h: 1 });
      if (tool === 'magnifier') Object.assign(shape, { sx: p.x, sy: p.y, x: p.x, y: p.y, r: 1, zoom: 2 });
      if (tool === 'pen' || tool === 'highlight') shape.points = [[p.x, p.y]];
      if (tool === 'highlight') delete shape.color;
      params.shapes.push(shape);
      drag = { shape, start: p };
    });
    stage.addEventListener('pointermove', (e) => {
      if (!drag) return;
      const p = pv.toImage(e.clientX, e.clientY), s = drag.shape, o = drag.start;
      if (s.type === 'arrow') { s.x2 = p.x; s.y2 = p.y; }
      if (s.type === 'rect' || s.type === 'ellipse') { s.x = Math.min(o.x, p.x); s.y = Math.min(o.y, p.y); s.w = Math.abs(p.x - o.x); s.h = Math.abs(p.y - o.y); }
      // Magnifier: press on the detail, drag to where the lens should sit.
      if (s.type === 'magnifier') { s.x = p.x; s.y = p.y; s.r = Math.max(30, Math.hypot(p.x - o.x, p.y - o.y) / 2); }
      if (s.points) s.points.push([p.x, p.y]);
      pv.update(params);
    });
    stage.addEventListener('pointerup', () => { drag = null; });

    const save = h('button', { class: 'hud-btn primary', type: 'button', text: 'SAVE ANNOTATED' });
    save.addEventListener('click', () => ctx.run());
    panel.right.append(h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save));
    window.addEventListener('keydown', (e) => { if ((e.ctrlKey || e.metaKey) && e.key === 'z') undo.click(); });
    pv.update(params);
  };
})();
