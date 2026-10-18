/* Metamorphosis — shared editor helpers.

   Editors register themselves in window.MorphEditors[editorId] = (ctx) => { ... }.
   ctx = { panel, spec, paths, info, params, setParam(k, v), run(params?), src(path), h }.
   Ops without a custom editor get the generic form built from their parameter schema. */
(function (global) {
  'use strict';
  const h = global.hudEl;
  global.MorphEditors = global.MorphEditors || {};

  /** A form control for one ParamSpec. Returns the field element. */
  function paramField(p, value, onChange) {
    const field = h('label', { class: 'hud-field' }, h('span', { text: p.label }));
    let input;
    switch (p.type) {
      case 'bool':
        input = h('input', { type: 'checkbox' });
        input.checked = !!value;
        input.addEventListener('change', () => onChange(input.checked));
        field.classList.add('hud-row');
        break;
      case 'int':
      case 'float': {
        const step = p.type === 'int' ? 1 : (p.step || 0.1);
        const wrap = h('div', { class: 'hud-row' });
        const range = h('input', { type: 'range', min: p.min, max: p.max, step });
        const num = h('input', { type: 'number', min: p.min, max: p.max, step, style: 'width:90px' });
        range.value = num.value = value;
        const set = (v) => { range.value = num.value = v; onChange(p.type === 'int' ? parseInt(v, 10) : parseFloat(v)); };
        range.addEventListener('input', () => set(range.value));
        num.addEventListener('change', () => set(num.value));
        wrap.append(range, num);
        input = wrap;
        break;
      }
      case 'choice':
        input = h('select');
        p.options.forEach((o) => input.append(h('option', { value: o, text: o })));
        input.value = value;
        input.addEventListener('change', () => onChange(input.value));
        break;
      case 'color':
        input = h('input', { type: 'color' });
        input.value = (value || '#ffffff').slice(0, 7);
        input.addEventListener('input', () => onChange(input.value));
        break;
      case 'json':
        return null; // managed by the custom editor
      default:
        input = h('input', { type: 'text' });
        input.value = value == null ? '' : value;
        input.addEventListener('change', () => onChange(input.value));
    }
    field.append(input);
    if (p.help) field.append(h('span', { style: 'text-transform:none;letter-spacing:0;opacity:.8', text: p.help }));
    return field;
  }

  /** Default values from a spec. */
  function defaults(spec) {
    const o = {};
    spec.params.forEach((p) => { o[p.key] = p.default; });
    return o;
  }

  global.MorphEditorKit = { paramField, defaults };
})(window);
