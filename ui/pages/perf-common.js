// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Small helpers shared by the Memory / Network / Graphics detail pages (built once, updated in place).
import { h, setText } from '../js/dom.js';

/** Page header: back link, title, subtitle, 'Live' pill (+ optional extra node). Returns the pageHead element with .setPill(text, live). */
export function perfHead(ctx, { title, sub, extra }) {
  const dot = h('span', { class: 'dot live-dot' }), txt = h('span', {}, 'Live');
  const pill = ctx.ui.pill(dot, txt);
  const head = ctx.ui.pageHead({ title, sub, back: ctx.embedded ? null : { to: 'dashboard', label: 'Dashboard' },
    right: h('div', { class: 'row', style: { '--g': '10px', alignSelf: 'flex-end' } }, pill, extra || null) });
  head.setPill = (text, live = true) => { setText(txt, text); dot.className = live ? 'dot live-dot' : 'dot off'; };
  return head;
}

/** Key/value list. rows: [[label, id]] -> {el, set(id, text|null, title?)} (null = "not available" tone). */
export function kv(rows) {
  const map = {};
  const el = h('dl', { class: 'kv' }, rows.map(([label, id]) => {
    const dd = h('dd', { class: 'na' }, '—'); map[id] = dd;
    return [h('dt', {}, label), dd];
  }));
  return { el, set(id, text, title) { const dd = map[id]; if (!dd) return; const na = text == null || text === ''; dd.classList.toggle('na', na); setText(dd, na ? '—' : text); dd.title = title || ''; } };
}

/** Time-series chart in the design's layout: y labels | svg (5 grid lines, area, line, optional dashed 2nd line). */
export function chart({ height = 220, color = 'var(--teal)', color2 = '#A7B0BB', labelW = 44, xPad, w = 600 }) {
  const NS = 'http://www.w3.org/2000/svg';
  const mk = (a) => { const p = document.createElementNS(NS, 'path'); for (const k in a) p.setAttribute(k, a[k]); return p; };
  const grid = mk({ d: 'M0,0.5 L600,0.5 M0,50 L600,50 M0,100 L600,100 M0,150 L600,150 M0,199.5 L600,199.5', stroke: '#1F242B', 'stroke-width': 1, 'vector-effect': 'non-scaling-stroke', fill: 'none' });
  const area = mk({ fill: color, 'fill-opacity': 0.14 });
  const line = mk({ fill: 'none', stroke: color, 'stroke-width': 1.8, 'vector-effect': 'non-scaling-stroke', 'stroke-linejoin': 'round' });
  const line2 = mk({ fill: 'none', stroke: color2, 'stroke-width': 1.4, 'stroke-dasharray': '5 4', 'vector-effect': 'non-scaling-stroke', 'stroke-linejoin': 'round' });
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('viewBox', `0 0 ${w} 200`); svg.setAttribute('preserveAspectRatio', 'none'); svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('class', 'chart-svg'); svg.style.height = `${height}px`;
  svg.append(grid, area, line, line2);
  const ly = [h('span'), h('span'), h('span')];
  const yEl = h('div', { class: 'chart-y', style: { height: `${height}px`, width: `${labelW}px` } }, ly);
  const xEl = h('div', { class: 'chart-x', style: { paddingLeft: `${xPad ?? labelW + 10}px` } }, h('span', {}, '60 seconds ago'), h('span', {}, 'now'));
  const el = h('div', { class: 'col', style: { '--g': '8px' } }, h('div', { class: 'chart-box' }, yEl, svg), xEl);
  const pts = (v, max, n = 60) => v.map((x, i) => `${(((n - v.length + i) / (n - 1)) * w).toFixed(1)},${(199 - Math.min(Math.max(x, 0), max) / max * 198).toFixed(1)}`);
  return {
    el,
    /** vals: samples (newest last); max: y-axis max; labels: [top, mid, bottom] strings */
    update(vals, max, labels, vals2) {
      if (labels) labels.forEach((t, i) => setText(ly[i], t));
      if (vals.length < 2) return;
      const p = pts(vals, max);
      line.setAttribute('d', `M${p.join(' L')}`);
      area.setAttribute('d', `M${p[0].split(',')[0]},200 L${p.join(' L')} L${w},200 Z`);
      if (vals2 && vals2.length > 1) line2.setAttribute('d', `M${pts(vals2, max).join(' L')}`);
    },
  };
}

/** Round a positive number up to 1/2/5 x 10^n ("nice" axis maximum). */
export function niceMax(v, min = 1) {
  v = Math.max(v, min);
  const p = 10 ** Math.floor(Math.log10(v)), f = v / p;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * p;
}

/** Split fmt.bytes() output into [value, unit]. */
export const splitBytes = (fmt, n) => { const [v, u] = fmt.bytes(n).split(' '); return [v, u || 'B']; };

/** Push to a rolling buffer, returns the buffer. */
export function roll(buf, v, n = 60) { buf.push(v); if (buf.length > n) buf.shift(); return buf; }

/** Like fmt.bytes but always one decimal for MB and above ("6.0 GB"), as in the design. */
export function bytes1(fmt, n) { return n >= 1048576 ? `${fmt.bytes(n, 1)}`.replace(/^(\d+) /, '$1.0 ') : fmt.bytes(n); }
