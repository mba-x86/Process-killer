// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { debounce } from '../js/dom.js';
import { seltable } from './_seltable.js';

const LIMIT = 300;
const ci = (k) => (a, b) => String(a[k]).localeCompare(String(b[k]), undefined, { sensitivity: 'base' });

export default async function (ctx) {
  const { h, ui, fmt } = ctx;
  ctx.css('css/pages/seltable.css');
  let all = [], q = '', maxMem = 1, auto = true, busy = false;

  const head = ui.pageHead({ title: 'Processes', sub: 'Loading…' });
  const lock = () => h('svg', { class: 'lock', width: 12, height: 12, viewBox: '0 0 16 16', fill: 'none', stroke: 'currentColor', 'stroke-width': 1.6, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' },
    h('rect', { x: 3, y: 7, width: 10, height: 7, rx: 1.5 }), h('path', { d: 'M5.5 7V5a2.5 2.5 0 0 1 5 0v2' }));
  const table = seltable({
    rowKey: 'pid', sortKey: 'mem', sortDir: -1, limit: LIMIT, emptyText: 'No processes match',
    rowClass: (r) => (r.protected ? 'prot' : ''), rowTitle: (r) => (r.protected ? 'Protected system process' : ''),
    onSelect: () => paintButtons(),
    columns: [
      { key: 'name', label: 'Name', sort: ci('name'), width: '22%', render: (r) => (r.protected ? [lock(), r.name] : r.name), sig: (r) => `${r.protected}|${r.name}`, text: (r) => r.name },
      { key: 'pid', label: 'PID', num: true, mono: true, width: '7%' },
      { key: 'cpu', label: 'CPU %', num: true, width: '7%', text: (r) => (r.cpu || 0).toFixed(1) },
      { key: 'mem', label: 'Memory', width: '20%', text: (r) => fmt.bytes(r.mem),
        sig: (r) => `${fmt.bytes(r.mem)}|${Math.round((r.mem / maxMem) * 100)}`,
        render: (r) => h('div', { class: 'mbar' }, h('i', {}, h('b', { style: { width: `${Math.max(2, Math.round((r.mem / maxMem) * 100))}%` } })), h('span', {}, fmt.bytes(r.mem))) },
      { key: 'exe', label: 'Path', mono: true, cls: 'path', sort: ci('exe'), text: (r) => r.exe || '—', title: (r) => r.exe || '' },
    ],
  });
  const search = ui.searchBox({ placeholder: 'Search processes…', onInput: debounce((v) => { q = v.trim().toLowerCase(); apply(); }, 150) });
  const chk = ui.checkbox({ checked: true, label: 'Auto-refresh', onChange: (v) => { auto = v; if (v) refresh(); } });

  const bEnd = ui.button({ label: 'End task', icon: 'x', kind: 'danger', onClick: () => end(false) });
  const bTree = ui.button({ label: 'End process tree', onClick: () => end(true) });
  const bOpen = ui.button({ label: 'Open file location', icon: 'folder', onClick: locate });
  function paintButtons() {
    const rows = table.selectedRows();
    bEnd.disabled = bTree.disabled = busy || !rows.length;
    bOpen.disabled = !rows.length || !rows[0].exe;
  }

  function apply() {
    const rows = q ? all.filter((p) => p.name.toLowerCase().includes(q) || String(p.pid) === q || (p.exe || '').toLowerCase().includes(q)) : all;
    maxMem = rows.reduce((m, p) => Math.max(m, p.mem || 0), 1);
    table.setRows(rows);
    head.setSub(`${rows.length < all.length ? `${rows.length} of ${all.length}` : all.length} processes${rows.length > table.shown ? ` · showing top ${table.shown} of ${rows.length}` : ''}`);
    paintButtons();
  }
  async function refresh() {
    if (busy) return;
    try { all = (await ctx.api.invoke('list_processes')).processes; apply(); }
    catch (e) { head.setSub('Could not read processes'); }
  }

  async function end(tree) {
    const rows = table.selectedRows();
    if (!rows.length) { ctx.toast('Select a process first'); return; }
    const blocked = rows.filter((r) => r.protected), todo = rows.filter((r) => !r.protected);
    if (blocked.length) ctx.toast(`${blocked.length} protected system process(es) skipped`, 'bad');
    if (!todo.length) return;
    const names = todo.slice(0, 3).map((r) => r.name).join(', ') + (todo.length > 3 ? '…' : '');
    const ok = await ui.confirmDialog({ title: tree ? 'End process tree' : 'End process', danger: true, confirmLabel: tree ? 'End tree' : 'End task',
      text: `End ${names}${tree ? ' and everything they started' : ''}? Unsaved data in them will be lost.` });
    if (!ok) return;
    busy = true; paintButtons();
    try {
      const { results } = await ctx.api.invoke('kill_process', { pids: todo.map((r) => r.pid), tree });
      const bad = results.filter((r) => !r.ok);
      bad.slice(0, 3).forEach((r) => ctx.toast(`PID ${r.pid}: ${r.message}`, 'bad'));
      ctx.toast(`Ended ${results.length - bad.length} of ${results.length}`, bad.length ? '' : 'ok');
    } catch (e) { ctx.toast(e.message, 'bad'); }
    busy = false;
    await refresh();
  }
  async function locate() {
    const r = table.selectedRows()[0];
    if (!r?.exe) return;
    try { await ctx.api.invoke('open_location', { path: r.exe }); } catch (e) { ctx.toast(e.message, 'bad'); }
  }

  paintButtons();
  ctx.every(3000, async () => { if (auto) await refresh(); }, { immediate: false });
  await refresh();
  return [head, h('div', { class: 'toolbar', style: { gap: '16px' } }, search, chk), h('div', { class: 'card tcard fit' }, table.el),
    h('div', { class: 'actions' }, bEnd, bTree, bOpen)];
}
