/* Settings (expanded mode), in plain words. Every change is saved right away. */
(function (global) {
  'use strict';
  const T = global.__TAURI__;
  const invoke = T.core.invoke;
  const h = global.hudEl;
  const L = global.MorphLook;
  const t = (k, en, vars) => L.t(k, en, vars);

  const KEYS = [['shift', 'Shift'], ['ctrl', 'Ctrl'], ['alt', 'Alt'], ['super', 'Windows key']];
  const PRESETS = [['amber', '#FFB347'], ['cyan', '#4FE3FF'], ['pink', '#FF5EA8'], ['lime', '#9CFF6E']];
  const FAMILIES = ['image', 'pdf', 'video', 'audio', 'document', 'slides', 'spreadsheet', 'subtitle', 'archive', 'folder'];
  const RINGS = ['convert', 'tools', 'actions'];
  // Naming choices; any other template shows as "Custom".
  const CONVERT_NAMES = [['{stem}.{ext}', 's.nameSame', 'Same name: photo.png'], ['{stem} {date}.{ext}', 's.nameDate', 'Same name + date: photo 2026-10-19.png']];
  const TOOL_NAMES = [['{stem} {op}.{ext}', 's.nameOp', 'Add what was done: photo Cropped.jpg'], ['{stem} {op} {date}.{ext}', 's.nameOpDate', 'Add what was done + date: photo Cropped 2026-10-19.jpg']];

  /* ---------- Building blocks ---------- */
  function select(options, value, onChange, label) {
    const s = h('select', { style: 'width:100%;max-width:420px' });
    if (label) s.setAttribute('aria-label', label);
    options.forEach(([v, text]) => s.append(h('option', { value: v, text })));
    s.value = value;
    s.addEventListener('change', () => onChange(s.value));
    return s;
  }
  const help = (text) => h('p', { class: 'ed-note', style: 'font-size:13px;line-height:1.5;margin:4px 0 10px;text-transform:none;letter-spacing:0', text });
  // One setting: what it is on the left, the control on the right.
  function row(title, hint, control) {
    // The row title names the control for screen readers.
    const fields = control.matches('select, input') ? [control] : [...control.querySelectorAll('select, input:not([type=checkbox])')];
    fields.forEach((f) => { if (!f.hasAttribute('aria-label')) f.setAttribute('aria-label', title); });
    const left = h('div', {}, h('div', { style: 'font:600 14px var(--font-label);letter-spacing:1px;color:var(--hud-text)', text: title }));
    if (hint) left.append(help(hint));
    return h('div', { style: 'display:grid;grid-template-columns:minmax(0,1fr) minmax(0,420px);gap:8px 28px;align-items:start;padding:14px 0;border-top:1px solid var(--hud-line-35)' }, left, h('div', {}, control));
  }
  function section(title, intro, ...rows) {
    const s = h('section', { class: 'hud-panel', style: 'margin-bottom:18px' }, h('h2', { text: title }));
    if (intro) s.append(help(intro));
    rows.forEach((r) => s.append(r));
    return s;
  }
  function toggle(label, value, onChange) {
    const c = h('input', { type: 'checkbox', style: 'width:18px;height:18px;accent-color:var(--hud-accent)' });
    c.checked = !!value;
    c.addEventListener('change', () => onChange(c.checked));
    return h('label', { style: 'display:inline-flex;align-items:center;gap:10px;font:14px var(--font-label);color:var(--hud-text);cursor:pointer' }, c, h('span', { text: label }));
  }
  function button(text, fn, attrs = {}) {
    const b = h('button', { class: 'hud-btn small', type: 'button', text, ...attrs });
    b.addEventListener('click', fn);
    return b;
  }

  async function show(root, version) {
    await L.ready;
    root.classList.add('hud-grid');
    const s = await invoke('get_settings');
    const portable = await invoke('portable_status');
    s.watch = s.watch || [];

    const frame = h('div', { class: 'hud-frame hud-settings', style: 'grid-template-columns:1fr;max-width:900px;margin:0 auto' });
    ['tl', 'tr', 'bl', 'br'].forEach((c) => frame.append(h('span', { class: `hud-corner ${c}`, 'aria-hidden': 'true' })));
    const status = h('span', { class: 'hud-mono', role: 'status', 'aria-live': 'polite', style: 'margin-left:auto' });
    frame.append(h('header', { class: 'hud-header' },
      h('span', { class: 'hud-brand', text: 'METAMORPHOSIS' }),
      h('span', { class: 'hud-mono', text: `${t('s.title', 'SETTINGS')} · v${version}` }), status));
    const body = h('div');
    frame.append(body);
    root.append(frame);

    let timer = 0;
    function save() {
      clearTimeout(timer);
      timer = setTimeout(async () => {
        try {
          await invoke('set_settings', { settings: s });
          status.textContent = `✓ ${t('s.saved', 'Saved')}`;
          status.style.color = '';
        } catch (e) {
          status.textContent = String(e);
          status.style.color = 'var(--hud-danger, #ff6b6b)';
        }
      }, 250);
    }

    body.append(help(t('s.intro', 'Metamorphosis runs in the background (look for its icon in the tray). Start dragging a file, tap a key, and drop the file on the ring.')));

    /* ---------- Keys ---------- */
    const keyRow = (ring, title) => row(title, null, select(KEYS, s.keys[ring], (v) => { s.keys[ring] = v; save(); }, title));
    body.append(section(t('s.keys', 'KEYS'),
      t('s.keysIntro', 'While you drag a file, tap one of these keys to open a ring. Tap it, don’t hold it. Each ring needs its own key.'),
      keyRow('convert', t('s.keyConvert', 'Convert (change the format)')),
      keyRow('tools', t('s.keyTools', 'Tools (compress, crop, edit…)')),
      keyRow('actions', t('s.keyActions', 'Actions (copy, move, share…)'))));

    /* ---------- Appearance ---------- */
    const contrastNote = help('');
    const custom = h('input', { type: 'color', 'aria-label': t('s.customColour', 'Pick any colour'), style: 'width:44px;height:30px;padding:0;border:0;background:none;cursor:pointer' });
    const swatches = h('div', { class: 'hud-row' });
    function setAccent(hex) {
      s.accent = hex.toUpperCase();
      custom.value = s.accent.toLowerCase();
      const low = global.HudColor.accentContrast(s.accent) < 4.5;
      contrastNote.textContent = low ? t('s.lowContrast', 'This colour is hard to read on the ring. Try a brighter one.') : '';
      contrastNote.style.color = low ? 'var(--hud-danger, #ff6b6b)' : '';
      swatches.querySelectorAll('button').forEach((b) => b.classList.toggle('on', b.dataset.hex === s.accent));
    }
    PRESETS.forEach(([name, hex]) => swatches.append(button(t(`s.c.${name}`, name[0].toUpperCase() + name.slice(1)),
      () => { setAccent(hex); save(); }, { 'data-hex': hex, style: `border-color:${hex};color:${hex}` })));
    custom.addEventListener('input', () => { setAccent(custom.value); save(); });
    swatches.append(custom);
    setAccent(s.accent);
    body.append(section(t('s.look', 'APPEARANCE'), null,
      row(t('s.colour', 'Colour'), t('s.colourHint', 'The highlight colour of the ring.'), h('div', {}, swatches, contrastNote)),
      row(t('s.motion', 'Animations'), t('s.motionHint', 'Reduced: no spinning or flicker, just a quick fade.'),
        select([['system', t('s.motionSystem', 'Same as my computer')], ['full', t('s.motionFull', 'On')], ['reduced', t('s.motionReduced', 'Reduced')]], s.motion, (v) => { s.motion = v; save(); })),
      row(t('s.language', 'Language'), null,
        select([['auto', t('s.langAuto', 'Same as my computer')], ['en', 'English'], ['fr', 'Français']], s.language, (v) => { s.language = v; save(); }))));

    /* ---------- New files ---------- */
    const preview = help('');
    const sample = () => {
      const date = new Date().toISOString().slice(0, 10);
      const render = (tpl, op, ext) => tpl.replace('{stem}', 'photo').replace('{op}', op).replace('{ext}', ext).replace('{date}', date).replace(/\s+\./, '.');
      const clash = { number: 'photo 2.png', timestamp: `photo ${date} 18-30-00.png`, skip: t('s.notSaved', 'not saved') }[s.naming.conflict];
      preview.textContent = `${t('s.example', 'Example')}: photo.jpg → ${render(s.naming.convert, '', 'png')} · ${render(s.naming.tool, 'Cropped', 'jpg')} · ${t('s.ifTaken', 'if the name is taken')}: ${clash}`;
    };
    function namePicker(key, choices) {
      const customBox = h('input', { type: 'text', spellcheck: 'false', style: 'width:100%;max-width:420px;margin-top:8px' });
      customBox.value = s.naming[key];
      const isPreset = choices.some(([v]) => v === s.naming[key]);
      customBox.hidden = isPreset;
      const sel = select([...choices.map(([v, k, en]) => [v, t(k, en)]), ['custom', t('s.nameCustom', 'Custom…')]],
        isPreset ? s.naming[key] : 'custom', (v) => {
          customBox.hidden = v !== 'custom';
          if (v !== 'custom') { s.naming[key] = v; customBox.value = v; save(); }
          sample();
        });
      customBox.addEventListener('input', () => { s.naming[key] = customBox.value; sample(); save(); });
      return h('div', {}, sel, customBox);
    }
    body.append(section(t('s.files', 'NEW FILES'),
      t('s.filesIntro', 'Results are saved next to the original. The original is never changed.'),
      row(t('s.converted', 'Name of converted files'), null, namePicker('convert', CONVERT_NAMES)),
      row(t('s.edited', 'Name of edited files'), t('s.customHint', 'Custom names can use {stem} (original name), {op} (what was done), {ext} (extension) and {date}.'), namePicker('tool', TOOL_NAMES)),
      row(t('s.taken', 'If a file with that name already exists'), null,
        select([['number', t('s.takenNumber', 'Keep both, add a number (photo 2.png)')], ['timestamp', t('s.takenTime', 'Keep both, add the time')], ['skip', t('s.takenSkip', 'Don’t save the new file')]],
          s.naming.conflict, (v) => { s.naming.conflict = v; sample(); save(); })),
      preview));
    sample();

    /* ---------- What the ring shows ---------- */
    const layoutHost = h('div', { style: 'display:grid;gap:6px' });
    let fam = 'image', ring = 'convert';
    async function renderLayout() {
      const key = `${ring}:${fam}`;
      const ops = await invoke('layout_ops', { ring, family: fam });
      const lay = s.layouts[key] || { order: [], pinned: [], hidden: [] };
      const rank = (id) => {
        const p = lay.pinned.indexOf(id); if (p >= 0) return [0, p];
        const o = lay.order.indexOf(id); if (o >= 0) return [1, o];
        return [2, 0];
      };
      const list = ops.map((o, i) => ({ ...o, i })).sort((a, b) => {
        const [ga, pa] = rank(a.id), [gb, pb] = rank(b.id);
        return ga - gb || pa - pb || a.i - b.i;
      });
      const commit = (ids) => { lay.order = ids.filter((id) => !lay.pinned.includes(id)); s.layouts[key] = lay; save(); renderLayout(); };
      layoutHost.textContent = '';
      if (!ops.length) layoutHost.append(help(t('s.nothing', 'Nothing to arrange for this combination.')));
      list.forEach((o, idx) => {
        const pinned = lay.pinned.includes(o.id), hidden = lay.hidden.includes(o.id);
        const ids = list.map((x) => x.id);
        const r = h('div', { class: 'hud-row', style: `justify-content:space-between;border:1px solid var(--hud-line-35);padding:6px 10px;${hidden ? 'opacity:.45' : ''}` });
        r.append(h('span', { class: 'hud-mono', text: `${idx + 1}. ${L.label(o.label)}${pinned ? ` · ${t('s.first', 'always first')}` : ''}${hidden ? ` · ${t('s.hidden', 'hidden')}` : ''}` }));
        const btns = h('div', { class: 'hud-row' });
        btns.append(button('▲', () => { if (idx > 0) { [ids[idx - 1], ids[idx]] = [ids[idx], ids[idx - 1]]; commit(ids); } }, { 'aria-label': `${t('s.up', 'Move up')}: ${o.label}` }));
        btns.append(button('▼', () => { if (idx < ids.length - 1) { [ids[idx + 1], ids[idx]] = [ids[idx], ids[idx + 1]]; commit(ids); } }, { 'aria-label': `${t('s.down', 'Move down')}: ${o.label}` }));
        btns.append(button(pinned ? t('s.unpin', 'Unpin') : t('s.pin', 'Always first'), () => {
          lay.pinned = pinned ? lay.pinned.filter((x) => x !== o.id) : [...lay.pinned, o.id]; commit(ids);
        }));
        btns.append(button(hidden ? t('s.show', 'Show') : t('s.hide', 'Hide'), () => {
          lay.hidden = hidden ? lay.hidden.filter((x) => x !== o.id) : [...lay.hidden, o.id]; commit(ids);
        }));
        r.append(btns);
        layoutHost.append(r);
      });
      if (s.layouts[key]) layoutHost.append(h('div', {}, button(t('s.reset', 'Back to default'), () => { delete s.layouts[key]; save(); renderLayout(); })));
    }
    body.append(section(t('s.ring', 'WHAT THE RING SHOWS'),
      t('s.ringIntro', 'Hide options you never use, or move your favourites to the top. Pick a file type and a ring:'),
      h('div', { class: 'hud-row', style: 'gap:12px;margin-bottom:10px;flex-wrap:nowrap' },
        select(FAMILIES.map((f) => [f, t(`family.${f}`, f[0].toUpperCase() + f.slice(1))]), fam, (v) => { fam = v; renderLayout(); }, t('s.fileType', 'File type')),
        select(RINGS.map((r) => [r, t(`s.ringName.${r}`, { convert: 'Convert ring', tools: 'Tools ring', actions: 'Actions ring' }[r])]), ring, (v) => { ring = v; renderLayout(); }, t('s.whichRing', 'Ring'))),
      layoutHost));
    renderLayout();

    /* ---------- Watch folders ---------- */
    const ops = await invoke('watch_ops');
    const opName = (o) => (o.ring === 'convert' ? `${t('s.convertTo', 'Convert to')} ${o.label}` : L.label(o.label));
    const watchList = h('div', { style: 'display:grid;gap:10px' });
    function renderWatch() {
      watchList.textContent = '';
      if (!s.watch.length) watchList.append(help(t('s.noWatch', 'No folders yet.')));
      s.watch.forEach((w, i) => {
        const outBox = h('input', { type: 'text', spellcheck: 'false', style: 'width:100%;max-width:420px' });
        outBox.value = w.out;
        outBox.addEventListener('change', () => { w.out = outBox.value.trim() || 'Metamorphosis'; outBox.value = w.out; save(); });
        watchList.append(h('div', { style: 'border:1px solid var(--hud-line-35);padding:12px 14px' },
          h('div', { class: 'hud-row', style: 'justify-content:space-between;margin-bottom:4px' },
            h('span', { class: 'hud-mono', style: 'overflow-wrap:anywhere', text: w.folder }),
            button(t('s.remove', 'Remove'), () => { s.watch.splice(i, 1); save(); renderWatch(); })),
          row(t('s.doWhat', 'What to do with new files'), null, select(ops.map((o) => [o.id, opName(o)]), w.op, (v) => { w.op = v; save(); })),
          row(t('s.putWhere', 'Put the results in this subfolder'), null, outBox),
          row(t('s.watchActive', 'Watching'), null, toggle(t('s.on', 'On'), w.enabled, (v) => { w.enabled = v; save(); }))));
      });
    }
    renderWatch();
    body.append(section(t('s.watch', 'WATCH FOLDERS'),
      t('s.watchIntro', 'Files you put in a watched folder are processed automatically. For example, every photo saved to “Inbox” gets compressed.'),
      watchList,
      h('div', { style: 'margin-top:10px' }, button(t('s.addFolder', 'Add a folder…'), async () => {
        const folder = await invoke('pick_folder');
        if (!folder) return;
        const first = ops.find((o) => o.id === 'image.compress') || ops[0];
        s.watch.push({ folder, op: first ? first.id : '', out: 'Metamorphosis', enabled: true });
        save();
        renderWatch();
      }))));

    /* ---------- General ---------- */
    const advanced = h('details', { style: 'margin-top:14px' }, h('summary', { style: 'cursor:pointer;font:600 13px var(--font-label);letter-spacing:1px;color:var(--hud-muted)', text: t('s.advanced', 'Advanced') }));
    if (portable.supported) {
      const restart = help('');
      advanced.append(row(t('s.portable', 'Portable mode'),
        t('s.portableHint', `Keep settings and history next to the app (${portable.folder}), for example to run it from a USB stick. Restart the app afterwards.`, { dir: portable.folder }),
        h('div', {}, toggle(t('s.on', 'On'), portable.on, async (v) => {
          try { await invoke('set_portable', { on: v }); restart.textContent = t('s.restart', 'Restart Metamorphosis to apply this.'); } catch (e) { restart.textContent = String(e); }
        }), restart)));
    }
    body.append(section(t('s.general', 'GENERAL'), null,
      row(t('s.autostart', 'Start when I sign in'), t('s.autostartHint', 'Metamorphosis waits quietly in the tray until you drag a file.'),
        toggle(t('s.on', 'On'), s.autostart, (v) => { s.autostart = v; save(); })),
      row(t('s.history', 'Remember my jobs'), t('s.historyHint', 'Needed for Undo and Run again.'),
        h('div', { style: 'display:grid;gap:10px;justify-items:start' },
          toggle(t('s.on', 'On'), s.history, (v) => { s.history = v; save(); }),
          button(t('s.openHistory', 'Open history'), () => invoke('open_history')))),
      advanced));

    current = { root, version, frame, lang: L.lang() };
  }

  // Re-render in the new language when it changes while settings are open.
  let current = null;
  L.onChange(() => {
    const c = current;
    if (c && document.body.contains(c.frame) && L.lang() !== c.lang) {
      c.root.textContent = '';
      show(c.root, c.version);
    }
  });

  global.MorphSettings = { show };
})(window);
