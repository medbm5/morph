/* Video editor (Trim, Crop, Speed, Join, Save frames, Split, Redact).
   Preview = frames decoded by FFmpeg: a proxy strip for scrubbing plus the exact frame
   at the playhead, so it works the same in WebView2 and WebKitGTK (no GStreamer needed). */
(function () {
  'use strict';
  const T = window.__TAURI__;

  const tc = (t, fps) => {
    const f = Math.round((t % 1) * fps);
    const s = Math.floor(t);
    return `${String(Math.floor(s / 3600)).padStart(2, '0')}:${String(Math.floor(s / 60) % 60).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}:${String(f).padStart(2, '0')}`;
  };

  window.MorphEditors.video = async (ctx) => {
    const { h, panel, params, spec } = ctx;
    const op = spec.id.split('.')[1];
    const wrap = h('div', { class: 'ed-wrap' });
    const stage = h('div', { class: 'ed-stage', style: 'touch-action:none' });
    const img = h('img', { alt: 'Frame preview', style: 'position:absolute;inset:8px;width:calc(100% - 16px);height:calc(100% - 16px);object-fit:contain' });
    const overlay = h('canvas', { style: 'position:absolute;inset:0;width:100%;height:100%;pointer-events:none' });
    stage.append(img, overlay);
    const timeline = h('div', { style: 'position:relative;height:64px;min-height:64px;border:1px solid var(--hud-line-25);overflow:hidden;cursor:pointer;touch-action:none' });
    const strip = h('div', { style: 'position:absolute;inset:0;display:flex;opacity:.85' });
    const range = h('div', { style: 'position:absolute;top:0;bottom:0;background:color-mix(in srgb,var(--hud-accent) 22%,transparent);border-left:2px solid var(--hud-accent);border-right:2px solid var(--hud-accent);pointer-events:none' });
    const head = h('div', { style: 'position:absolute;top:0;bottom:0;width:2px;background:#fff;box-shadow:0 0 6px var(--hud-line);pointer-events:none' });
    const marks = h('div', { style: 'position:absolute;inset:0;pointer-events:none' });
    timeline.append(strip, range, marks, head);
    const controls = h('div', { class: 'ed-toolbar' });
    const clock = h('span', { class: 'hud-mono', style: 'min-width:220px', text: '00:00:00:00' });
    wrap.append(stage, timeline, controls);
    panel.centre.append(wrap);

    const info = await ctx.invoke('video_info', { path: ctx.paths[0] });
    const dur = info.duration, fps = info.fps, step = 1 / fps;
    let t = 0;
    let inT = 0, outT = dur;

    // Proxy strip for scrubbing.
    ctx.invoke('video_strip', { path: ctx.paths[0], count: 40, width: 160 }).then((frames) => {
      frames.forEach((f) => strip.append(h('img', { src: T.core.convertFileSrc(f), alt: '', style: 'flex:1;min-width:0;height:100%;object-fit:cover' })));
    });
    let reqSeq = 0;
    async function show(exact) {
      const my = ++reqSeq;
      head.style.left = `${(t / dur) * 100}%`;
      clock.textContent = `${tc(t, fps)}  /  ${tc(dur, fps)}`;
      const width = exact ? Math.min(info.width, 1280) : 480;
      const f = await ctx.invoke('video_frame', { path: ctx.paths[0], t, width });
      if (my === reqSeq) img.src = T.core.convertFileSrc(f);
    }
    const seek = (nt, exact = true) => { t = Math.max(0, Math.min(dur, nt)); show(exact); drawOverlay(); };

    // Timeline: click/drag to scrub.
    let scrubbing = false;
    const tAt = (e) => { const r = timeline.getBoundingClientRect(); return ((e.clientX - r.left) / r.width) * dur; };
    timeline.addEventListener('pointerdown', (e) => { scrubbing = true; timeline.setPointerCapture(e.pointerId); seek(tAt(e), false); });
    timeline.addEventListener('pointermove', (e) => { if (scrubbing) seek(tAt(e), false); });
    timeline.addEventListener('pointerup', (e) => { scrubbing = false; seek(tAt(e), true); });

    const btn = (label, fn, title) => { const b = h('button', { class: 'hud-chip', type: 'button', text: label, title: title || label }); b.addEventListener('click', fn); controls.append(b); return b; };
    btn('⏮', () => seek(0), 'Start');
    btn('◀ FRAME', () => seek(t - step), 'Previous frame (←)');
    btn('FRAME ▶', () => seek(t + step), 'Next frame (→)');
    btn('⏭', () => seek(dur - step), 'End');
    controls.append(clock);
    window.addEventListener('keydown', (e) => {
      if (e.target.tagName === 'INPUT') return;
      if (e.key === 'ArrowLeft') seek(t - (e.shiftKey ? 1 : step));
      if (e.key === 'ArrowRight') seek(t + (e.shiftKey ? 1 : step));
      if (e.key === 'i') { inT = t; sync(); }
      if (e.key === 'o') { outT = t; sync(); }
    });

    const right = panel.right;
    const field = (label, input) => { right.append(h('label', { class: 'hud-field' }, h('span', { text: label }), input)); return input; };
    const num = (v) => { const i = h('input', { type: 'text', style: 'width:140px' }); i.value = v; return i; };
    const markList = h('div', { class: 'hud-list' });
    function sync() {
      if (op === 'trim' || op === 'redact') {
        range.style.display = '';
        range.style.left = `${(inT / dur) * 100}%`;
        range.style.width = `${((outT - inT) / dur) * 100}%`;
      } else range.style.display = 'none';
      if (startIn) startIn.value = tc(inT, fps);
      if (endIn) endIn.value = tc(outT, fps);
      marks.textContent = '';
      const pts = op === 'split' ? splitPts : op === 'frames' ? framePts : [];
      pts.forEach((p) => marks.append(h('div', { style: `position:absolute;top:0;bottom:0;left:${(p / dur) * 100}%;width:2px;background:var(--hud-accent)` })));
      markList.textContent = '';
      pts.forEach((p, i) => {
        const b = h('button', { type: 'button' }, h('span', { class: 'hud-mono', text: String(i + 1).padStart(2, '0') }), h('span', { text: tc(p, fps) }), h('span', { class: 'hud-mono', text: '✕' }));
        b.addEventListener('click', () => { pts.splice(i, 1); sync(); });
        markList.append(b);
      });
      drawOverlay();
    }
    const fromTc = (s) => { const p = s.split(':').map(Number); if (p.length === 4) return p[0] * 3600 + p[1] * 60 + p[2] + p[3] / fps; return p.reduce((a, b) => a * 60 + b, 0); };

    let startIn = null, endIn = null;
    const splitPts = [], framePts = [];
    const regions = [];

    if (op === 'trim' || op === 'redact') {
      startIn = field('IN (I)', num(tc(0, fps)));
      endIn = field('OUT (O)', num(tc(dur, fps)));
      startIn.addEventListener('change', () => { inT = fromTc(startIn.value); sync(); });
      endIn.addEventListener('change', () => { outT = fromTc(endIn.value); sync(); });
      const setIn = h('button', { class: 'hud-btn small', type: 'button', text: 'SET IN AT PLAYHEAD' });
      const setOut = h('button', { class: 'hud-btn small', type: 'button', text: 'SET OUT AT PLAYHEAD' });
      setIn.addEventListener('click', () => { inT = t; sync(); });
      setOut.addEventListener('click', () => { outT = t; sync(); });
      right.append(h('div', { class: 'hud-row' }, setIn, setOut));
    }
    if (op === 'speed') {
      const f = field('SPEED ×', h('input', { type: 'range', min: 0.25, max: 4, step: 0.05 }));
      const label = h('span', { class: 'hud-mono', text: '2.00×' });
      f.value = params.factor || 2;
      f.addEventListener('input', () => { params.factor = +f.value; label.textContent = `${(+f.value).toFixed(2)}× · ${tc(dur / f.value, fps)}`; });
      right.append(label);
      f.dispatchEvent(new Event('input'));
    }
    if (op === 'split' || op === 'frames') {
      const add = h('button', { class: 'hud-btn small', type: 'button', text: op === 'split' ? 'SPLIT AT PLAYHEAD' : 'GRAB FRAME AT PLAYHEAD' });
      add.addEventListener('click', () => { (op === 'split' ? splitPts : framePts).push(+t.toFixed(3)); (op === 'split' ? splitPts : framePts).sort((a, b) => a - b); sync(); });
      right.append(add, markList);
      if (op === 'split') {
        const parts = field('OR EQUAL PARTS', h('input', { type: 'number', min: 2, max: 100, style: 'width:90px' }));
        parts.value = 2;
        parts.addEventListener('change', () => { params.parts = +parts.value; });
      }
    }
    if (op === 'join') {
      const order = h('div', { class: 'hud-list' });
      const render = () => {
        order.textContent = '';
        ctx.paths.forEach((p, i) => {
          const up = h('button', { class: 'hud-btn small', type: 'button', text: '▲' });
          up.disabled = i === 0;
          up.addEventListener('click', () => { [ctx.paths[i - 1], ctx.paths[i]] = [ctx.paths[i], ctx.paths[i - 1]]; render(); });
          order.append(h('div', { class: 'hud-row', style: 'border:1px solid var(--hud-line-35);padding:6px 10px;justify-content:space-between' },
            h('span', { class: 'hud-mono', text: `${String(i + 1).padStart(2, '0')}  ${String(p).split(/[\\/]/).pop()}${i === 0 ? '  · CANVAS' : ''}` }), up));
        });
      };
      right.append(h('h2', { class: 'hud-panel', style: 'font-size:11px;letter-spacing:3px;color:var(--hud-line);margin:18px 0 8px', text: 'ORDER · FIRST CLIP SETS THE SIZE' }), order);
      render();
    }

    // Crop & redact rectangles over the frame.
    let rect = null;
    // The frame is letterboxed inside the img box (object-fit: contain).
    const imgRect = () => {
      const r = img.getBoundingClientRect(), s = stage.getBoundingClientRect();
      const k = Math.min(r.width / info.width, r.height / info.height);
      const w = info.width * k, hh = info.height * k;
      return { x: r.left - s.left + (r.width - w) / 2, y: r.top - s.top + (r.height - hh) / 2, w, h: hh, k };
    };
    function drawOverlay() {
      const s = stage.getBoundingClientRect();
      overlay.width = s.width; overlay.height = s.height;
      const g = overlay.getContext('2d');
      g.clearRect(0, 0, s.width, s.height);
      const b = imgRect();
      const accent = getComputedStyle(document.documentElement).getPropertyValue('--hud-accent').trim();
      const box = (r, active) => {
        g.strokeStyle = accent; g.lineWidth = 2;
        g.fillStyle = active ? 'rgba(0,0,0,.65)' : 'rgba(0,0,0,.2)';
        const x = b.x + r.x * b.k, y = b.y + r.y * b.k, w = r.w * b.k, hh = r.h * b.k;
        if (op === 'redact') g.fillRect(x, y, w, hh);
        g.strokeRect(x, y, w, hh);
      };
      if (op === 'crop' && rect) box(rect, true);
      if (op === 'redact') regions.forEach((r) => box(r, t >= r.start && t <= r.end));
    }
    if (op === 'crop' || op === 'redact') {
      overlay.style.pointerEvents = 'auto';
      overlay.style.cursor = 'crosshair';
      let drag = null;
      const toVid = (e) => { const b = imgRect(), s = stage.getBoundingClientRect(); return { x: Math.round((e.clientX - s.left - b.x) / b.k), y: Math.round((e.clientY - s.top - b.y) / b.k) }; };
      overlay.addEventListener('pointerdown', (e) => {
        overlay.setPointerCapture(e.pointerId);
        const p = toVid(e);
        const r = { x: p.x, y: p.y, w: 1, h: 1, start: inT, end: outT };
        if (op === 'crop') rect = r; else regions.push(r);
        drag = { r, o: p };
      });
      overlay.addEventListener('pointermove', (e) => {
        if (!drag) return;
        const p = toVid(e);
        Object.assign(drag.r, { x: Math.min(p.x, drag.o.x), y: Math.min(p.y, drag.o.y), w: Math.abs(p.x - drag.o.x), h: Math.abs(p.y - drag.o.y) });
        drawOverlay();
        if (op === 'crop') panel.setOutput(`${drag.r.w} × ${drag.r.h} at ${drag.r.x}, ${drag.r.y}`);
      });
      overlay.addEventListener('pointerup', () => { drag = null; sync(); });
      if (op === 'redact') {
        const style = h('button', { class: 'hud-btn small', type: 'button', text: 'STYLE · SOLID' });
        style.addEventListener('click', () => { params.style = params.style === 'blur' ? 'solid' : 'blur'; style.textContent = `STYLE · ${params.style.toUpperCase()}`; });
        const clear = h('button', { class: 'hud-btn small', type: 'button', text: 'CLEAR REGIONS' });
        clear.addEventListener('click', () => { regions.length = 0; drawOverlay(); });
        right.append(h('p', { class: 'ed-note', text: 'Set IN/OUT, then draw a box: it is hidden during that range.' }), h('div', { class: 'hud-row' }, style, clear));
      }
    }
    new ResizeObserver(drawOverlay).observe(stage);
    img.addEventListener('load', drawOverlay);

    const save = h('button', { class: 'hud-btn primary', type: 'button', text: `SAVE · ${spec.label.toUpperCase()}` });
    save.addEventListener('click', () => {
      const extra = {};
      if (op === 'trim') Object.assign(extra, { start: String(inT), end: String(outT) });
      if (op === 'crop' && rect) Object.assign(extra, { x: rect.x, y: rect.y, w: rect.w, h: rect.h });
      if (op === 'split' && splitPts.length) Object.assign(extra, { points: splitPts.join(','), parts: 0 });
      if (op === 'frames') extra.times = (framePts.length ? framePts : [t]).join(',');
      if (op === 'redact') extra.regions = regions;
      ctx.run(extra);
    });
    right.append(h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save));
    sync();
    seek(0);
  };
})();
