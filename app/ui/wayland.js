/* First-run explanation for Wayland sessions: why the ring becomes a drop dock. */
(function (global) {
  'use strict';
  const T = global.__TAURI__;
  const h = global.hudEl;

  function show(root, version) {
    root.classList.add('hud-grid');
    const frame = h('div', { class: 'hud-frame', style: 'grid-template-columns:1fr;max-width:760px;margin:0 auto' });
    ['tl', 'tr', 'bl', 'br'].forEach((c) => frame.append(h('span', { class: `hud-corner ${c}`, 'aria-hidden': 'true' })));
    frame.append(h('header', { class: 'hud-header' },
      h('span', { class: 'hud-blink', 'aria-hidden': 'true' }), h('span', { class: 'hud-brand', text: 'METAMORPHOSIS' }),
      h('span', { class: 'hud-mono', text: `// WAYLAND MODE · v${version}` })));
    const p = (text) => h('p', { class: 'ed-note', style: 'font-size:14px;line-height:1.6', text });
    const body = h('section', { class: 'hud-panel' },
      h('h2', { text: 'DROP DOCK' }),
      p('This desktop runs Wayland, which keeps apps from watching the keyboard and mouse globally. ' +
        'That protects you, but it means Metamorphosis can’t notice a drag and open the ring next to your cursor.'),
      p('Instead, a small dock sits at the right edge of the screen. Drag files onto it and it opens into the ring. ' +
        'Hover CONVERT, TOOLS or ACTIONS for a moment to switch rings, then drop on a segment.'),
      p('Ctrl+Alt+M brings the dock back if you hid it (your desktop may ask you to confirm the shortcut). ' +
        'The file-manager menu (right-click › Metamorphosis) works as everywhere else.'),
      p('On KDE Plasma and wlroots desktops the dock stays pinned above other windows. On GNOME it is a normal always-on-top window you can move.'));
    const row = h('div', { class: 'hud-row', style: 'margin-top:14px' });
    const ok = h('button', { class: 'hud-btn', type: 'button', text: 'GOT IT' });
    ok.addEventListener('click', () => T.window.getCurrentWindow().hide());
    row.append(ok);
    body.append(row);
    frame.append(body);
    root.append(frame);
  }

  global.MorphWayland = { show };
})(window);
