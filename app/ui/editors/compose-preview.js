/* Shared preview for SVG compositions: asks Rust for the scene (the exact SVG the export
   renders), swaps `morph:i` hrefs for webview file URLs and shows it. */
(function (global) {
  'use strict';
  const T = global.__TAURI__;

  /** Mount into `stage`; returns { update(params), toImage(clientX, clientY), size } */
  function createComposePreview(stage, ctx, op) {
    const holder = document.createElement('div');
    holder.style.cssText = 'position:relative;max-width:100%;max-height:100%;display:grid;place-items:center;width:100%;height:100%';
    stage.append(holder);
    let seq = 0;
    let size = [1, 1];
    let pending = null;

    async function update(params) {
      const my = ++seq;
      pending = params;
      await new Promise((r) => setTimeout(r, 30)); // coalesce slider drags
      if (my !== seq) return;
      try {
        const p = await T.core.invoke('compose_preview', { op, paths: ctx.paths, params: pending });
        if (my !== seq) return;
        let svg = p.svg;
        p.sources.forEach((src, i) => { svg = svg.split(`"morph:${i}"`).join(`"${T.core.convertFileSrc(src)}"`); });
        size = [p.width, p.height];
        holder.innerHTML = svg;
        const el = holder.querySelector('svg');
        el.removeAttribute('width'); el.removeAttribute('height');
        el.style.cssText = `max-width:100%;max-height:100%;width:auto;height:auto;aspect-ratio:${p.width}/${p.height};display:block`;
        el.setAttribute('preserveAspectRatio', 'xMidYMid meet');
        el.style.width = '100%';
        ctx.panel.setStatus(`${p.width} × ${p.height}`);
      } catch (e) {
        ctx.panel.setStatus(String(e), 'error');
      }
    }

    /** Client point → scene pixels. */
    function toImage(clientX, clientY) {
      const el = holder.querySelector('svg');
      if (!el) return { x: 0, y: 0 };
      const r = el.getBoundingClientRect();
      const s = Math.min(r.width / size[0], r.height / size[1]);
      const ox = r.left + (r.width - size[0] * s) / 2, oy = r.top + (r.height - size[1] * s) / 2;
      return { x: (clientX - ox) / s, y: (clientY - oy) / s, scale: s };
    }

    return { update, toImage, holder, size: () => size };
  }

  global.createComposePreview = createComposePreview;
})(window);
