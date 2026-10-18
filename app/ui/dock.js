/* Metamorphosis — drop dock: expands into the ring when files are dragged onto it. */
(() => {
  const T = window.__TAURI__, invoke = T.core.invoke;
  const L = window.MorphLook;
  const NAMES = ['convert', 'tools', 'actions'];
  const tabs = document.getElementById('tabs');
  let kind = 'convert', paths = [], expanded = false, busy = false, hoverTab = null, hoverAt = 0;
  const ring = createHudRing(document.getElementById('host'), { items: [], boot: false });
  NAMES.forEach((n) => { const s = document.createElement('span'); s.textContent = L.t(`ring.${n}`, n.toUpperCase()); s.dataset.k = n; tabs.append(s); });
  L.onChange((look) => {
    ring.setReduced(look.reduced);
    tabs.querySelectorAll('span').forEach((t) => { t.textContent = L.t(`ring.${t.dataset.k}`, t.dataset.k.toUpperCase()); });
  });
  const markTabs = () => tabs.querySelectorAll('span').forEach((s) => s.classList.toggle('on', s.dataset.k === kind));

  async function load() {
    markTabs();
    const r = await invoke('ring_items', { paths, ring: kind });
    ring.setItems(r.items.map((it) => ({ ...it, label: L.label(it.label) })), { ringName: L.t(`ring.${kind}`, kind.toUpperCase()), file: { size: r.file.size, type: r.file.type }, animate: 'switch' });
  }
  async function expand(on) {
    if (on === expanded) return;
    expanded = on;
    await invoke('dock_expand', { expand: on });
    document.body.classList.toggle('expanded', on);
    if (on) ring.boot();
  }
  const base = (p) => String(p).split(/[\\/]/).pop();
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
  const toClient =(p) => ({ x: p.x / devicePixelRatio, y: p.y / devicePixelRatio });

  T.webview.getCurrentWebview().onDragDropEvent(async ({ payload: e }) => {
    if (busy) return;
    if (e.type === 'enter') {
      if (!e.paths || !e.paths.length) return;
      paths = e.paths;
      await expand(true);
      load();
    } else if (e.type === 'over') {
      const { x, y } = toClient(e.position);
      // No keys on Wayland: hovering a tab for 350 ms switches the ring.
      const tab = document.elementFromPoint(x, y)?.closest?.('#tabs span');
      if (tab && tab.dataset.k !== kind) {
        if (hoverTab !== tab) { hoverTab = tab; hoverAt = performance.now(); }
        else if (performance.now() - hoverAt > 350) { kind = tab.dataset.k; hoverTab = null; load(); }
      } else hoverTab = null;
      const i = ring.hitTest(x, y);
      if (i >= 0) ring.setHot(i);
    } else if (e.type === 'leave') {
      setTimeout(() => { if (!busy) expand(false); }, 600);
    } else if (e.type === 'drop') {
      const { x, y } = toClient(e.position);
      const i = ring.hitTest(x, y);
      const item = i >= 0 ? ring.item(i) : null;
      if (!item) { expand(false); return; }
      if (item.disabled) { invoke('open_components', { focus: item.component || null }); expand(false); return; }
      if (item.editor) { invoke('open_editor', { op: item.id, paths: e.paths || paths }); expand(false); return; }
      busy = true;
      ring.collapse();
      const id = await invoke('run_op', { op: item.id, paths: e.paths || paths });
      const saved = [], errors = [];
      const un = await T.event.listen('job://event', ({ payload: ev }) => {
        if (ev.job !== id) return;
        if (ev.event === 'saved') saved.push(ev.path);
        if (ev.event === 'failed') errors.push(ev.error);
        if (ev.event === 'finished') {
          un();
          const done = () => { busy = false; expand(false); };
          if (errors.length && !saved.length) ring.toast(`<b>${L.t('toast.failed', '✗ FAILED')}</b> · ${esc(errors[0])}`, { error: true, ms: 2600, onHide: done });
          else ring.saved(saved.length === 1 ? base(saved[0]) : `${saved.length || 'DONE'} · ${item.label}`, { delay: 380, ms: 1800, onHide: done });
        }
      });
    }
  });
})();
