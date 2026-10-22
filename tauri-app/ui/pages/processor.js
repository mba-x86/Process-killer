// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Processor detail page: utilisation chart, specifications, per-logical-CPU sparklines, top processes by CPU.
import { setText } from '../js/dom.js';

export default async function (ctx) {
  ctx.css('css/pages/processor.css');
  const { h, ui, fmt, charts, api } = ctx;
  const detail = await api.invoke('get_cpu_detail').catch(() => null) || { cores: [] };

  /* ---------- header ---------- */
  const pill = ui.pill(ui.dot('ok'), 'Live · every 1.5 s');
  pill.style.height = '28px'; pill.style.alignSelf = 'flex-end'; pill.style.borderRadius = '14px';
  const head = ui.pageHead({ title: 'Processor', sub: [detail.model, detail.socket && `socket ${detail.socket}`].filter(Boolean).join(' · '), back: { to: 'dashboard', label: 'Dashboard' }, right: pill });

  /* ---------- utilisation ---------- */
  const histAll = charts.history(60);
  const chart = charts.lineChart({ height: 220, color: 'var(--blue)', yMax: 100, grid: [100, 75, 50, 25, 0].map((v) => v), fmt: (v) => (v % 50 === 0 ? `${v}%` : '') });
  const bigPct = h('span', { class: 'huge tnum', style: { letterSpacing: '-.03em' } }, '–');
  const speed = h('b', {}, '–'), procs = h('b', {}, detail.process_count != null ? fmt.num(detail.process_count) : '–'), up = h('b', {}, '–');
  const stat = (l, el) => h('div', { class: 'stat' }, h('span', { class: 'lbl' }, l), el);
  const utilCard = ui.card({ style: { flex: '999 1 560px', minWidth: 0, padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: '14px' } },
    h('div', { class: 'row between wrap', style: { alignItems: 'flex-end', gap: '16px' } },
      h('div', { class: 'col', style: { gap: '4px' } }, h('span', { class: 'lbl' }, 'UTILIZATION'), bigPct),
      h('div', { class: 'row', style: { gap: '28px', alignItems: 'flex-start' } }, stat('SPEED NOW', speed), stat('PROCESSES', procs), stat('UPTIME', up))),
    chart,
    h('div', { class: 'row between muted', style: { paddingLeft: '44px', fontSize: '11px' } }, h('span', {}, '60 seconds ago'), h('span', {}, 'now')));

  /* ---------- specifications ---------- */
  const d = detail, kb = (n) => (n == null ? null : fmt.bytes(n, 0));
  const virt = d.virtualization == null ? null : h('span', { class: d.virtualization ? 'ok' : 'warn' }, d.virtualization ? 'Enabled' : 'Disabled');
  const coreTxt = d.cores_p != null ? h('span', {}, `${(d.cores_p || 0) + (d.cores_e || 0)} `, d.cores_e ? h('span', { class: 'muted' }, `(${d.cores_p} P + ${d.cores_e} E)`) : null) : null;
  const specRows = [
    ['Cores', coreTxt], ['Threads', d.threads], ['Base speed', d.base_mhz ? fmt.ghz(d.base_mhz) : null], ['Max turbo', d.max_mhz ? fmt.ghz(d.max_mhz) : null],
    ['L2 cache', kb(d.l2_bytes)], ['L3 cache', kb(d.l3_bytes)], ['Socket', d.socket], ['Architecture', d.architecture], ['Virtualization', virt],
  ].filter(([, v]) => v != null && v !== '');
  const specCard = ui.card({ style: { flex: '1 1 300px', minWidth: 0, padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: '14px' } },
    h('h2', {}, 'Specifications'), h('dl', { class: 'kv' }, specRows.map(([k, v]) => [h('dt', {}, k), h('dd', {}, v)])));

  /* ---------- logical processors ---------- */
  const kinds = d.cores || [];
  const nCores = Math.max(kinds.length, d.threads || 0);
  const coreHist = [], coreSpark = [];
  const grid = h('div', { class: 'cores' });
  for (let i = 0; i < nCores; i++) {
    const e = kinds[i]?.kind === 'E';
    const spark = charts.sparkline({ w: 120, h: 34, color: e ? '#8B95A1' : 'var(--blue)', max: 100, fill: false, stroke: 1.4 });
    coreHist.push(charts.history(40)); coreSpark.push(spark);
    grid.append(h('div', { class: 'core' }, h('div', { class: 'core-top' }, h('span', {}, `CPU ${kinds[i]?.id ?? i}`),
      kinds[i] ? h('span', { class: ['tag', e ? 'tag-e' : 'tag-p'] }, kinds[i].kind) : null), spark));
  }
  const hasKinds = kinds.some((k) => k.kind === 'E') && kinds.some((k) => k.kind === 'P');
  const coresCard = ui.card({ style: { flex: '999 1 560px', minWidth: 0, padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: '14px' } },
    h('div', { class: 'row between wrap', style: { gap: '12px' } }, h('h2', {}, 'Logical processors'),
      hasKinds ? h('span', { class: 'row muted', style: { gap: '10px', fontSize: '12px' } },
        h('span', {}, h('span', { class: 'tag tag-p' }, 'P'), ' Performance core'), h('span', {}, h('span', { class: 'tag tag-e' }, 'E'), ' Efficient core')) : null),
    grid);

  /* ---------- top processes ---------- */
  const tbody = h('tbody');
  const tbl = h('table', { class: 'tbl' }, h('thead', {}, h('tr', {}, h('th', { style: { height: '32px' } }, 'Process'),
    h('th', { class: 'num', style: { height: '32px' } }, 'PID'), h('th', { class: 'num', style: { height: '32px' } }, 'CPU'))), tbody);
  const topCard = ui.card({ style: { flex: '1 1 300px', minWidth: 0, padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: '10px' } },
    h('div', { class: 'row between' }, h('h2', {}, 'Top processes by CPU'), h('a', { href: '#/processes', style: { fontSize: '12px' } }, 'All ›')), tbl);

  /* ---------- polling ---------- */
  const up0 = (await ctx.overview().catch(() => null))?.uptime_secs, t0 = Date.now();
  const paintUp = () => up0 != null && setText(up, fmt.uptime(up0 + (Date.now() - t0) / 1000));
  paintUp();
  ctx.every(30000, paintUp, { immediate: false });
  ctx.every(1500, async () => {
    const l = await api.invoke('get_live'), c = l.cpu;
    setText(bigPct, `${Math.round(c.percent)}%`); setText(speed, fmt.ghz(c.freq_mhz));
    chart.update(histAll.push(c.percent));
    (c.per_core || []).forEach((v, i) => { if (coreSpark[i]) { coreSpark[i].update(coreHist[i].push(v)); } });
    paintUp();
  });
  ctx.every(3000, async () => {
    const list = await api.invoke('get_top_processes', { by: 'cpu', limit: 6 });
    tbody.replaceChildren(...list.map((p) => h('tr', {}, h('td', {}, p.name), h('td', { class: 'num mono', style: { color: 'var(--muted-2)' } }, String(p.pid)), h('td', { class: 'num' }, `${p.cpu.toFixed(1)}%`))));
  });

  return [head, h('div', { class: 'split', style: { flex: 'none' } }, utilCard, specCard), h('div', { class: 'split' }, coresCard, topCard)];
}
