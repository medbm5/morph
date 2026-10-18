/* Metadata editor: view EXIF / GPS / XMP / IPTC, edit common fields, strip everything. */
(function () {
  'use strict';
  const EDITABLE = [
    ['artist', 'Author', ['Artist']],
    ['copyright', 'Copyright', ['Copyright']],
    ['description', 'Description', ['ImageDescription']],
    ['datetime', 'Date taken', ['DateTimeOriginal']],
    ['make', 'Camera make', ['Make']],
    ['model', 'Camera model', ['Model']],
    ['software', 'Software', ['Software']],
  ];

  window.MorphEditors.metadata = async (ctx) => {
    const { h, panel } = ctx;
    const many = ctx.paths.length > 1;
    const fields = many ? [] : await ctx.invoke('image_metadata', { path: ctx.paths[0] });
    const wrap = h('div', { class: 'ed-wrap', style: 'overflow:auto' });
    const form = h('div', { class: 'ed-form' });

    if (many) {
      form.append(h('p', { class: 'ed-note', text: `${ctx.paths.length} files selected: strip all metadata or edit shared fields.` }));
    }
    const inputs = {};
    EDITABLE.forEach(([key, label, tags]) => {
      const cur = fields.find((f) => f.group === 'EXIF' && tags.includes(f.key));
      const i = h('input', { type: 'text', placeholder: '—' });
      i.value = cur ? cur.value.replace(/^"|"$/g, '') : '';
      i.dataset.orig = i.value;
      inputs[key] = i;
      form.append(h('label', { class: 'hud-field' }, h('span', { text: label }), i));
    });
    const gps = fields.filter((f) => f.group === 'GPS');
    const rmGps = h('input', { type: 'checkbox' });
    const rmXmp = h('input', { type: 'checkbox' });
    form.append(
      h('label', { class: 'hud-field hud-row' }, rmGps, h('span', { text: `Remove location${gps.length ? ` (${gps.length} GPS tags)` : ''}` })),
      h('label', { class: 'hud-field hud-row' }, rmXmp, h('span', { text: 'Remove XMP and IPTC' })),
    );
    const save = h('button', { class: 'hud-btn primary', type: 'button', text: 'SAVE CHANGES' });
    const strip = h('button', { class: 'hud-btn', type: 'button', text: 'STRIP ALL' });
    form.append(h('div', { class: 'ed-actions' }, strip, save));
    wrap.append(form);

    if (fields.length) {
      const table = h('dl', { class: 'hud-dl', style: 'max-width:720px;margin:24px auto 0;grid-template-columns:70px 200px 1fr' });
      fields.forEach((f) => table.append(
        h('dt', { text: f.group }), h('dt', { style: 'color:var(--hud-text)', text: f.key }), h('dd', { text: f.value })));
      wrap.append(h('h2', { class: 'hud-panel', style: 'max-width:720px;margin:28px auto 0;font-size:12px;letter-spacing:3px;color:var(--hud-line)', text: `ALL FIELDS · ${fields.length}` }), table);
    } else if (!many) {
      wrap.append(h('p', { class: 'ed-note', style: 'text-align:center', text: 'No metadata found.' }));
    }
    panel.centre.append(wrap);

    save.addEventListener('click', () => {
      const set = {};
      Object.entries(inputs).forEach(([k, i]) => { if (i.value !== i.dataset.orig) set[k] = i.value; });
      ctx.run({ set, remove_gps: rmGps.checked, remove_xmp_iptc: rmXmp.checked });
    });
    strip.addEventListener('click', async () => {
      panel.setStatus('STRIPPING…');
      const id = await ctx.invoke('run_op', { op: 'image.strip', paths: ctx.paths });
      const saved = [];
      const un = await window.__TAURI__.event.listen('job://event', ({ payload: e }) => {
        if (e.job !== id) return;
        if (e.event === 'saved') saved.push(e.path);
        if (e.event === 'failed') panel.setStatus(`✗ ${e.error}`, 'error');
        if (e.event === 'finished') { un(); if (saved.length) panel.setStatus(`✓ SAVED · ${saved.map((p) => p.split(/[\\/]/).pop()).join(', ')}`); }
      });
    });
  };
})();
