/* Redact: draw areas, detect faces/text locally, move or delete areas, solid or blur. */
(function () {
  'use strict';

  window.MorphEditors.redact = async (ctx) => {
    const { h, panel, params } = ctx;
    params.areas = [];
    const wrap = h('div', { class: 'ed-wrap' });
    const toolbar = h('div', { class: 'ed-toolbar' });
    const stage = h('div', { class: 'ed-stage', style: 'cursor:crosshair;touch-action:none' });
    wrap.append(toolbar, stage);
    panel.centre.append(wrap);
    const pv = createComposePreview(stage, ctx, 'image.redact');
    const refresh = () => { pv.update(params); list(); };

    const status = await ctx.invoke('components_status');
    const mlReady = status.some((c) => c.name === 'ml' && c.installed);
    const detect = (faces, text, label) => {
      const b = h('button', { class: 'hud-chip', type: 'button', text: label });
      b.addEventListener('click', async () => {
        if (!mlReady) { ctx.invoke('open_components', { focus: 'ml' }); return; }
        b.disabled = true; panel.setStatus('DETECTING…');
        try {
          const found = await ctx.invoke('detect_regions', { path: ctx.paths[0], faces, text });
          found.forEach((r) => params.areas.push({ x: r.x, y: r.y, w: r.w, h: r.h, source: faces ? 'face' : 'text' }));
          panel.setStatus(`${found.length} AREA(S) FOUND`);
          refresh();
        } catch (e) { panel.setStatus(String(e), 'error'); }
        b.disabled = false;
      });
      return b;
    };
    const style = h('button', { class: 'hud-chip', type: 'button', text: 'STYLE · SOLID' });
    style.addEventListener('click', () => {
      params.style = params.style === 'blur' ? 'solid' : 'blur';
      style.textContent = `STYLE · ${params.style.toUpperCase()}`;
      refresh();
    });
    const clear = h('button', { class: 'hud-chip', type: 'button', text: 'CLEAR' });
    clear.addEventListener('click', () => { params.areas = []; refresh(); });
    toolbar.append(detect(true, false, 'DETECT FACES'), detect(false, true, 'DETECT TEXT'), style, clear);
    if (!mlReady) toolbar.append(h('span', { class: 'ed-note', text: 'Detection needs the Local AI component.' }));

    // Draw a new area, or drag an existing one (topmost first).
    let drag = null;
    const hit = (p) => {
      for (let i = params.areas.length - 1; i >= 0; i--) {
        const a = params.areas[i];
        if (p.x >= a.x && p.x <= a.x + a.w && p.y >= a.y && p.y <= a.y + a.h) return i;
      }
      return -1;
    };
    stage.addEventListener('pointerdown', (e) => {
      const p = pv.toImage(e.clientX, e.clientY);
      stage.setPointerCapture(e.pointerId);
      const i = hit(p);
      if (i >= 0) { drag = { move: i, start: p, orig: { ...params.areas[i] } }; return; }
      const a = { x: p.x, y: p.y, w: 1, h: 1, source: 'manual' };
      params.areas.push(a);
      drag = { draw: a, start: p };
    });
    stage.addEventListener('pointermove', (e) => {
      if (!drag) return;
      const p = pv.toImage(e.clientX, e.clientY);
      if (drag.draw) {
        const a = drag.draw, o = drag.start;
        a.x = Math.min(o.x, p.x); a.y = Math.min(o.y, p.y); a.w = Math.abs(p.x - o.x); a.h = Math.abs(p.y - o.y);
      } else {
        const a = params.areas[drag.move], o = drag.orig;
        a.x = o.x + p.x - drag.start.x; a.y = o.y + p.y - drag.start.y;
      }
      pv.update(params);
    });
    stage.addEventListener('pointerup', () => {
      if (drag && drag.draw && (drag.draw.w < 4 || drag.draw.h < 4)) params.areas.pop();
      drag = null; refresh();
    });

    // Layer list: every area can be removed individually.
    const layers = h('div', { class: 'hud-list' });
    function list() {
      layers.textContent = '';
      params.areas.forEach((a, i) => {
        const b = h('button', { type: 'button' },
          h('span', { class: 'hud-mono', text: String(i + 1).padStart(2, '0') }),
          h('span', { text: `${(a.source || 'manual').toUpperCase()} · ${Math.round(a.w)}×${Math.round(a.h)}` }),
          h('span', { class: 'hud-mono', text: '✕' }));
        b.addEventListener('click', () => { params.areas.splice(i, 1); refresh(); });
        layers.append(b);
      });
    }
    panel.right.append(h('h2', { class: 'hud-panel', style: 'font-size:11px;letter-spacing:3px;color:var(--hud-line);margin:22px 0 8px', text: 'AREAS · CLICK TO REMOVE' }), layers);
    const save = h('button', { class: 'hud-btn primary', type: 'button', text: 'SAVE REDACTED' });
    save.addEventListener('click', () => ctx.run({ auto: 'none' }));
    panel.right.append(h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save));
    refresh();
  };
})();
