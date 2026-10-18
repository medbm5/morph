/* Settings (expanded mode): trigger keys, accent, motion, ring layout per file type,
   naming and conflict policy, history, autostart, language and portable mode. */
(function (global) {
  'use strict';
  const T = global.__TAURI__;
  const invoke = T.core.invoke;
  const h = global.hudEl;
  const L = global.MorphLook;
  const t = (k, en, vars) => L.t(k, en, vars);

  const KEYS = [['shift', 'Shift'], ['ctrl', 'Ctrl'], ['alt', 'Alt'], ['super', 'Super / Win']];
  const PRESETS = [['amber', '#FFB347'], ['cyan', '#4FE3FF'], ['pink', '#FF5EA8'], ['lime', '#9CFF6E']];
  const FAMILIES = ['image', 'pdf', 'video', 'audio', 'document', 'slides', 'spreadsheet', 'subtitle', 'archive', 'folder'];
  const RINGS = ['convert', 'tools', 'actions'];

  function select(options, value, onChange) {
    const s = h('select', { style: 'max-width:340px' });
    options.forEach(([v, label]) => s.append(h('option', { value: v, text: label })));
    s.value = value;
    s.addEventListener('change', () => onChange(s.value));
    return s;
  }
  const field = (label, ...kids) => h('label', { class: 'hud-field' }, h('span', { text: label }), ...kids);
  const note = (text) => h('p', { class: 'ed-note', text });
  const section = (title, ...kids) => h('section', { class: 'hud-panel', style: 'margin-bottom:14px' }, h('h2', { text: title }), ...kids);
  const check = (label, value, onChange) => {
    const c = h('input', { type: 'checkbox' });
    c.checked = !!value;
    c.addEventListener('change', () => onChange(c.checked));
    c.style.accentColor = 'var(--hud-accent)';
    return h('label', { class: 'hud-field', style: 'flex-direction:row;align-items:center;gap:10px' }, c, h('span', { text: label }));
  };

  async function show(root, version) {
    await L.ready;
    root.classList.add('hud-grid');
    let s = await invoke('get_settings');
    const portable = await invoke('portable_status');
    const frame = h('div', { class: 'hud-frame', style: 'grid-template-columns:1fr;max-width:860px;margin:0 auto' });
    ['tl', 'tr', 'bl', 'br'].forEach((c) => frame.append(h('span', { class: `hud-corner ${c}`, 'aria-hidden': 'true' })));
    frame.append(h('header', { class: 'hud-header' },
      h('span', { class: 'hud-blink', 'aria-hidden': 'true' }), h('span', { class: 'hud-brand', text: 'METAMORPHOSIS' }),
      h('span', { class: 'hud-mono', text: `// ${t('settings.title', 'SETTINGS')} · v${version}` })));
    const body = h('div');
    frame.append(body);
    root.append(frame);

    const status = h('span', { class: 'hud-mono', role: 'status', 'aria-live': 'polite' });
    let saveTimer = 0;
    // Every change saves (debounced); the backend validates and broadcasts the new look.
    function save() {
      clearTimeout(saveTimer);
      saveTimer = setTimeout(async () => {
        try {
          await invoke('set_settings', { settings: s });
          status.textContent = t('settings.saved', 'Saved');
          status.style.color = '';
        } catch (e) {
          status.textContent = String(e);
          status.style.color = 'var(--hud-danger, #ff6b6b)';
        }
      }, 250);
    }

    /* Trigger keys */
    const keyRow = h('div', { class: 'hud-row', style: 'gap:18px' });
    RINGS.forEach((r) => keyRow.append(field(t(`ring.${r}`, r.toUpperCase()),
      select(KEYS, s.keys[r], (v) => { s.keys[r] = v; save(); }))));
    body.append(section(t('settings.keys', 'TRIGGER KEYS'), keyRow, note(t('settings.keysHelp', 'While dragging, tap this key to open the ring.'))));

    /* Accent */
    const contrastNote = note('');
    const custom = h('input', { type: 'color', 'aria-label': t('settings.custom', 'Custom') });
    const swatches = h('div', { class: 'hud-row' });
    function setAccent(hex) {
      s.accent = hex.toUpperCase();
      custom.value = s.accent.toLowerCase();
      const r = global.HudColor.accentContrast(s.accent);
      const ratio = r.toFixed(1);
      contrastNote.textContent = r < 4.5
        ? t('settings.lowContrast', `Low contrast against the ring centre (${ratio}:1, needs 4.5:1).`, { r: ratio })
        : t('settings.contrastOk', `Contrast ${ratio}:1`, { r: ratio });
      contrastNote.style.color = r < 4.5 ? 'var(--hud-danger, #ff6b6b)' : '';
      swatches.querySelectorAll('button').forEach((b) => b.classList.toggle('on', b.dataset.hex === s.accent));
    }
    PRESETS.forEach(([name, hex]) => {
      const b = h('button', { class: 'hud-btn small', type: 'button', 'data-hex': hex, text: name.toUpperCase(), style: `border-color:${hex};color:${hex}` });
      b.addEventListener('click', () => { setAccent(hex); save(); });
      swatches.append(b);
    });
    custom.addEventListener('input', () => { setAccent(custom.value); save(); });
    swatches.append(h('label', { class: 'hud-row hud-mono' }, custom, h('span', { text: t('settings.custom', 'Custom') })));
    setAccent(s.accent);
    body.append(section(t('settings.accent', 'ACCENT COLOUR'), swatches, contrastNote));

    /* Motion */
    body.append(section(t('settings.motion', 'MOTION'), field('',
      select([['system', t('motion.system', 'Follow system')], ['full', t('motion.full', 'Full')], ['reduced', t('motion.reduced', 'Reduced')]],
        s.motion, (v) => { s.motion = v; save(); }))));

    /* Ring layout */
    const layoutHost = h('div', { class: 'hud-list' });
    let fam = 'image', ring = 'convert';
    const famSel = select(FAMILIES.map((f) => [f, t(`family.${f}`, f[0].toUpperCase() + f.slice(1))]), fam, (v) => { fam = v; renderLayout(); });
    const ringSel = select(RINGS.map((r) => [r, t(`ring.${r}`, r.toUpperCase())]), ring, (v) => { ring = v; renderLayout(); });
    async function renderLayout() {
      const key = `${ring}:${fam}`;
      const ops = await invoke('layout_ops', { ring, family: fam });
      const lay = s.layouts[key] || { order: [], pinned: [], hidden: [] };
      // Current arrangement: pinned, ordered, rest (hidden ones stay listed, dimmed).
      const rank = (id) => {
        const p = lay.pinned.indexOf(id); if (p >= 0) return [0, p];
        const o = lay.order.indexOf(id); if (o >= 0) return [1, o];
        return [2, 0];
      };
      const list = ops.map((o, i) => ({ ...o, i })).sort((a, b) => {
        const [ga, pa] = rank(a.id), [gb, pb] = rank(b.id);
        return ga - gb || pa - pb || a.i - b.i;
      });
      const commit = (ids) => {
        lay.order = ids.filter((id) => !lay.pinned.includes(id));
        s.layouts[key] = lay;
        save();
        renderLayout();
      };
      layoutHost.textContent = '';
      if (!ops.length) layoutHost.append(note('—'));
      list.forEach((o, idx) => {
        const pinned = lay.pinned.includes(o.id), hidden = lay.hidden.includes(o.id);
        const row = h('div', { class: 'hud-row', style: `justify-content:space-between;border:1px solid var(--hud-line-35);padding:6px 10px;${hidden ? 'opacity:.45' : ''}` });
        row.append(h('span', { class: 'hud-mono', text: `${idx + 1}. ${L.label(o.label)}${pinned ? ' ◆' : ''}` }));
        const btns = h('div', { class: 'hud-row' });
        const btn = (label, fn, aria) => { const b = h('button', { class: 'hud-btn small', type: 'button', text: label, 'aria-label': aria || label }); b.addEventListener('click', fn); btns.append(b); };
        const ids = list.map((x) => x.id);
        btn('▲', () => { if (idx > 0) { [ids[idx - 1], ids[idx]] = [ids[idx], ids[idx - 1]]; commit(ids); } }, `Move ${o.label} up`);
        btn('▼', () => { if (idx < ids.length - 1) { [ids[idx + 1], ids[idx]] = [ids[idx], ids[idx + 1]]; commit(ids); } }, `Move ${o.label} down`);
        btn(pinned ? t('settings.unpin', 'UNPIN') : t('settings.pin', 'PIN'), () => {
          lay.pinned = pinned ? lay.pinned.filter((x) => x !== o.id) : [...lay.pinned, o.id];
          commit(ids);
        });
        btn(hidden ? t('settings.show', 'SHOW') : t('settings.hide', 'HIDE'), () => {
          lay.hidden = hidden ? lay.hidden.filter((x) => x !== o.id) : [...lay.hidden, o.id];
          commit(ids);
        });
        row.append(btns);
        layoutHost.append(row);
      });
      const reset = h('button', { class: 'hud-btn small', type: 'button', text: t('settings.reset', 'RESET') });
      reset.addEventListener('click', () => { delete s.layouts[key]; save(); renderLayout(); });
      layoutHost.append(h('div', { class: 'hud-row' }, reset));
    }
    body.append(section(t('settings.layout', 'RING LAYOUT'),
      note(t('settings.layoutHelp', 'Pin, reorder or hide segments for each file type.')),
      h('div', { class: 'hud-row', style: 'gap:18px' }, field(t('settings.family', 'Type'), famSel), field(t('settings.ring', 'Ring'), ringSel)),
      layoutHost));
    renderLayout();

    /* Naming */
    const preview = h('p', { class: 'hud-mono' });
    const sample = () => {
      const render = (tpl, op, ext) => tpl.replace('{stem}', 'Page 001').replace('{op}', op).replace('{ext}', ext).replace('{date}', new Date().toISOString().slice(0, 10));
      const conflict = { number: 'Page 001 Cropped 2.jpg', timestamp: 'Page 001 Cropped 2026-…jpg', skip: '—' }[s.naming.conflict];
      preview.textContent = `${t('settings.preview', 'Preview')}: Page 001.pdf → ${render(s.naming.convert, '', 'jpg')} · ${render(s.naming.tool, 'Cropped', 'jpg')} · ${conflict}`;
    };
    const tpl = (k) => {
      const i = h('input', { type: 'text', spellcheck: 'false' });
      i.value = s.naming[k];
      i.addEventListener('input', () => { s.naming[k] = i.value; sample(); save(); });
      return i;
    };
    body.append(section(t('settings.naming', 'OUTPUT NAMES'),
      field(t('settings.convertName', 'Conversions'), tpl('convert')),
      field(t('settings.toolName', 'Tools'), tpl('tool')),
      note(t('settings.namingHelp', 'Variables: {stem} original name, {op} operation, {ext} extension, {date} today.')),
      field(t('settings.conflict', 'When the name is taken'), select([
        ['number', t('conflict.number', 'Keep both (photo 2.png)')],
        ['timestamp', t('conflict.timestamp', 'Keep both, timestamped')],
        ['skip', t('conflict.skip', 'Skip the new file')],
      ], s.naming.conflict, (v) => { s.naming.conflict = v; sample(); save(); })),
      preview));
    sample();

    /* History */
    const openHistory = h('button', { class: 'hud-btn small', type: 'button', text: t('settings.openHistory', 'OPEN HISTORY') });
    openHistory.addEventListener('click', () => invoke('open_history'));
    body.append(section(t('settings.history', 'HISTORY'),
      check(t('settings.keepHistory', 'Record jobs (Undo, Run again)'), s.history, (v) => { s.history = v; save(); }),
      h('div', { class: 'hud-row' }, openHistory)));

    /* System */
    const sys = section(t('settings.system', 'SYSTEM'),
      check(t('settings.autostart', 'Start with the session'), s.autostart, (v) => { s.autostart = v; save(); }),
      field(t('settings.language', 'Language'), select([['auto', t('lang.auto', 'Follow system')], ['en', 'English'], ['fr', 'Français']],
        s.language, (v) => { s.language = v; save(); })));
    if (portable.supported) {
      const restart = note('');
      sys.append(check(t('settings.portable', 'Portable mode'), portable.on, async (v) => {
        try {
          await invoke('set_portable', { on: v });
          restart.textContent = t('settings.restart', 'Restart Metamorphosis to apply.');
        } catch (e) { restart.textContent = String(e); }
      }), note(t('settings.portableHelp', `Settings, recipes and history next to the app (${portable.folder}). Needs a restart.`, { dir: portable.folder })), restart);
    }
    body.append(sys);
    body.append(h('div', { class: 'hud-row', style: 'justify-content:flex-end' }, status));

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
