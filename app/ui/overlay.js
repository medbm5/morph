/* Metamorphosis — ring overlay controller.

   Rust shows this window under the cursor when a trigger key is tapped mid-drag
   (`ring://open`), forwards ring switches and keys, and runs the job we ask for.
   The drop itself arrives through the webview's native drop target
   (OLE IDropTarget on Windows, XDND on X11) as Tauri drag-drop events. */
(() => {
  'use strict';
  const T = window.__TAURI__;
  const invoke = T.core.invoke;
  const listen = T.event.listen;
  const L = window.MorphLook;
  const RING_NAMES = new Proxy({}, { get: (_, k) => L.t(`ring.${String(k)}`, String(k).toUpperCase()) });

  let ringKind = 'convert';
  let paths = [];          // what's being dragged (drop target wins over prefetch)
  let pathsKey = '';
  let locked = -1;         // segment picked with 1–7 (wins over the pointer)
  let job = null;          // { id, total, saved: [], errors: [], started }
  let releaseTimer = null;
  let open = false;

  const ring = createHudRing(document.getElementById('host'), {
    items: [], ringName: '', boot: false,
    onSelect: (item) => apply(item, paths),
    onCancel: () => close(),
  });

  const sameSet = (a) => a.join('\n');

  async function loadItems(nextPaths, animate) {
    const key = sameSet(nextPaths);
    if (!nextPaths.length) return;
    paths = nextPaths;
    pathsKey = key;
    try {
      const r = await invoke('ring_items', { paths, ring: ringKind });
      if (pathsKey !== key) return; // a newer selection arrived meanwhile
      ring.setItems(r.items.map((it) => ({ ...it, label: L.label(it.label) })), { ringName: RING_NAMES[ringKind], file: { size: r.file.size, type: r.file.type }, animate });
      if (locked >= 0) ring.setHot(Math.min(locked, ring.count() - 1));
    } catch (e) {
      console.warn('ring_items', e);
    }
  }

  function close() {
    open = false;
    locked = -1;
    paths = [];
    pathsKey = '';
    clearTimeout(releaseTimer);
    ring.hideToast();
    invoke('hide_ring');
  }

  /* ---------- Show ---------- */
  const trace = (m) => window.morphTrace && window.morphTrace(m);

  listen('ring://open', ({ payload }) => {
    trace(`open ${payload.ring} prefetched=${payload.paths.length}`);
    open = true;
    job = null;
    locked = -1;
    ringKind = payload.ring;
    document.documentElement.classList.toggle('no-compositor', !payload.compositor);
    ring.setReduced(payload.reduced);
    ring.setItems([], { ringName: RING_NAMES[ringKind], file: { size: L.t('ring.scanning', 'SCANNING'), type: '' }, animate: 'none' });
    ring.boot();
    requestAnimationFrame(() => requestAnimationFrame(() => invoke('ring_shown')));
    if (payload.paths && payload.paths.length) loadItems(payload.paths, 'boot');
  });

  listen('ring://switch', ({ payload }) => {
    ringKind = payload;
    locked = -1;
    if (paths.length) {
      pathsKey = '';
      loadItems(paths, 'switch');
    } else {
      ring.setItems([], { ringName: RING_NAMES[ringKind], animate: 'switch' });
    }
  });

  listen('ring://key', ({ payload }) => {
    if (!open || ring.busy()) return;
    switch (payload.type) {
      case 'pick': {
        const it = ring.item(payload.index);
        if (!it) return;
        if (it.more) { ring.select(payload.index); return; }
        locked = payload.index;
        ring.setHot(payload.index);
        break;
      }
      case 'next': locked = -1; ring.next(); break;
      case 'prev': locked = -1; ring.prev(); break;
      case 'apply': if (paths.length) ring.select(ring.hot()); break;
      case 'cancel': ring.cancel(); break;
    }
  });

  // Mouse released while the ring was up: the drop event normally follows at once.
  // If none arrives (released outside the ring, or the drag carried no files), close.
  listen('ring://released', () => {
    clearTimeout(releaseTimer);
    releaseTimer = setTimeout(() => { if (open && !job) close(); }, 450);
  });

  // Clicked somewhere else while the ring was still up: dismiss it (a running job keeps its card).
  listen('ring://outside', () => {
    if (!open || job) return;
    if (ring.busy()) close(); else ring.cancel();
  });

  /* ---------- Native drop target ---------- */
  const toClient = (pos) => ({ x: pos.x / window.devicePixelRatio, y: pos.y / window.devicePixelRatio });

  // Drop events: our own OLE drop target on Windows (`ring://dnd`, client pixels),
  // the webview's native handler elsewhere (XDND on X11). Same shape after mapping.
  function onDnd(e) {
    if (e.type !== 'over') trace(`dnd ${e.type} open=${open} paths=${(e.paths || []).length}`);
    if (!open || job) return;
    const pos = e.position || { x: e.x, y: e.y };
    if (e.type === 'enter') {
      if (!e.paths || !e.paths.length) { close(); return; } // not a file drag
      if (sameSet(e.paths) !== pathsKey) loadItems(e.paths, ring.count() ? 'switch' : 'boot');
    } else if (e.type === 'over') {
      if (locked >= 0) return;
      const { x, y } = toClient(pos);
      const i = ring.hitTest(x, y);
      if (i >= 0 && i !== ring.hot()) ring.setHot(i);
    } else if (e.type === 'drop') {
      clearTimeout(releaseTimer);
      const dropped = e.paths && e.paths.length ? e.paths : paths;
      const { x, y } = toClient(pos);
      const i = locked >= 0 ? locked : ring.hitTest(x, y);
      const item = i >= 0 ? ring.item(i) : null;
      if (!item) { close(); return; }
      if (item.more) { ring.select(i); return; }
      // Items may still be loading if the drop came right after enter.
      if (sameSet(dropped) !== pathsKey) { paths = dropped; }
      apply(item, dropped);
    }
  }
  listen('ring://dnd', ({ payload }) => onDnd(payload));
  T.webview.getCurrentWebview().onDragDropEvent(({ payload }) => onDnd(payload));

  /* ---------- Run ---------- */
  function apply(item, onPaths) {
    trace(`apply ${item && item.id} on ${onPaths.length}`);
    if (!item || !onPaths.length) return;
    if (item.disabled) {
      // Missing optional component: open the installer instead of failing.
      invoke('open_components', { focus: item.component || null });
      ring.toast(`<b>${L.t('toast.install', 'INSTALL COMPONENT')}</b> · ${item.component || item.label}`, { ms: 1400, onHide: close });
      return;
    }
    if (item.editor) {
      invoke('open_editor', { op: item.id, paths: onPaths });
      ring.collapse();
      setTimeout(close, 200);
      return;
    }
    ring.collapse();
    job = { id: null, total: onPaths.length, saved: [], errors: [], started: performance.now(), item };
    invoke('run_op', { op: item.id, paths: onPaths }).then((id) => { if (job) job.id = id; });
  }

  const base = (p) => String(p).split(/[\\/]/).pop();

  listen('job://event', ({ payload: e }) => {
    if (!job || (job.id !== null && e.job !== job.id)) return;
    if (job.id === null) job.id = e.job;
    switch (e.event) {
      case 'started': job.total = e.total; break;
      case 'progress':
        // Long or multi-file jobs get a progress card instead of a bare toast.
        if (job.total > 1 || performance.now() - job.started > 600) {
          ring.progress(job.saved.length, job.total, e.fraction, e.message || base(job.item.out || ''));
        }
        break;
      case 'saved': job.saved.push(e.path); break;
      case 'failed': job.errors.push(e.error); break;
      case 'finished': finish(e); break;
    }
  });

  function finish(e) {
    const done = job;
    job = null;
    if (done.errors.length && !done.saved.length) {
      const [head, ...more] = String(done.errors[0]).split('\n');
      const actions = more.length ? [{
        label: L.t('toast.details', 'DETAILS'),
        run: () => { invoke('show_error', { title: (done.item && done.item.label) || 'Job', text: done.errors.join('\n\n') }); close(); },
      }] : [];
      ring.toast(`<b>${L.t('toast.failed', '✗ FAILED')}</b> · ${escapeHtml(head)}`, { error: true, ms: more.length ? 6000 : 3200, delay: 200, actions, onHide: close });
      return;
    }
    if (e.cancelled) { ring.toast(`<b>${L.t('toast.cancelled', 'CANCELLED')}</b>`, { ms: 1200, onHide: close }); return; }
    const first = done.saved[0];
    const label = done.saved.length === 1 ? base(first) : `${done.saved.length} files · ${base(first)}…`;
    const actions = first ? [
      { label: 'SHOW IN FOLDER', run: () => { invoke('reveal', { path: first }); close(); } },
      { label: L.t('toast.undo', 'UNDO'), run: () => { invoke('undo_last').then(() => ring.toast(`<b>${L.t('toast.undone', 'UNDONE')}</b> · ${L.t('toast.trash', 'moved to trash')}`, { ms: 1200, onHide: close })); } },
    ] : [];
    ring.saved(label + (done.errors.length ? ` · ${done.errors.length} failed` : ''), {
      delay: 380,
      ms: 2600,
      actions,
      onHide: close,
    });
  }

  function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  window.addEventListener('keydown', (e) => { if (e.key === 'Escape') ring.cancel(); });
})();

// Forward overlay diagnostics to the app log (MORPH_LOG=debug).
window.morphTrace = (msg) => window.__TAURI__.core.invoke('trace', { msg: String(msg) }).catch(() => {});
