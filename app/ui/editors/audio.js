/* Audio editor (Normalize, Visualizer, Trim): waveform view with range
   selection; before/after loudness meters; A/B listening against a rendered preview. */
(function () {
  'use strict';
  const T = window.__TAURI__;
  const mmss = (t) => `${String(Math.floor(t / 60)).padStart(2, '0')}:${(t % 60).toFixed(2).padStart(5, '0')}`;

  window.MorphEditors.audio = async (ctx) => {
    const { h, panel, params, spec } = ctx;
    const op = spec.id.split('.')[1];
    const path = ctx.paths[0];
    const wrap = h('div', { class: 'ed-wrap' });
    const stage = h('div', { class: 'ed-stage', style: 'cursor:text;touch-action:none' });
    const canvas = h('canvas', { 'aria-label': 'Waveform', style: 'position:absolute;inset:8px;width:calc(100% - 16px);height:calc(100% - 16px)' });
    stage.append(canvas);
    const bar = h('div', { class: 'ed-toolbar' });
    const clock = h('span', { class: 'hud-mono' });
    wrap.append(stage, bar);
    panel.centre.append(wrap);

    const info = await ctx.invoke('video_info', { path });
    const dur = info.duration;
    const peaks = await ctx.invoke('audio_peaks', { path, buckets: 1200 });
    let sel = { a: 0, b: dur };

    function draw() {
      const r = canvas.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(r.width * devicePixelRatio));
      canvas.height = Math.max(1, Math.round(r.height * devicePixelRatio));
      const g = canvas.getContext('2d');
      const W = canvas.width, H = canvas.height, mid = H / 2;
      const css = getComputedStyle(document.documentElement);
      const line = css.getPropertyValue('--hud-line').trim(), accent = css.getPropertyValue('--hud-accent').trim();
      g.clearRect(0, 0, W, H);
      const x = (t) => (t / dur) * W;
      g.fillStyle = 'rgba(255,179,71,.16)';
      if (op === 'trim') g.fillRect(x(sel.a), 0, x(sel.b) - x(sel.a), H);
      g.strokeStyle = line; g.lineWidth = Math.max(1, devicePixelRatio);
      g.beginPath();
      peaks.forEach(([lo, hi], i) => { const px = (i / peaks.length) * W; g.moveTo(px, mid - hi * mid * 0.95); g.lineTo(px, mid - lo * mid * 0.95); });
      g.stroke();
      g.strokeStyle = 'rgba(79,227,255,.25)'; g.beginPath(); g.moveTo(0, mid); g.lineTo(W, mid); g.stroke();
      if (drag || op === 'trim') { g.fillStyle = accent; g.fillRect(x(sel.a) - 1, 0, 2, H); g.fillRect(x(sel.b) - 1, 0, 2, H); }
      clock.textContent = `${mmss(sel.a)} → ${mmss(sel.b)}  ·  ${mmss(sel.b - sel.a)}  /  ${mmss(dur)}`;
    }

    let drag = null;
    const tAt = (e) => { const r = canvas.getBoundingClientRect(); return Math.max(0, Math.min(dur, ((e.clientX - r.left) / r.width) * dur)); };
    stage.addEventListener('pointerdown', (e) => { stage.setPointerCapture(e.pointerId); const t = tAt(e); drag = { o: t }; sel = { a: t, b: t }; draw(); });
    stage.addEventListener('pointermove', (e) => { if (!drag) return; const t = tAt(e); sel = { a: Math.min(drag.o, t), b: Math.max(drag.o, t) }; draw(); });
    stage.addEventListener('pointerup', () => { drag = null; if (sel.b - sel.a < 0.02) sel = { a: 0, b: dur }; draw(); });
    new ResizeObserver(draw).observe(stage);

    // Playback (where the webview can decode the format): A = original, B = rendered preview.
    const player = h('audio', { preload: 'none' });
    const play = (src) => { player.src = T.core.convertFileSrc(src); player.currentTime = op === 'trim' ? sel.a : 0; player.play().catch(() => panel.setStatus('This format can\'t be played here', 'error')); };
    const a = h('button', { class: 'hud-chip', type: 'button', text: '▶ A · ORIGINAL' });
    a.addEventListener('click', () => play(path));
    const b = h('button', { class: 'hud-chip', type: 'button', text: '▶ B · RESULT' });
    b.addEventListener('click', async () => {
      panel.setStatus('RENDERING PREVIEW…');
      try { const out = await ctx.invoke('preview_op', { op: spec.id, paths: [path], params: collect() }); panel.setStatus('PREVIEW READY'); play(out); if (op === 'normalize') meter(out, afterM); }
      catch (e) { panel.setStatus(String(e), 'error'); }
    });
    const stop = h('button', { class: 'hud-chip', type: 'button', text: '■' });
    stop.addEventListener('click', () => player.pause());
    bar.append(a, b, stop, clock);

    const right = panel.right;
    const form = h('div');
    right.append(form);
    ctx.spec.params.filter((p) => p.type !== 'json' && !['start', 'end'].includes(p.key)).forEach((p) => {
      const f = MorphEditorKit.paramField(p, params[p.key], (v) => { params[p.key] = v; });
      if (f) form.append(f);
    });

    const meterBox = (label) => h('div', { class: 'hud-mono', style: 'margin:6px 0', text: `${label}: —` });
    const beforeM = meterBox('BEFORE'), afterM = meterBox('AFTER');
    async function meter(p, el) {
      el.textContent = `${el.textContent.split(':')[0]}: measuring…`;
      const m = await ctx.invoke('audio_loudness', { path: p });
      el.textContent = `${el.textContent.split(':')[0]}: ${m.i.toFixed(1)} LUFS · LRA ${m.lra.toFixed(1)} LU · TP ${m.tp.toFixed(1)} dBTP`;
    }
    if (op === 'normalize') { right.append(h('h2', { class: 'hud-panel', style: 'font-size:11px;letter-spacing:3px;color:var(--hud-line);margin:18px 0 6px', text: 'METERS' }), beforeM, afterM); meter(path, beforeM); }

    if (op === 'trim') right.append(h('p', { class: 'ed-note', text: 'Drag on the waveform to choose what to keep.' }));

    function collect() {
      const p = Object.assign({}, params);
      if (op === 'trim') Object.assign(p, { start: String(sel.a), end: String(sel.b) });
      return p;
    }
    const save = h('button', { class: 'hud-btn primary', type: 'button', text: `SAVE · ${spec.label.toUpperCase()}` });
    save.addEventListener('click', () => ctx.run(collect()));
    right.append(h('div', { class: 'ed-actions', style: 'margin-top:16px' }, save), player);
    draw();
  };
})();
