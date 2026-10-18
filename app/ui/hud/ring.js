/* Metamorphosis — HUD ring component (no dependencies).

   const ring = createHudRing(host, {
     items: [{ id, label, out?, badge?, disabled? }],   // segments (≤ 7 per page, MORE pages the rest)
     file: { size: '1.1 MB', type: 'JPG' },             // centre readout
     ringName: 'CONVERT',
     accent: '#FFB347', reduced: false,
     onHover(item, index), onSelect(item, index), onCancel(), onShowInFolder(path)
   });

   ring.setItems(items, { ringName, file, animate: 'switch' | 'boot' | 'none' })
   ring.hitTest(clientX, clientY) → index | -1        // pure geometry: works from frame 0
   ring.setHot(i), ring.hot(), ring.select(i), ring.cancel(), ring.boot()
   ring.collapse()                                      // drop animation
   ring.toast(html, { error, ms, actions: [{ label, run }] })
   ring.progress(done, total, fraction, text)           // progress card for long jobs
   ring.setAccent(hex) → contrast ratio vs --hud-core; ring.setReduced(bool)
   ring.measureIdle(ms) → Promise<avg frame ms>; drops the glow when over budget
*/
(function (global) {
  'use strict';
  const NS = 'http://www.w3.org/2000/svg';
  const R_IN = 122, R_OUT = 232, R_LABEL = 177, R_NUM = 244, R_BADGE = 196, GAP_DEG = 1.8, VIEW = 600;
  const PAGE = 7;
  const CORE_RGB = [4, 10, 18];

  const el = (name, attrs = {}, parent) => {
    const n = document.createElementNS(NS, name);
    for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
    if (parent) parent.append(n);
    return n;
  };
  const polar = (r, deg) => { const a = (deg - 90) * Math.PI / 180; return [r * Math.cos(a), r * Math.sin(a)]; };
  const sector = (a0, a1) => {
    const [x0, y0] = polar(R_OUT, a0), [x1, y1] = polar(R_OUT, a1), [x2, y2] = polar(R_IN, a1), [x3, y3] = polar(R_IN, a0);
    const L = a1 - a0 > 180 ? 1 : 0, f = (n) => n.toFixed(1);
    return `M${f(x0)} ${f(y0)}A${R_OUT} ${R_OUT} 0 ${L} 1 ${f(x1)} ${f(y1)}L${f(x2)} ${f(y2)}A${R_IN} ${R_IN} 0 ${L} 0 ${f(x3)} ${f(y3)}Z`;
  };
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  /* ---------- Colour helpers (WCAG 2.x) ---------- */
  function hexToRgb(hex) {
    const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})([0-9a-f]{2})?$/i.exec(String(hex).trim());
    if (!m) return null;
    let h = m[1];
    if (h.length === 3) h = h.split('').map((c) => c + c).join('');
    return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
  }
  function luminance([r, g, b]) {
    const lin = (c) => { c /= 255; return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4); };
    return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
  }
  function contrast(a, b) {
    const ra = typeof a === 'string' ? hexToRgb(a) : a, rb = typeof b === 'string' ? hexToRgb(b) : b;
    if (!ra || !rb) return 0;
    const [hi, lo] = [luminance(ra), luminance(rb)].sort((x, y) => y - x);
    return (hi + 0.05) / (lo + 0.05);
  }
  /** Contrast of an accent against the centre disc (--hud-core). The centre label needs ≥ 4.5:1. */
  const accentContrast = (hex) => contrast(hex, CORE_RGB);

  const prefersReduced = () => global.matchMedia && global.matchMedia('(prefers-reduced-motion: reduce)').matches;

  function createHudRing(host, opts = {}) {
    let all = opts.items || [];
    let page = 0;
    let items = [];
    let file = opts.file || { size: '', type: '' };
    let ringName = opts.ringName || '';
    let hot = 0;
    let busy = false;
    let reduced = !!opts.reduced;
    const timers = new Set();
    const later = (fn, ms) => { const t = setTimeout(() => { timers.delete(t); fn(); }, ms); timers.add(t); return t; };
    const clearTimers = () => { timers.forEach(clearTimeout); timers.clear(); };
    const isReduced = () => reduced || prefersReduced();

    host.classList.add('hud');
    host.classList.toggle('hud-reduced', reduced);
    if (opts.accent) host.style.setProperty('--hud-accent', opts.accent);

    const uid = Math.random().toString(36).slice(2, 8);
    const svg = el('svg', { class: 'hud-svg', viewBox: `${-VIEW / 2} ${-VIEW / 2} ${VIEW} ${VIEW}`, role: 'group', 'aria-label': opts.label || 'Action ring' }, host);
    const defs = el('defs', {}, svg);
    const clip = el('clipPath', { id: `hud-clip-${uid}` }, defs);
    el('circle', { r: 262 }, clip);
    const grad = el('linearGradient', { id: `hud-scan-${uid}`, x1: 0, y1: 0, x2: 0, y2: 1 }, defs);
    el('stop', { offset: 0, 'stop-color': '#4FE3FF', 'stop-opacity': 0 }, grad);
    el('stop', { offset: 0.5, 'stop-color': '#4FE3FF', 'stop-opacity': 0.05 }, grad);
    el('stop', { offset: 1, 'stop-color': '#4FE3FF', 'stop-opacity': 0 }, grad);

    const body = el('g', { class: 'hud-body' }, svg);
    el('circle', { r: 262, fill: 'var(--hud-backdrop)', class: 'hud-flicker' }, body);
    const scanWrap = el('g', { 'clip-path': `url(#${clip.id})`, 'aria-hidden': 'true' }, body);
    el('rect', { x: -262, y: 0, width: 524, height: 120, fill: `url(#${grad.id})`, class: 'hud-scan' }, scanWrap);

    const deco = el('g', { 'aria-hidden': 'true' }, body);
    const ring = (r, cls, attrs) => {
      // Flicker (boot) and rotation (idle) live on separate groups so neither overrides the other.
      const f = el('g', { class: 'hud-flicker' }, deco);
      const g = el('g', { class: `hud-spin ${cls}` }, f);
      el('circle', Object.assign({ r, fill: 'none' }, attrs), g);
    };
    ring(282, 'hud-spin-slow', { stroke: 'var(--hud-line)', 'stroke-opacity': 0.45, 'stroke-width': 1, 'stroke-dasharray': '2 8' });
    ring(268, 'hud-spin-mid', { stroke: 'var(--hud-line)', 'stroke-opacity': 0.7, 'stroke-width': 2, 'stroke-dasharray': '120 40 12 40 60 80' });
    ring(248, 'hud-spin-fast', { stroke: 'var(--hud-accent)', 'stroke-opacity': 0.6, 'stroke-width': 1.5, 'stroke-dasharray': '30 400' });
    el('circle', { r: 240, fill: 'none', stroke: 'var(--hud-line)', 'stroke-opacity': 0.25, 'stroke-width': 1, pathLength: 1, class: 'hud-draw', style: '--d:0' }, deco);
    [[0, -292, 0, -272], [0, 272, 0, 292], [-292, 0, -272, 0], [272, 0, 292, 0]].forEach(([x1, y1, x2, y2]) =>
      el('line', { x1, y1, x2, y2, stroke: 'var(--hud-line)', 'stroke-width': 1, class: 'hud-flicker' }, deco));

    const segLayer = el('g', { class: 'hud-glow hud-segs' }, body);

    const core = el('g', { class: 'hud-center' }, body);
    el('circle', { r: 108, fill: 'var(--hud-core)', stroke: 'var(--hud-line)', 'stroke-opacity': 0.55, 'stroke-width': 1 }, core);
    const inner = el('g', { class: 'hud-spin hud-spin-mid', 'aria-hidden': 'true' }, core);
    el('circle', { r: 98, fill: 'none', stroke: 'var(--hud-line)', 'stroke-opacity': 0.35, 'stroke-width': 1, 'stroke-dasharray': '1 5' }, inner);
    const t = (y, cls, txt) => { const n = el('text', { x: 0, y, 'text-anchor': 'middle', class: cls }, core); n.textContent = txt; return n; };
    const ringText = t(-58, 'hud-ringname', '');
    t(-38, 'hud-cap', 'ACTION');
    const actionText = t(-4, 'hud-action', '');
    const hintText = t(24, 'hud-hint', 'RELEASE TO APPLY');
    const metaText = t(46, 'hud-meta', '');

    el('circle', { r: 3, class: 'hud-dot' }, svg);
    el('circle', { r: 240, class: 'hud-pulse' }, svg);

    const live = document.createElement('div');
    live.setAttribute('aria-live', 'polite');
    live.style.cssText = 'position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0)';
    host.append(live);

    const toastEl = document.createElement('div');
    toastEl.className = 'hud-toast';
    toastEl.setAttribute('role', 'status');
    host.append(toastEl);

    /* ---------- Segments and paging ---------- */
    function pageItems() {
      if (all.length <= PAGE) return all.slice();
      const per = PAGE - 1;
      const pages = Math.ceil(all.length / per);
      const p = page % pages;
      const slice = all.slice(p * per, p * per + per);
      slice.push({ id: '__more', label: 'More', more: true, badge: `${p + 1}/${pages}` });
      return slice;
    }

    let segs = [];
    function renderSegments() {
      items = pageItems();
      segLayer.textContent = '';
      const n = items.length;
      if (!n) { segs = []; setHot(-1); return; }
      const step = 360 / n;
      segs = items.map((item, i) => {
        const mid = i * step, a0 = mid - step / 2 + GAP_DEG, a1 = mid + step / 2 - GAP_DEG;
        const cls = ['hud-seg', item.disabled ? 'is-off' : ''].join(' ').trim();
        const g = el('g', { class: cls, role: 'button', tabindex: i === hot ? '0' : '-1', 'aria-label': `${item.label}, ${i + 1} of ${n}`, 'data-index': i }, segLayer);
        el('path', { d: n === 1 ? sector(180.5, 539.5) : sector(a0, a1), pathLength: 1, class: 'hud-draw', style: `--d:${i + 1}` }, g);
        const [lx, ly] = polar(R_LABEL, mid), [nx, ny] = polar(R_NUM, mid);
        el('text', { x: lx.toFixed(1), y: ly.toFixed(1), 'text-anchor': 'middle', 'dominant-baseline': 'middle', class: 'hud-label' }, g).textContent = item.label;
        el('text', { x: nx.toFixed(1), y: ny.toFixed(1), 'text-anchor': 'middle', 'dominant-baseline': 'middle', class: 'hud-num' }, g).textContent = String(i + 1).padStart(2, '0');
        if (item.badge) {
          const [bx, by] = polar(R_BADGE, mid);
          el('text', { x: bx.toFixed(1), y: by.toFixed(1), 'text-anchor': 'middle', 'dominant-baseline': 'middle', class: 'hud-badge' }, g).textContent = item.badge;
        }
        g.addEventListener('pointerenter', () => { if (!busy) setHot(i); });
        g.addEventListener('focus', () => { if (!busy) setHot(i); });
        g.addEventListener('click', () => select(i));
        return g;
      });
      setHot(Math.max(0, Math.min(hot, n - 1)));
    }

    function setHot(i) {
      if (i === hot && segs[i] && segs[i].classList.contains('is-hot')) return;
      hot = i;
      segs.forEach((s, j) => { s.classList.toggle('is-hot', j === i); s.setAttribute('tabindex', j === i ? '0' : '-1'); });
      const item = items[i];
      actionText.textContent = item ? item.label.toUpperCase() : '';
      hintText.textContent = item && item.disabled ? (item.badge || 'UNAVAILABLE') : item && item.more ? 'NEXT PAGE' : 'RELEASE TO APPLY';
      metaText.textContent = [file.size, file.type].filter(Boolean).join(' · ');
      ringText.textContent = ringName;
      if (item) live.textContent = `${item.label}, ${i + 1} of ${items.length}`;
      opts.onHover && opts.onHover(item, i);
    }

    /** Segment under a client point, or -1 (centre, outside, or a gap). Pure geometry. */
    function hitTest(clientX, clientY) {
      const r = svg.getBoundingClientRect();
      if (!r.width || !items.length) return -1;
      const s = VIEW / r.width;
      const x = (clientX - r.left) * s - VIEW / 2, y = (clientY - r.top) * s - VIEW / 2;
      const d = Math.hypot(x, y);
      if (d < R_IN - 4 || d > R_OUT + 28) return -1;
      let deg = Math.atan2(y, x) * 180 / Math.PI + 90;
      if (deg < 0) deg += 360;
      const step = 360 / items.length;
      return Math.round(deg / step) % items.length;
    }

    /* ---------- Animation states ---------- */
    function restart(cls) {
      host.classList.remove('hud-collapsed', 'hud-cancelled', 'hud-boot', 'hud-switch-in', 'hud-switch-out');
      void host.getBoundingClientRect(); // restart CSS animations
      if (cls) host.classList.add(cls);
    }

    function boot() { clearTimers(); busy = false; hideToast(); restart('hud-boot'); }

    function setItems(next, o = {}) {
      all = next || [];
      page = 0;
      if (o.file) file = o.file;
      if (o.ringName !== undefined) ringName = o.ringName;
      hot = 0;
      const anim = o.animate || 'switch';
      if (anim === 'switch' && segs.length && !isReduced()) {
        host.classList.add('hud-switch-out');
        later(() => { renderSegments(); restart('hud-switch-in'); }, 100);
      } else {
        renderSegments();
        if (anim === 'boot') boot();
        else if (anim === 'switch') restart('hud-switch-in');
      }
    }

    function select(i) {
      if (busy || !items[i]) return;
      const item = items[i];
      if (item.more) { page += 1; hot = 0; renderSegments(); restart('hud-switch-in'); return; }
      if (item.disabled && !item.badge) return;
      setHot(i);
      opts.onSelect && opts.onSelect(item, i);
    }

    function collapse() {
      busy = true;
      restart('hud-collapsed');
    }

    function cancel() {
      if (busy) return;
      busy = true;
      host.classList.add('hud-cancelled');
      later(() => { busy = false; opts.onCancel && opts.onCancel(); }, 120);
    }

    /* ---------- Toast and progress card ---------- */
    let toastTimer = null;
    function hideToast() { clearTimeout(toastTimer); toastEl.classList.remove('is-on', 'is-error'); }

    function toast(html, o = {}) {
      clearTimeout(toastTimer);
      toastEl.innerHTML = html;
      if (o.actions && o.actions.length) {
        const links = document.createElement('div');
        links.className = 'hud-links';
        o.actions.forEach((a) => {
          const b = document.createElement('button');
          b.type = 'button';
          b.textContent = a.label;
          b.addEventListener('click', (e) => { e.stopPropagation(); a.run(); });
          links.append(b);
        });
        toastEl.append(links);
      }
      toastEl.classList.toggle('is-error', !!o.error);
      const show = () => toastEl.classList.add('is-on');
      if (o.delay) later(show, isReduced() ? 100 : o.delay); else show();
      const ms = o.ms === undefined ? 1600 : o.ms;
      if (ms > 0) toastTimer = setTimeout(() => { toastEl.classList.remove('is-on'); o.onHide && o.onHide(); }, ms + (o.delay || 0));
    }

    /** Saved confirmation: `✓ SAVED · Page 001 Annotated.jpg`. */
    function saved(name, o = {}) {
      toast(`<b>✓ ${esc(o.verb || 'SAVED')}</b> · ${esc(name)}`, o);
    }

    function progress(done, total, fraction, text) {
      clearTimeout(toastTimer);
      const p = Math.max(0, Math.min(1, fraction || 0));
      toastEl.innerHTML = `<b>${esc(done)} OF ${esc(total)} SAVED</b>${text ? ' · ' + esc(text) : ''}<span class="hud-bar"><i style="--p:${p}"></i></span>`;
      toastEl.classList.remove('is-error');
      toastEl.classList.add('is-on');
    }

    /* ---------- Keyboard (when focused; the overlay forwards global keys via the methods) ---------- */
    host.addEventListener('keydown', (e) => {
      const n = items.length;
      if (/^[1-9]$/.test(e.key) && Number(e.key) <= n) { e.preventDefault(); select(Number(e.key) - 1); return; }
      if (!n) return;
      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') { e.preventDefault(); setHot((hot + 1) % n); segs[hot].focus(); }
      if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') { e.preventDefault(); setHot((hot - 1 + n) % n); segs[hot].focus(); }
      if ((e.key === 'Enter' || e.key === ' ') && e.target.closest && e.target.closest('.hud-seg')) { e.preventDefault(); select(hot); }
      if (e.key === 'Escape') cancel();
    });

    /* ---------- Performance gate: drop the glow if the idle ring misses 60 fps ---------- */
    function measureIdle(ms = 1500) {
      return new Promise((resolve) => {
        const deltas = [];
        let last = performance.now();
        const start = last;
        const tick = (now) => {
          deltas.push(now - last);
          last = now;
          if (now - start < ms) requestAnimationFrame(tick);
          else {
            deltas.sort((a, b) => a - b);
            const median = deltas[Math.floor(deltas.length / 2)] || 0;
            const p90 = deltas[Math.floor(deltas.length * 0.9)] || 0;
            if (median > 16.7 || p90 > 25) host.classList.add('hud-noglow');
            resolve({ median, p90, frames: deltas.length, glow: !host.classList.contains('hud-noglow') });
          }
        };
        requestAnimationFrame(tick);
      });
    }

    renderSegments();
    if (opts.boot !== false) boot();

    return {
      setItems, setHot, hitTest, select, cancel, boot, collapse, toast, saved, progress, hideToast, measureIdle,
      hot: () => hot,
      item: (i) => items[i === undefined ? hot : i],
      count: () => items.length,
      busy: () => busy,
      next() { if (items.length) setHot((hot + 1) % items.length); },
      prev() { if (items.length) setHot((hot - 1 + items.length) % items.length); },
      setFile(f) { file = f; setHot(hot); },
      setAccent(c) { host.style.setProperty('--hud-accent', c); return accentContrast(c); },
      setReduced(r) { reduced = !!r; host.classList.toggle('hud-reduced', reduced); },
      focus() { segs[hot] && segs[hot].focus(); },
      destroy() { clearTimers(); host.textContent = ''; },
    };
  }

  global.createHudRing = createHudRing;
  global.HudColor = { contrast, accentContrast, hexToRgb };
})(window);
