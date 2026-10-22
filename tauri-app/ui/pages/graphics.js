// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Graphics detail page. Live values come from get_gpu_detail and are null when Windows cannot provide them
// (older GPUs / drivers): those cards show an honest "not available" state - nothing is ever estimated.
import { setText } from '../js/dom.js';
import { perfHead, kv, chart, roll } from './perf-common.js';

const PURPLE = '#B8A2FF';
const VNOTE = 'Games and browsers with hardware acceleration use the most video memory.';

export default async function (ctx) {
  const { h, ui, fmt, api, charts } = ctx;
  ctx.css('css/pages/perf.css');
  const st = { adapters: [], pick: null, hist: new Map(), engHist: new Map() };   // hist: id -> util samples; engHist: engine name -> samples

  const head = perfHead(ctx, { title: 'Graphics', sub: ' ' });

  /* ---- utilisation card ---- */
  const util = h('span', { class: 'perf-big' }), temp = h('span', { class: 'perf-mid' }), clock = h('span', { class: 'perf-mid' }), power = h('span', { class: 'perf-mid' });
  const ch = chart({ height: 220, color: PURPLE, labelW: 34 });
  const stat = (t, v) => h('div', { class: 'col', style: { '--g': '4px' } }, h('span', { class: 'lbl' }, t), v);
  const noLive = h('div', { class: 'state-box', style: { display: 'none' } },
    h('strong', {}, 'Live usage is not available for this GPU'),
    h('span', {}, 'Windows does not report utilization, video memory use or temperature for this graphics adapter (typical for older GPUs and drivers). Static details are shown on the right; no values are estimated.'));
  const chartWrap = h('div', {}, ch.el);
  const utilCard = ui.card({ class: 'perf-card perf-main' },
    h('div', { class: 'perf-stats' }, stat('GPU UTILIZATION', util), h('div', { class: 'row', style: { '--g': '28px', alignItems: 'flex-end' } }, stat('TEMPERATURE', temp), stat('CLOCK', clock), stat('POWER', power))),
    chartWrap, noLive);

  /* ---- details ---- */
  const det = kv([['Model', 'name'], ['Vendor', 'vendor'], ['Driver version', 'driver'], ['Driver date', 'date'], ['Dedicated memory', 'vram'], ['DirectX', 'dx']]);
  const sel = h('select', { class: 'select', 'aria-label': 'Graphics adapter', style: { height: '28px', fontSize: '12px', maxWidth: '180px', display: 'none' },
    onchange: (e) => { st.pick = e.target.value; paint(); } });
  const detCard = ui.card({ class: 'perf-card perf-side' }, h('div', { class: 'row between' }, h('h2', {}, 'Details'), sel), det.el);

  /* ---- engines ---- */
  const engGrid = h('div', { style: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: '8px' } });
  const engNone = h('div', { class: 'state-box' }, h('strong', {}, 'Per-engine usage is not available'),
    h('span', {}, 'This GPU or driver does not report usage per engine (3D, Copy, Video decode / encode, Compute).'));
  let engRows = new Map(), engKey = '';
  const engCard = ui.card({ class: 'perf-card', style: { flex: '999 1 560px' } }, h('h2', {}, 'Engines'), engGrid, engNone);
  function paintEngines(list, push) {
    engNone.style.display = list ? 'none' : ''; engGrid.style.display = list ? 'grid' : 'none';
    if (!list) { engKey = ''; return; }
    const key = list.map((e) => e.name).join('|');
    if (key !== engKey) {   // build once per engine set
      engKey = key; engRows = new Map();
      engGrid.replaceChildren(...list.map((e) => {
        const r = { val: h('span', { style: { color: 'var(--muted-2)' } }), spark: charts.sparkline({ w: 120, h: 36, color: PURPLE, max: 100, fill: false, stroke: 1.4 }) };
        r.spark.style.height = '40px';
        engRows.set(e.name, r);
        return h('div', { class: 'eng' }, h('div', { class: 'row between', style: { fontSize: '12px' } }, h('span', {}, e.name), r.val), r.spark);
      }));
    }
    for (const e of list) {
      let buf = st.engHist.get(e.name); if (!buf) st.engHist.set(e.name, buf = []);
      if (push) roll(buf, e.percent);
      const r = engRows.get(e.name); setText(r.val, fmt.pct(e.percent)); r.spark.update(buf);
    }
  }

  /* ---- video memory ---- */
  const vUsed = h('span', { style: { fontSize: '24px', fontWeight: 600, whiteSpace: 'nowrap' } }), vOf = h('span', { class: 'muted' });
  const vFill = h('div', { style: { width: '0%', height: '100%', background: PURPLE, opacity: 0.8 } });
  const vNote = h('p', { class: 'muted', style: { fontSize: '12px', lineHeight: 1.55 } }, VNOTE);
  const vramCard = ui.card({ class: 'perf-card perf-side' }, h('h2', {}, 'Video memory'),
    h('div', { class: 'row', style: { '--g': '8px', alignItems: 'baseline' } }, vUsed, vOf),
    h('div', { style: { height: '10px', borderRadius: '5px', background: 'var(--track)', overflow: 'hidden' } }, vFill), vNote);

  /* ---- paint ---- */
  const na = (el, txt = 'N/A', title = 'Windows does not report this value for this GPU') => { setText(el, txt); el.classList.add('perf-na'); el.title = title; };
  const val = (el, txt) => { setText(el, txt); el.classList.remove('perf-na'); el.title = ''; };

  function choose() { return st.adapters.find((a) => a.id === st.pick) || st.adapters[0]; }
  function paint(live = st.live, tick = false) {
    const a = choose();
    if (!a) { head.setSub('No graphics adapter found'); head.setPill('Unavailable', false); return; }
    head.setSub(`${a.name}${a.driver ? ` · driver ${a.driver}` : ''}`);
    const ok = live && a.util_percent != null;
    head.setPill(ok ? 'Live' : 'Static info', !!ok);
    det.set('name', a.name); det.set('vendor', a.vendor || null); det.set('driver', a.driver || null); det.set('date', a.driver_date || null);
    det.set('vram', a.vram_total ? fmt.bytes(a.vram_total) : null); det.set('dx', a.directx || null);

    chartWrap.style.display = ok ? '' : 'none'; noLive.style.display = ok ? 'none' : '';
    if (ok) {
      val(util, `${Math.round(a.util_percent)}%`);
      const buf = st.hist.get(a.id) || []; st.hist.set(a.id, buf);
      ch.update(buf, 100, ['100%', '50%', '0%']);
    } else na(util, 'N/A');
    if (a.temperature_c != null) val(temp, `${Math.round(a.temperature_c)}°C`); else na(temp);
    if (a.clock_mhz != null) val(clock, `${fmt.num(Math.round(a.clock_mhz))} MHz`); else na(clock);
    if (a.power_w != null) val(power, `${Math.round(a.power_w)} W`); else na(power);

    paintEngines(ok ? a.engines || null : null, tick);

    if (a.vram_used != null && a.vram_total) {
      val(vUsed, fmt.bytes(a.vram_used)); setText(vOf, `of ${fmt.bytes(a.vram_total)} dedicated`);
      vFill.style.width = `${Math.min(100, (a.vram_used / a.vram_total) * 100)}%`; setText(vNote, VNOTE);
    } else {
      na(vUsed, a.vram_total ? fmt.bytes(a.vram_total) : 'N/A', a.vram_total ? '' : undefined); vUsed.classList.toggle('perf-na', !a.vram_total);
      setText(vOf, a.vram_total ? 'dedicated' : 'size unknown');
      vFill.style.width = '0%'; setText(vNote, 'Live video memory use is not available for this GPU.');
    }
  }

  ctx.every(2000, async () => {
    const d = await api.invoke('get_gpu_detail');
    st.adapters = d.adapters || []; st.live = d.live_supported;
    const ids = st.adapters.map((a) => a.id).join('|');
    if (ids !== sel._ids) {
      sel._ids = ids;
      if (!st.adapters.some((a) => a.id === st.pick)) st.pick = (st.adapters.find((a) => a.util_percent != null) || st.adapters[0])?.id;
      sel.replaceChildren(...st.adapters.map((a) => h('option', { value: a.id }, a.name))); sel.value = st.pick;
      sel.style.display = st.adapters.length > 1 ? '' : 'none';
    }
    const a = choose();
    if (a && a.util_percent != null) roll(st.hist.get(a.id) || (st.hist.set(a.id, []), st.hist.get(a.id)), a.util_percent);
    paint(d.live_supported, true);
  });

  return [head, h('div', { class: 'perf' },
    h('div', { class: 'perf-row' }, utilCard, detCard),
    h('div', { class: 'perf-row perf-fill' }, engCard, vramCard))];
}
