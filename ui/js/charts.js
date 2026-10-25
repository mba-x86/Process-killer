// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Dependency-free SVG charts. Every chart returns an element with an update method so pages can refresh
// in place (no re-render, no layout churn): cheap enough to run every second.
import { h, setText } from './dom.js';
import { clamp } from './fmt.js';

/** Full-circle gauge used on dashboard cards. ring({value:62,color:'var(--teal)'}).set(70,'70%') */
export function ring({ value = 0, color = 'var(--blue)', size = 84, stroke = 7, text } = {}) {
  const r = (size - stroke) / 2, c = 2 * Math.PI * r, mid = size / 2;
  const fg = h('circle', { class: 'fg', cx: mid, cy: mid, r, fill: 'none', stroke: color, 'stroke-width': stroke, 'stroke-linecap': 'round',
    'stroke-dasharray': `0 ${c}`, transform: `rotate(-90 ${mid} ${mid})` });
  const label = h('span', { class: 'ring-val', style: { fontSize: `${Math.round(size * 0.226)}px` } });
  const el = h('div', { class: 'ring', style: { width: `${size}px`, height: `${size}px` } },
    h('svg', { width: size, height: size, 'aria-hidden': 'true' }, h('circle', { cx: mid, cy: mid, r, fill: 'none', stroke: 'var(--track)', 'stroke-width': stroke }), fg), label);
  el.set = (v, t, col) => {
    v = clamp(v);
    fg.setAttribute('stroke-dasharray', `${(c * v) / 100} ${c}`);
    if (col) fg.setAttribute('stroke', col);
    setText(label, t ?? `${Math.round(v)}%`);
  };
  el.set(value, text);
  return el;
}

/** Small filled line chart from an array of values. sparkline({w:200,h:36,color,max:100}) */
export function sparkline({ w = 200, h: ht = 36, color = 'var(--blue)', max = 100, values = [], fill = true, stroke = 1.5 } = {}) {
  const area = h('path', { fill: color, 'fill-opacity': 0.12 }), line = h('path', { fill: 'none', stroke: color, 'stroke-width': stroke, 'vector-effect': 'non-scaling-stroke', 'stroke-linejoin': 'round' });
  const svg = h('svg', { viewBox: `0 0 ${w} ${ht}`, preserveAspectRatio: 'none', style: { width: '100%', height: `${ht}px`, display: 'block' }, 'aria-hidden': 'true' }, fill ? area : null, line);
  svg.update = (vals, newMax) => {
    const m = newMax || max, n = vals.length;
    if (n < 2) { line.setAttribute('d', ''); area.setAttribute('d', ''); return; }
    const pts = vals.map((v, i) => `${((i / (n - 1)) * w).toFixed(1)},${(ht - 2 - (clamp(v, 0, m) / m) * (ht - 4)).toFixed(1)}`);
    line.setAttribute('d', `M${pts.join(' L')}`);
    area.setAttribute('d', `M0,${ht} L${pts.join(' L')} L${w},${ht} Z`);
  };
  svg.update(values);
  return svg;
}

/** Rolling buffer helper: const b = history(60); b.push(v); b.values */
export function history(n = 60) {
  const values = [];
  return { values, push(v) { values.push(v); if (values.length > n) values.shift(); return values; }, clear() { values.length = 0; } };
}

/** Large chart with grid + y labels (Processor / Memory / Network pages).
 *  lineChart({height:240,color,yMax:100,grid:[100,50,0],fmt:(v)=>v+'%'}) -> el with .update(values, yMax?) */
export function lineChart({ height = 240, color = 'var(--blue)', yMax = 100, grid = [100, 50, 0], fmt = (v) => `${v}%`, w = 720, series = [] } = {}) {
  const pad = { l: 44, r: 6, t: 8, b: 6 }, ih = height - pad.t - pad.b, iw = w - pad.l - pad.r;
  const gridEls = h('g'), area = h('path', { fill: color, 'fill-opacity': 0.14 }), line = h('path', { fill: 'none', stroke: color, 'stroke-width': 1.6, 'vector-effect': 'non-scaling-stroke', 'stroke-linejoin': 'round' });
  const extra = series.map((s) => h('path', { fill: 'none', stroke: s.color, 'stroke-width': 1.4, 'vector-effect': 'non-scaling-stroke', 'stroke-linejoin': 'round' }));
  const svg = h('svg', { viewBox: `0 0 ${w} ${height}`, preserveAspectRatio: 'none', style: { width: '100%', height: `${height}px`, display: 'block' }, 'aria-hidden': 'true' }, gridEls, area, line, ...extra);
  const labels = h('div', { style: { position: 'absolute', left: 0, top: 0, bottom: 0, width: `${pad.l - 6}px`, pointerEvents: 'none' } });
  const el = h('div', { style: { position: 'relative', height: `${height}px` } }, svg, labels);
  const y = (v, m) => pad.t + ih - (clamp(v, 0, m) / m) * ih;
  const draw = (m) => {
    gridEls.replaceChildren(); labels.replaceChildren();
    for (const g of grid.map((f) => (f <= 1 ? f * m : f))) {
      const yy = y(g, m);
      gridEls.append(h('line', { x1: pad.l, x2: w - pad.r, y1: yy, y2: yy, stroke: '#1E232A', 'stroke-width': 1, 'vector-effect': 'non-scaling-stroke' }));
      labels.append(h('span', { style: { position: 'absolute', right: 0, top: `${(yy / height) * 100}%`, transform: 'translateY(-50%)', fontSize: '11px', color: 'var(--muted)' } }, fmt(g)));
    }
  };
  const path = (vals, m) => vals.map((v, i) => `${(pad.l + (i / Math.max(vals.length - 1, 1)) * iw).toFixed(1)},${y(v, m).toFixed(1)}`);
  draw(yMax);
  el.update = (vals, m = yMax, others = []) => {
    if (m !== el._m) { el._m = m; draw(m); }
    if (vals.length < 2) return;
    const p = path(vals, m);
    line.setAttribute('d', `M${p.join(' L')}`);
    area.setAttribute('d', `M${pad.l},${pad.t + ih} L${p.join(' L')} L${pad.l + iw},${pad.t + ih} Z`);
    extra.forEach((e, i) => others[i] && others[i].length > 1 && e.setAttribute('d', `M${path(others[i], m).join(' L')}`));
  };
  return el;
}

/** 270-degree gauge for the Optimizer / Memory pages. arcGauge({size:200,color}).set(62,'62%','RAM in use') */
export function arcGauge({ size = 200, stroke = 14, color = 'var(--teal)' } = {}) {
  const r = (size - stroke) / 2, c = 2 * Math.PI * r, mid = size / 2, sweep = c * 0.75;
  const common = { cx: mid, cy: mid, r, fill: 'none', 'stroke-width': stroke, 'stroke-linecap': 'round', transform: `rotate(135 ${mid} ${mid})` };
  const fg = h('circle', { ...common, class: 'fg', stroke: color, 'stroke-dasharray': `0 ${c}` });
  const big = h('div', { style: { fontSize: `${Math.round(size * 0.26)}px`, fontWeight: 600, letterSpacing: '-.02em', lineHeight: 1 } });
  const sub = h('div', { class: 'muted', style: { fontSize: '12px' } });
  const el = h('div', { class: 'ring', style: { width: `${size}px`, height: `${size}px` } },
    h('svg', { width: size, height: size, 'aria-hidden': 'true' }, h('circle', { ...common, stroke: 'var(--track)', 'stroke-dasharray': `${sweep} ${c}` }), fg),
    h('div', { style: { position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: '6px' } }, big, sub));
  el.set = (v, text, caption, col) => {
    fg.setAttribute('stroke-dasharray', `${(sweep * clamp(v)) / 100} ${c}`);
    if (col) fg.setAttribute('stroke', col);
    setText(big, text ?? `${Math.round(v)}%`); if (caption != null) setText(sub, caption);
  };
  return el;
}

/** Simple progress bar element: bar(62,'var(--amber)').set(70) */
export function bar(value = 0, color = 'var(--amber)') {
  const i = h('i', { style: { width: `${clamp(value)}%`, background: color } });
  const el = h('div', { class: 'bar' }, i);
  el.set = (v, col) => { i.style.width = `${clamp(v)}%`; if (col) i.style.background = col; };
  return el;
}
