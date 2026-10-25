// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Network detail page: live download/upload chart for the busiest (or chosen) adapter, adapter details, session totals, adapter list.
import { setText } from '../js/dom.js';
import { perfHead, kv, chart, niceMax, splitBytes, roll } from './perf-common.js';

const AUTO = '__auto';

export default async function (ctx) {
  const { h, ui, fmt, api } = ctx;
  ctx.css('css/pages/perf.css');
  const ov = await ctx.overview().catch(() => null);
  const t0 = Date.now(), up0 = ov?.uptime_secs || 0;
  const st = { adapters: [], pick: AUTO, shown: null, hist: new Map() };   // hist: id -> {down:[], up:[], peakD, peakU}

  const head = perfHead(ctx, { title: 'Network', sub: ' ' });

  /* ---- chart card ---- */
  const dlNum = h('span'), dlUnit = h('small'), ulNum = h('span'), ulUnit = h('small');
  const ch = chart({ height: 240, color: 'var(--accent-2)', color2: '#A7B0BB', labelW: 52 });
  const legend = (sw, t, num, unit) => h('div', { class: 'col', style: { '--g': '4px' } },
    h('span', { class: 'lbl row', style: { '--g': '6px' } }, sw, t), h('span', { class: 'perf-big' }, num, unit));
  const chartCard = ui.card({ class: 'perf-card perf-main' },
    h('div', { class: 'row wrap', style: { '--g': '36px', alignItems: 'flex-end' } },
      legend(h('span', { style: { width: '10px', height: '3px', borderRadius: '2px', background: 'var(--accent-2)' } }), 'DOWNLOAD', dlNum, dlUnit),
      legend(h('span', { style: { width: '10px', height: 0, borderTop: '2px dashed #A7B0BB' } }), 'UPLOAD', ulNum, ulUnit)),
    ch.el);

  /* ---- adapter card ---- */
  const det = kv([['Name', 'name'], ['Connection', 'conn'], ['Status', 'status'], ['Link speed', 'link'], ['IPv4 address', 'ip'], ['Gateway', 'gw'], ['DNS', 'dns'], ['MAC address', 'mac']]);
  const sel = h('select', { class: 'select', 'aria-label': 'Adapter', style: { height: '28px', fontSize: '12px', maxWidth: '180px' }, onchange: (e) => { st.pick = e.target.value; paint(); } });
  const selWrap = h('div', { style: { display: 'none' } }, sel);
  const adCard = ui.card({ class: 'perf-card perf-side' }, h('div', { class: 'row between' }, h('h2', {}, 'Adapter'), selWrap), det.el);

  /* ---- session card ---- */
  const subSession = h('span', { style: { fontWeight: 400, color: 'var(--muted)' } });
  const tile = (label, hint) => { const v = h('span', { class: 'perf-mid' }, '—'); return { v, el: h('div', { class: 'stat' }, h('span', { class: 'lbl' }, label), v, hint ? h('span', { class: 'hint' }, hint) : null) }; };
  const tRx = tile('RECEIVED'), tTx = tile('SENT'), tPd = tile('PEAK DOWNLOAD', 'since you opened this page'), tPu = tile('PEAK UPLOAD', 'since you opened this page');
  const sessCard = ui.card({ class: 'perf-card' }, h('h2', {}, 'This session ', subSession),
    h('div', { class: 'row wrap', style: { '--g': '10px', alignItems: 'stretch' } }, tRx.el, tTx.el, tPd.el, tPu.el));
  const paintSession = () => setText(subSession, `· since start-up ${fmt.uptime(up0 + (Date.now() - t0) / 1000)} ago`);
  paintSession(); ctx.every(30000, paintSession, { immediate: false });

  /* ---- adapters list ---- */
  const tbody = h('tbody');
  let rowEls = new Map();   // id -> {tr, dot, name, kind, ip, dn, upl}
  const listCard = ui.card({ class: 'perf-card', style: { display: 'none', gap: '6px' } }, h('h2', {}, 'All adapters'),
    h('table', { class: 'tbl' }, h('thead', {}, h('tr', {}, h('th', {}, 'Adapter'), h('th', {}, 'Type'), h('th', {}, 'IPv4'), h('th', { class: 'num' }, '↓ Download'), h('th', { class: 'num' }, '↑ Upload'))), tbody));
  function syncList() {
    const ids = st.adapters.map((a) => a.id).join('|');
    if (ids === listCard._ids) return;
    listCard._ids = ids; rowEls = new Map();
    tbody.replaceChildren(...st.adapters.map((a) => {
      const r = { dot: ui.dot('off'), name: h('span'), kind: h('td', { class: 'dim' }), ip: h('td', { class: 'mono' }), dn: h('td', { class: 'num' }), upl: h('td', { class: 'num dim' }) };
      r.tr = h('tr', { class: 'pk-click', onclick: () => { st.pick = a.id; sel.value = a.id; paint(); } },
        h('td', {}, h('span', { class: 'row', style: { '--g': '8px' } }, r.dot, r.name)), r.kind, r.ip, r.dn, r.upl);
      rowEls.set(a.id, r); return r.tr;
    }));
    sel.replaceChildren(h('option', { value: AUTO }, 'Busiest adapter'), ...st.adapters.map((a) => h('option', { value: a.id }, a.name)));
    if (st.pick !== AUTO && !st.adapters.some((a) => a.id === st.pick)) st.pick = AUTO;
    sel.value = st.pick;
    const many = st.adapters.length > 1;
    selWrap.style.display = many ? '' : 'none'; listCard.style.display = st.adapters.length ? '' : 'none';
  }

  /* ---- paint ---- */
  const avg = (a, n = 10) => { const s = a.slice(-n); return s.length ? s.reduce((x, y) => x + y, 0) / s.length : 0; };
  function choose() {
    if (st.pick !== AUTO) return st.adapters.find((a) => a.id === st.pick) || st.adapters[0];
    const live = st.adapters.filter((a) => a.up);
    const score = (a) => { const hh = st.hist.get(a.id); return hh ? avg(hh.down) + avg(hh.up) : 0; };
    return (live.length ? live : st.adapters).reduce((best, a) => (!best || score(a) > score(best) ? a : best), null);
  }
  function paint() {
    const a = choose();
    if (!a) { head.setSub('No network adapter found'); head.setPill('Offline', false); return; }
    st.shown = a;
    const hh = st.hist.get(a.id);
    const [dv, du] = splitBytes(fmt, a.down_bps), [uv, uu] = splitBytes(fmt, a.up_bps);
    setText(dlNum, dv); setText(dlUnit, `${du}/s`); setText(ulNum, uv); setText(ulUnit, `${uu}/s`);
    const max = niceMax(Math.max(...hh.down, ...hh.up, 1) / 1024, 10) * 1024;
    ch.update(hh.down, max, [fmt.rate(max), fmt.rate(max / 2), '0'], hh.up);
    head.setSub(`${a.name} · ${a.kind}${a.ssid ? ` (${a.ssid})` : ''}`);
    head.setPill(a.up ? 'Live' : 'Disconnected', a.up);
    det.set('name', a.name); det.set('conn', a.kind + (a.ssid ? ` · ${a.ssid}` : ''));
    det.set('status', a.up ? 'Connected' : 'Disconnected'); det.set('link', a.link_mbps ? `${fmt.num(a.link_mbps)} Mbps` : null);
    det.set('ip', a.ip || null); det.set('gw', a.gateway || null); det.set('dns', a.dns?.length ? a.dns.join(', ') : null); det.set('mac', a.mac || null);
    setText(tRx.v, fmt.bytes(a.total_down)); setText(tTx.v, fmt.bytes(a.total_up));
    setText(tPd.v, fmt.rate(hh.peakD)); setText(tPu.v, fmt.rate(hh.peakU));
    for (const x of st.adapters) {
      const r = rowEls.get(x.id); if (!r) continue;
      r.dot.className = x.up ? 'dot' : 'dot off'; setText(r.name, x.name); setText(r.kind, x.kind);
      setText(r.ip, x.ip || '—'); setText(r.dn, x.up ? fmt.rate(x.down_bps) : '—'); setText(r.upl, x.up ? fmt.rate(x.up_bps) : '—');
      r.tr.style.background = x.id === a.id ? '#1A1F27' : '';
    }
  }

  ctx.every(1000, async () => {
    const d = await api.invoke('get_net_detail');
    st.adapters = d.adapters || [];
    for (const a of st.adapters) {
      let hh = st.hist.get(a.id);
      if (!hh) st.hist.set(a.id, hh = { down: [], up: [], peakD: 0, peakU: 0 });
      roll(hh.down, a.down_bps); roll(hh.up, a.up_bps);
      hh.peakD = Math.max(hh.peakD, a.down_bps); hh.peakU = Math.max(hh.peakU, a.up_bps);
    }
    syncList(); paint();
  });

  return [head, h('div', { class: 'perf' }, h('div', { class: 'perf-row' }, chartCard, adCard), sessCard, listCard)];
}
