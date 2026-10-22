// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Tiny DOM helper (no framework): h('div', {class:'card', onclick}, child, ...) -> HTMLElement / SVGElement.
const SVG = 'http://www.w3.org/2000/svg';
const SVG_TAGS = new Set(['svg', 'path', 'circle', 'rect', 'g', 'line', 'polyline', 'polygon', 'defs', 'linearGradient', 'stop', 'text', 'clipPath']);

function append(el, kid) {
  if (kid == null || kid === false || kid === true) return;
  if (Array.isArray(kid)) { kid.forEach((k) => append(el, k)); return; }
  el.append(kid instanceof Node ? kid : document.createTextNode(String(kid)));
}

export function h(tag, props, ...kids) {
  const svg = SVG_TAGS.has(tag);
  const el = svg ? document.createElementNS(SVG, tag) : document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v == null || v === false) continue;
    if (k === 'class') el.setAttribute('class', Array.isArray(v) ? v.filter(Boolean).join(' ') : v);
    else if (k === 'style' && typeof v === 'object') {
      for (const [p, val] of Object.entries(v)) {
        if (val == null) continue;
        if (p.startsWith('--')) el.style.setProperty(p, val);   // custom properties (--g, --min) need setProperty
        else el.style[p] = val;
      }
    }
    else if (k === 'ref') v(el);
    else if (k === 'dataset') Object.assign(el.dataset, v);
    else if (k.startsWith('on') && typeof v === 'function') el.addEventListener(k.slice(2).toLowerCase(), v);
    else if (!svg && (k === 'value' || k === 'checked' || k === 'disabled' || k === 'selected' || k === 'indeterminate')) el[k] = v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  append(el, kids);
  return el;
}

/** Replace all children. */
export function setChildren(el, ...kids) { el.replaceChildren(); append(el, kids); return el; }

/** Debounce for search boxes etc. */
export function debounce(fn, ms = 150) { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); }; }

/** Only touch the DOM when the text really changed (cheap steady-state updates). */
export function setText(el, text) { text = String(text); if (el.textContent !== text) el.textContent = text; }
