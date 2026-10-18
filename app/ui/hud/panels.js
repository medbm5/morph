/* Metamorphosis — expanded-mode frame (header, TARGET ACQUIRED, centre, ACTIONS, output preview).

   const panel = createHudPanel(root, { version: '0.2.0', title: 'ACTIONS · KEYS 1–7', footer: true });
   panel.setTarget({ name, type, size, dim, state, thumb })
   panel.setActions(items, { onPick(item, i) })   panel.setHot(i)
   panel.setOutput(name)   panel.setStatus(text, 'armed' | 'ok' | 'error')
   panel.centre — element to mount the ring or an editor canvas into
   panel.left / panel.right — extra content areas under the panels
*/
(function (global) {
  'use strict';
  const h = (tag, attrs = {}, ...kids) => {
    const n = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k === 'class') n.className = v;
      else if (k === 'text') n.textContent = v;
      else n.setAttribute(k, v);
    }
    kids.flat().forEach((k) => k != null && n.append(k));
    return n;
  };

  function createHudPanel(root, opts = {}) {
    root.classList.add('hud-grid');
    const frame = h('div', { class: 'hud-frame' + (opts.wide ? ' is-wide' : '') });
    ['tl', 'tr', 'bl', 'br'].forEach((c) => frame.append(h('span', { class: `hud-corner ${c}`, 'aria-hidden': 'true' })));

    const version = h('span', { class: 'hud-mono', text: `// STAND ACTIVE · v${opts.version || '0.x'}` });
    const headerRight = h('span', { class: 'hud-spacer' });
    const header = h('header', { class: 'hud-header' },
      h('span', { class: 'hud-blink', 'aria-hidden': 'true' }), h('span', { class: 'hud-brand', text: 'METAMORPHOSIS' }), version, headerRight);
    const tools = h('div', { class: 'hud-row' });
    header.append(tools);

    const thumb = h('div', { class: 'hud-thumb', 'aria-hidden': 'true' });
    const dl = h('dl', { class: 'hud-dl' });
    const left = h('div');
    const target = h('section', { class: 'hud-panel', 'aria-labelledby': 'hud-t' },
      h('h2', { id: 'hud-t', text: opts.targetTitle || 'TARGET ACQUIRED' }), thumb, dl, left);

    const centre = h('div', { class: 'hud-centre' });

    const list = h('div', { class: 'hud-list' });
    const outName = h('span', { class: 'hud-out-name' });
    const status = h('span', { class: 'hud-armed', text: 'AWAITING DROP' });
    const out = h('div', { class: 'hud-out', 'aria-live': 'polite' },
      h('span', { text: 'OUTPUT' }), outName, h('span', { text: 'SOURCE UNTOUCHED · SAVED BESIDE ORIGINAL' }), status);
    const right = h('div');
    const actions = h('section', { class: 'hud-panel', 'aria-labelledby': 'hud-a' },
      h('h2', { id: 'hud-a', text: opts.title || 'ACTIONS · KEYS 1–7' }), list, out, right);

    frame.append(header, target, centre, actions);
    if (opts.footer !== false) {
      frame.append(h('footer', { class: 'hud-footer' },
        h('span', { class: 'hud-key', text: opts.footerKey || 'SHIFT' }), h('span', { text: '+ DRAG' }),
        h('span', { 'aria-hidden': 'true', style: 'color:var(--hud-line)', text: '▸' }), h('span', { text: 'DROP ON A SEGMENT' })));
    }
    root.append(frame);

    let buttons = [];
    let onPick = null;

    return {
      frame, centre, left, right, tools, header, out,
      setVersion(v) { version.textContent = `// STAND ACTIVE · v${v}`; },
      setTarget(t) {
        dl.textContent = '';
        const rows = [['FILE', t.name], ['TYPE', t.type], ['SIZE', t.size], ['DIM', t.dim], ['STATE', t.state || 'ARMED']];
        rows.filter(([, v]) => v).forEach(([k, v]) => {
          dl.append(h('dt', { text: k }), h('dd', { class: k === 'STATE' ? 'hud-armed' : '', text: v }));
        });
        thumb.style.backgroundImage = t.thumb ? `url("${t.thumb}")` : '';
        thumb.hidden = t.thumb === false;
      },
      setActions(items, o = {}) {
        onPick = o.onPick || null;
        list.textContent = '';
        buttons = items.map((it, i) => {
          const b = h('button', { type: 'button' },
            h('span', { class: 'hud-mono', text: String(i + 1).padStart(2, '0') }), h('span', { text: it.label.toUpperCase() }));
          if (it.badge) b.append(h('span', { class: 'hud-mono', text: it.badge }));
          b.disabled = !!it.disabled && !it.badge;
          b.addEventListener('click', () => onPick && onPick(it, i));
          b.addEventListener('mouseenter', () => o.onHover && o.onHover(it, i));
          list.append(b);
          return b;
        });
      },
      setHot(i) { buttons.forEach((b, j) => b.classList.toggle('on', j === i)); },
      setOutput(name) { outName.textContent = name ? `→ ${name}` : ''; },
      setStatus(text, state = 'armed') {
        status.textContent = text;
        status.className = state === 'error' ? 'hud-danger' : 'hud-armed';
      },
    };
  }

  global.createHudPanel = createHudPanel;
  global.hudEl = h;
})(window);
