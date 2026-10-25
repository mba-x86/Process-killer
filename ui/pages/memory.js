// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Memory detail page: live in-use chart, hardware/commit details, composition, top apps. Built once, updated in place.
import { setText } from '../js/dom.js';
import { perfHead, kv, chart, roll, bytes1 } from './perf-common.js';

export default async function (ctx) {
  const { h, ui, fmt, api } = ctx;
  ctx.css('css/pages/perf.css');
  const hist = [];
  let total = 0;

  const boost = ui.button({ label: 'Boost RAM', icon: 'bolt', kind: 'primary', onClick: async () => {
    boost.disabled = true;
    try { const r = await api.invoke('boost_ram'); ctx.toast(`RAM boosted - ${fmt.bytes(r.freed)} freed`, 'ok'); }
    catch (e) { ctx.toast(e.message || String(e), 'bad'); } finally { boost.disabled = false; }
  } });
  const head = perfHead(ctx, { title: 'Memory', sub: ' ', extra: boost });

  /* ---- chart card ---- */
  const inUse = h('span'), inUsePct = h('span', { style: { fontSize: '20px', color: 'var(--muted)', letterSpacing: 0 } });
  const avail = h('span', { class: 'perf-mid' }), tot = h('span', { class: 'perf-mid' });
  const ch = chart({ height: 220, color: 'var(--teal)', labelW: 44 });
  const lbl = (t, v) => h('div', { class: 'col', style: { '--g': '4px' } }, h('span', { class: 'lbl' }, t), v);
  const chartCard = ui.card({ class: 'perf-card perf-main' },
    h('div', { class: 'perf-stats' },
      lbl('IN USE', h('span', { class: 'perf-big' }, inUse, ' ', inUsePct)),
      h('div', { class: 'row', style: { '--g': '28px', alignItems: 'flex-end' } }, lbl('AVAILABLE', avail), lbl('TOTAL', tot))),
    ch.el);

  /* ---- details ---- */
  const det = kv([['Total installed', 'total'], ['Type', 'type'], ['Speed', 'speed'], ['Slots used', 'slots'], ['Committed', 'commit'],
    ['Cached', 'cached'], ['Paged pool', 'paged'], ['Non-paged pool', 'nonpaged'], ['Page file', 'pagefile'], ['Compression', 'comp']]);
  const detCard = ui.card({ class: 'perf-card perf-side' }, h('h2', {}, 'Details'), det.el);

  /* ---- composition ---- */
  const barFill = h('div', { style: { width: '0%', background: 'var(--teal)' } });
  const useTxt = h('span', { class: 'tnum' }), avTxt = h('span', { class: 'tnum' });
  const sw = (c, t, v) => h('div', { class: 'legend' }, h('span', { class: 'sw' }, h('i', { style: { background: c } }), t), v);
  const compCard = ui.card({ class: 'perf-card', style: { flex: '1 1 380px' } }, h('h2', {}, 'Composition'),
    h('div', { style: { display: 'flex', height: '14px', borderRadius: '7px', overflow: 'hidden', background: 'var(--track)' }, role: 'img', 'aria-label': 'Memory in use versus available' }, barFill),
    h('div', { class: 'col', style: { '--g': '10px' } }, sw('var(--teal)', 'In use', useTxt), sw('#3A424D', 'Available', avTxt)),
    h('p', { class: 'muted', style: { fontSize: '12px', lineHeight: 1.55 } }, 'Windows keeps spare memory busy as cache and hands it back instantly when an app needs it.'));

  /* ---- top apps (rows rebuilt only when the list length changes) ---- */
  const tbody = h('tbody');
  let rows = [];
  function paintApps(list) {
    if (rows.length !== list.length) {
      rows = list.map(() => ({ name: h('span'), cnt: h('span', { class: 'muted' }), fill: h('b'), val: h('span') }));
      tbody.replaceChildren(...rows.map((r) => h('tr', {}, h('td', {}, r.name, r.cnt), h('td', {}, h('div', { class: 'pbar' }, h('i', {}, r.fill), r.val)))));
    }
    const max = Math.max(...list.map((a) => a.mem), 1);
    list.forEach((a, i) => {
      const r = rows[i];
      setText(r.name, a.name); setText(r.cnt, a.count > 1 ? ` × ${a.count}` : '');
      r.fill.style.width = `${(a.mem / max) * 100}%`; setText(r.val, fmt.bytes(a.mem));
    });
  }
  const appsCard = ui.card({ class: 'perf-card', style: { flex: '999 1 520px', gap: '10px' } },
    h('div', { class: 'row between' }, h('h2', {}, 'Top apps by memory'), h('a', { href: '#/processes', style: { fontSize: '12px' } }, 'All processes ›')),
    h('table', { class: 'tbl' }, h('thead', {}, h('tr', {}, h('th', {}, 'App'), h('th', { style: { width: '55%' } }, 'Memory ↓'))), tbody));

  /* ---- data ---- */
  const hw = { type: null, speed: null, slots: null };
  const ov = await ctx.overview().catch(() => null);
  const paintSub = (d) => head.setSub([total ? fmt.bytes(total) : null, (d || hw).type, ov?.model].filter(Boolean).join(' · ') || ' ');
  const paintHw = () => {
    det.set('type', hw.type);
    det.set('speed', hw.speed ? `${hw.speed} MT/s` : null);
    det.set('slots', hw.slots);
  };
  api.invoke('get_mem_hw').then((d) => {
    hw.type = d.type || null; hw.speed = d.speed_mhz || null;
    hw.slots = d.slots_used != null ? (d.slots_total ? `${d.slots_used} of ${d.slots_total}` : `${d.slots_used}`) : null;
    const mods = (d.modules || []).map((m) => `${m.slot}: ${fmt.bytes(m.size, 0)} ${m.manufacturer || ''} ${m.part_number || ''}`.trim()).join('\n');
    paintHw();
    if (mods) det.set('slots', hw.slots, mods);
    paintSub(d);
  }).catch(() => {});

  ctx.every(1000, async () => {
    const l = await api.invoke('get_live'); const m = l.mem;
    total = m.total; roll(hist, m.used);
    setText(inUse, bytes1(fmt, m.used)); setText(inUsePct, `${Math.round(m.percent)}%`);
    setText(avail, bytes1(fmt, m.available)); setText(tot, bytes1(fmt, m.total));
    ch.update(hist, m.total, [bytes1(fmt, m.total), bytes1(fmt, m.total / 2), '0']);
    barFill.style.width = `${m.percent}%`;
    setText(useTxt, `${bytes1(fmt, m.used)} · ${Math.round(m.percent)}%`); setText(avTxt, `${bytes1(fmt, m.available)} · ${Math.round(100 - m.percent)}%`);
    det.set('total', bytes1(fmt, m.total));
    if (!head._sub) { head._sub = 1; paintSub(); }
  });
  ctx.every(1500, async () => {
    const d = await api.invoke('get_mem_detail');
    det.set('commit', d.commit_limit ? `${fmt.bytes(d.committed)} of ${fmt.bytes(d.commit_limit)}` : fmt.bytes(d.committed));
    det.set('cached', fmt.bytes(d.cached)); det.set('paged', fmt.bytes(d.paged_pool)); det.set('nonpaged', fmt.bytes(d.nonpaged_pool));
    det.set('pagefile', d.page_file_total ? `${fmt.bytes(d.page_file_used)} of ${fmt.bytes(d.page_file_total)}` : 'None');
    det.set('comp', d.compressed != null ? `${fmt.bytes(d.compressed)} (MemCompression)` : null, d.compressed == null ? 'Memory compression is not active or not readable' : '');
  });
  ctx.every(3000, async () => paintApps(await api.invoke('get_top_apps', { by: 'memory', limit: 6 })));

  return [head, h('div', { class: 'perf' },
    h('div', { class: 'perf-row' }, chartCard, detCard),
    h('div', { class: 'perf-row perf-fill' }, compCard, appsCard))];
}
