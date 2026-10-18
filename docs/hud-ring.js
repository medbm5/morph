/* Metamorphosis — HUD ring component (no dependencies).
   createHudRing(host, { items, file, onHover, onDrop, accent }) → { setItems, setAccent, boot, cancel, destroy } */
(function (global) {
  'use strict';
  const NS = 'http://www.w3.org/2000/svg';
  const R_IN = 122, R_OUT = 232, R_LABEL = 160, R_NUM = 200, GAP_DEG = 1.8;

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
  const reducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  function createHudRing(host, opts) {
    let items = opts.items || [];
    let file = opts.file || { size: '1.1 MB', type: 'JPG' };
    let hot = 0;
    let busy = false;

    host.classList.add('hud');
    if (opts.accent) host.style.setProperty('--hud-accent', opts.accent);
    const svg = el('svg', { class: 'hud-svg', viewBox: '-300 -300 600 600', role: 'group', 'aria-label': opts.label || 'Action ring' }, host);
    const defs = el('defs', {}, svg);
    const clip = el('clipPath', { id: `hud-clip-${Math.random().toString(36).slice(2, 8)}` }, defs);
    el('circle', { r: 262 }, clip);
    const grad = el('linearGradient', { id: clip.id + '-g', x1: 0, y1: 0, x2: 0, y2: 1 }, defs);
    el('stop', { offset: 0, 'stop-color': '#4FE3FF', 'stop-opacity': 0 }, grad);
    el('stop', { offset: 0.5, 'stop-color': '#4FE3FF', 'stop-opacity': 0.06 }, grad);
    el('stop', { offset: 1, 'stop-color': '#4FE3FF', 'stop-opacity': 0 }, grad);

    const body = el('g', { class: 'hud-body' }, svg);
    el('circle', { r: 262, fill: 'var(--hud-backdrop)', class: 'hud-flicker' }, body);
    const scanWrap = el('g', { 'clip-path': `url(#${clip.id})`, 'aria-hidden': 'true' }, body);
    el('rect', { x: -262, y: 0, width: 524, height: 120, fill: `url(#${grad.id})`, class: 'hud-scan' }, scanWrap);

    const deco = el('g', { 'aria-hidden': 'true' }, body);
    const ring = (r, cls, attrs) => {
      const g = el('g', { class: `hud-spin ${cls} hud-flicker` }, deco);
      el('circle', Object.assign({ r, fill: 'none' }, attrs), g);
    };
    ring(282, 'hud-spin-slow', { stroke: 'rgba(79,227,255,.45)', 'stroke-width': 1, 'stroke-dasharray': '2 8' });
    ring(268, 'hud-spin-mid', { stroke: 'rgba(79,227,255,.7)', 'stroke-width': 2, 'stroke-dasharray': '120 40 12 40 60 80' });
    ring(248, 'hud-spin-fast', { stroke: 'var(--hud-accent)', 'stroke-opacity': 0.6, 'stroke-width': 1.5, 'stroke-dasharray': '30 400' });
    el('circle', { r: 240, fill: 'none', stroke: 'rgba(79,227,255,.25)', 'stroke-width': 1, pathLength: 1, class: 'hud-draw', style: '--d:0' }, deco);
    [[0, -290, 0, -270], [0, 270, 0, 290], [-290, 0, -270, 0], [270, 0, 290, 0]].forEach(([x1, y1, x2, y2]) =>
      el('line', { x1, y1, x2, y2, stroke: '#4FE3FF', 'stroke-width': 1, class: 'hud-flicker' }, deco));

    const segLayer = el('g', { class: 'hud-glow' }, body);

    const core = el('g', { class: 'hud-center' }, body);
    el('circle', { r: 108, fill: 'var(--hud-core)', stroke: 'rgba(79,227,255,.55)', 'stroke-width': 1 }, core);
    const inner = el('g', { class: 'hud-spin hud-spin-mid', 'aria-hidden': 'true' }, core);
    el('circle', { r: 98, fill: 'none', stroke: 'rgba(79,227,255,.35)', 'stroke-width': 1, 'stroke-dasharray': '1 5' }, inner);
    const t = (y, cls, txt) => { const n = el('text', { x: 0, y, 'text-anchor': 'middle', class: cls }, core); n.textContent = txt; return n; };
    t(-38, 'hud-cap', 'ACTION');
    const actionText = t(-4, 'hud-action', '');
    t(24, 'hud-hint', 'RELEASE TO APPLY');
    const metaText = t(46, 'hud-meta', '');

    el('circle', { r: 3, class: 'hud-dot' }, svg);
    el('circle', { r: 240, class: 'hud-pulse' }, svg);

    const toast = document.createElement('div');
    toast.className = 'hud-toast';
    toast.setAttribute('role', 'status');
    host.append(toast);

    let segs = [];
    function renderSegments() {
      segLayer.textContent = '';
      const n = items.length, step = 360 / n;
      segs = items.map((item, i) => {
        const mid = i * step, a0 = mid - step / 2 + GAP_DEG, a1 = mid + step / 2 - GAP_DEG;
        const g = el('g', { class: 'hud-seg', role: 'button', tabindex: i === hot ? '0' : '-1', 'aria-label': `${item.label}, ${i + 1} of ${n}` }, segLayer);
        el('path', { d: sector(a0, a1), pathLength: 1, class: 'hud-draw', style: `--d:${i + 1}` }, g);
        const [lx, ly] = polar(R_LABEL, mid), [nx, ny] = polar(R_NUM, mid);
        el('text', { x: lx.toFixed(1), y: ly.toFixed(1), 'text-anchor': 'middle', 'dominant-baseline': 'middle', class: 'hud-label' }, g).textContent = item.label;
        el('text', { x: nx.toFixed(1), y: ny.toFixed(1), 'text-anchor': 'middle', 'dominant-baseline': 'middle', class: 'hud-num' }, g).textContent = String(i + 1).padStart(2, '0');
        g.addEventListener('pointerenter', () => { if (!busy) setHot(i); });
        g.addEventListener('focus', () => { if (!busy) setHot(i); });
        g.addEventListener('click', () => drop(i));
        return g;
      });
      setHot(Math.min(hot, n - 1));
    }

    function setHot(i) {
      hot = i;
      segs.forEach((s, j) => { s.classList.toggle('is-hot', j === i); s.setAttribute('tabindex', j === i ? '0' : '-1'); });
      actionText.textContent = items[i] ? items[i].label.toUpperCase() : '';
      metaText.textContent = `${file.size} · ${file.type}`;
      opts.onHover && opts.onHover(items[i], i);
    }

    function boot() {
      host.classList.remove('hud-collapsed', 'hud-cancelled', 'hud-boot');
      void host.offsetWidth; // restart CSS animations
      host.classList.add('hud-boot');
    }

    function drop(i) {
      if (busy || !items[i]) return;
      busy = true;
      setHot(i);
      host.classList.add('hud-collapsed');
      const out = items[i].out ? items[i].out(file) : null;
      const verb = items[i].verb || 'SAVED';
      toast.innerHTML = `<b>✓ ${verb}</b> · ${out || items[i].label}`;
      const fast = reducedMotion();
      setTimeout(() => toast.classList.add('is-on'), fast ? 100 : 380);
      setTimeout(() => { toast.classList.remove('is-on'); }, fast ? 1700 : 1980);
      setTimeout(() => { busy = false; boot(); opts.onDrop && opts.onDrop(items[i], i, out); }, fast ? 1800 : 2150);
    }

    function cancel() {
      if (busy) return;
      busy = true;
      host.classList.add('hud-cancelled');
      setTimeout(() => { busy = false; boot(); }, 700);
    }

    host.addEventListener('keydown', (e) => {
      if (/^[1-9]$/.test(e.key) && Number(e.key) <= items.length) { e.preventDefault(); drop(Number(e.key) - 1); return; }
      const n = items.length;
      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') { e.preventDefault(); setHot((hot + 1) % n); segs[hot].focus(); }
      if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') { e.preventDefault(); setHot((hot - 1 + n) % n); segs[hot].focus(); }
      if ((e.key === 'Enter' || e.key === ' ') && e.target.closest && e.target.closest('.hud-seg')) { e.preventDefault(); drop(hot); }
      if (e.key === 'Escape') cancel();
    });

    renderSegments();
    boot();

    return {
      setItems(next, nextFile) { items = next; if (nextFile) file = nextFile; hot = 0; renderSegments(); boot(); },
      setAccent(c) { host.style.setProperty('--hud-accent', c); },
      drop, cancel, boot,
      focus() { segs[hot] && segs[hot].focus(); },
      destroy() { host.textContent = ''; },
    };
  }

  global.createHudRing = createHudRing;
})(window);
