/* Crop editor: drag/resize a rectangle over the image, ratio presets, exact W×H output. */
(function () {
  'use strict';
  const RATIOS = ['free', '1:1', '4:5', '16:9', '3:2', '4:3', '9:16', '2:3'];

  window.MorphEditors.crop = async (ctx) => {
    const { h, panel, params } = ctx;
    const path = ctx.paths[0];
    const [iw, ih] = (await ctx.invoke('image_size', { path })) || [0, 0];
    if (!iw) { panel.centre.append(h('p', { class: 'ed-note', text: 'Preview unavailable for this format; enter the crop numerically.' })); }

    const wrap = h('div', { class: 'ed-wrap' });
    const toolbar = h('div', { class: 'ed-toolbar' });
    const stage = h('div', { class: 'ed-stage' });
    const canvas = h('canvas', { 'aria-label': 'Crop preview' });
    stage.append(canvas);
    const fields = h('div', { class: 'hud-row' });
    const num = (label, key) => {
      const i = h('input', { type: 'number', min: 0, style: 'width:90px', 'aria-label': label });
      const f = h('label', { class: 'hud-field' }, h('span', { text: label }), i);
      fields.append(f);
      return i;
    };
    const fx = num('X', 'x'), fy = num('Y', 'y'), fw = num('W', 'w'), fh = num('H', 'h');
    const ow = num('OUT W', 'width'), oh = num('OUT H', 'height');
    const apply = h('button', { class: 'hud-btn primary', type: 'button', text: 'CROP' });
    const actions = h('div', { class: 'ed-actions' }, apply);
    wrap.append(toolbar, stage, fields, actions);
    panel.centre.append(wrap);

    let ratio = null;
    let rect = { x: 0, y: 0, w: iw, h: ih };
    RATIOS.forEach((r) => {
      const b = h('button', { class: 'hud-chip' + (r === 'free' ? ' on' : ''), type: 'button', text: r.toUpperCase() });
      b.addEventListener('click', () => {
        toolbar.querySelectorAll('.hud-chip').forEach((c) => c.classList.toggle('on', c === b));
        ratio = r === 'free' ? null : r.split(':').map(Number);
        if (ratio) rect = centred(ratio);
        sync(); draw();
      });
      toolbar.append(b);
    });

    function centred([a, b]) {
      const want = a / b;
      let w = iw, hh = ih;
      if (iw / ih > want) w = Math.round(ih * want); else hh = Math.round(iw / want);
      return { x: Math.round((iw - w) / 2), y: Math.round((ih - hh) / 2), w, h: hh };
    }

    const img = new Image();
    img.src = ctx.src(path);
    await new Promise((r) => { img.onload = r; img.onerror = r; });

    let scale = 1;
    function layout() {
      const box = stage.getBoundingClientRect();
      scale = Math.min((box.width - 16) / iw, (box.height - 16) / ih, 1) || 1;
      canvas.width = Math.round(iw * scale);
      canvas.height = Math.round(ih * scale);
    }
    function draw() {
      const g = canvas.getContext('2d');
      g.clearRect(0, 0, canvas.width, canvas.height);
      if (img.naturalWidth) g.drawImage(img, 0, 0, canvas.width, canvas.height);
      g.fillStyle = 'rgba(4,10,18,.62)';
      const r = { x: rect.x * scale, y: rect.y * scale, w: rect.w * scale, h: rect.h * scale };
      g.fillRect(0, 0, canvas.width, r.y);
      g.fillRect(0, r.y + r.h, canvas.width, canvas.height - r.y - r.h);
      g.fillRect(0, r.y, r.x, r.h);
      g.fillRect(r.x + r.w, r.y, canvas.width - r.x - r.w, r.h);
      const accent = getComputedStyle(document.documentElement).getPropertyValue('--hud-accent').trim() || '#FFB347';
      g.strokeStyle = accent; g.lineWidth = 2; g.strokeRect(r.x + 1, r.y + 1, r.w - 2, r.h - 2);
      g.strokeStyle = 'rgba(79,227,255,.5)'; g.lineWidth = 1;
      for (let i = 1; i < 3; i++) {
        g.beginPath(); g.moveTo(r.x + r.w * i / 3, r.y); g.lineTo(r.x + r.w * i / 3, r.y + r.h); g.stroke();
        g.beginPath(); g.moveTo(r.x, r.y + r.h * i / 3); g.lineTo(r.x + r.w, r.y + r.h * i / 3); g.stroke();
      }
      g.fillStyle = accent;
      corners(r).forEach(([cx, cy]) => g.fillRect(cx - 5, cy - 5, 10, 10));
    }
    const corners = (r) => [[r.x, r.y], [r.x + r.w, r.y], [r.x, r.y + r.h], [r.x + r.w, r.y + r.h]];
    function sync() {
      fx.value = rect.x; fy.value = rect.y; fw.value = rect.w; fh.value = rect.h;
      panel.setOutput(`${base(path)} → ${rect.w} × ${rect.h}${ow.value && oh.value ? ` → ${ow.value} × ${oh.value}` : ''}`);
    }
    const base = (p) => String(p).split(/[\\/]/).pop();
    const clamp = () => {
      rect.w = Math.max(1, Math.min(rect.w, iw)); rect.h = Math.max(1, Math.min(rect.h, ih));
      rect.x = Math.max(0, Math.min(rect.x, iw - rect.w)); rect.y = Math.max(0, Math.min(rect.y, ih - rect.h));
    };
    [fx, fy, fw, fh].forEach((i) => i.addEventListener('change', () => {
      rect = { x: +fx.value, y: +fy.value, w: +fw.value, h: +fh.value }; clamp(); sync(); draw();
    }));
    [ow, oh].forEach((i) => i.addEventListener('change', sync));

    // Pointer: drag inside to move, drag a corner to resize (keeping the ratio if one is set).
    let drag = null;
    canvas.addEventListener('pointerdown', (e) => {
      const p = toImage(e);
      const r = { x: rect.x, y: rect.y, w: rect.w, h: rect.h };
      const hit = corners(r).findIndex(([cx, cy]) => Math.abs(cx - p.x) < 12 / scale && Math.abs(cy - p.y) < 12 / scale);
      drag = { start: p, orig: { ...rect }, corner: hit };
      canvas.setPointerCapture(e.pointerId);
    });
    canvas.addEventListener('pointermove', (e) => {
      if (!drag) return;
      const p = toImage(e), dx = p.x - drag.start.x, dy = p.y - drag.start.y, o = drag.orig;
      if (drag.corner < 0) { rect = { ...o, x: o.x + dx, y: o.y + dy }; }
      else {
        const left = drag.corner % 2 === 0, top = drag.corner < 2;
        let x0 = left ? o.x + dx : o.x, x1 = left ? o.x + o.w : o.x + o.w + dx;
        let y0 = top ? o.y + dy : o.y, y1 = top ? o.y + o.h : o.y + o.h + dy;
        let w = Math.max(8, x1 - x0), hh = Math.max(8, y1 - y0);
        if (ratio) { hh = w * ratio[1] / ratio[0]; if (top) y0 = y1 - hh; }
        rect = { x: Math.min(x0, x1), y: Math.min(y0, y1), w, h: hh };
      }
      rect = { x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.w), h: Math.round(rect.h) };
      clamp(); sync(); draw();
    });
    canvas.addEventListener('pointerup', () => { drag = null; });
    const toImage = (e) => { const b = canvas.getBoundingClientRect(); return { x: (e.clientX - b.left) / scale, y: (e.clientY - b.top) / scale }; };

    apply.addEventListener('click', () => ctx.run({
      x: rect.x, y: rect.y, w: rect.w, h: rect.h,
      width: +ow.value || 0, height: +oh.value || 0,
    }));
    params.ratio = 'free';
    layout(); sync(); draw();
    new ResizeObserver(() => { layout(); draw(); }).observe(stage);
  };
})();
