// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { debounce } from '../js/dom.js';
import { seltable } from './_seltable.js';

const cap = (s) => (s ? s[0].toUpperCase() + s.slice(1) : '');
const START = { automatic: 'Automatic', manual: 'Manual', disabled: 'Disabled', delayed: 'Automatic (delayed)' };
const ci = (k) => (a, b) => String(a[k]).localeCompare(String(b[k]), undefined, { sensitivity: 'base' });
const VERB = { stop: ['Stop', 'Stopped'], restart: ['Restart', 'Restarted'], disable: ['Disable', 'Disabled'],
  start: ['Start', 'Started'], enable: ['Enable', 'Enabled'], manual: ['Manual', 'Set to manual'] };

export default async function (ctx) {
  const { h, ui, icon } = ctx;
  ctx.css('css/pages/seltable.css');
  let all = [], q = '', filter = 'all', busy = false;
  const fx = new Map();   // service name -> 'working' | 'ok' | 'bad' | 'pending' | 'verified' (UI-only feedback while / after an action)
  const fxTimers = new Map();
  const setFx = (name, v, ms) => {
    clearTimeout(fxTimers.get(name));
    if (v) fx.set(name, v); else fx.delete(name);
    if (ms) fxTimers.set(name, setTimeout(() => { fx.delete(name); apply(); }, ms));
  };
  let alive = true;
  ctx.onCleanup(() => { alive = false; fxTimers.forEach(clearTimeout); });

  const head = ui.pageHead({ title: 'Services', sub: 'Loading…' });
  const table = seltable({
    rowKey: 'name', sortKey: 'display', limit: 300, emptyText: 'No services match',
    rowTitle: (r) => `${r.display} (${r.name})`,
    rowClass: (r) => { const v = fx.get(r.name); return v === 'ok' || v === 'verified' ? 'fx-ok' : v === 'bad' ? 'fx-bad' : ''; },
    onSelect: () => paintButtons(),
    columns: [
      { key: 'display', label: 'Display name', sort: ci('display'), width: '28%' },
      { key: 'name', label: 'Service name', sort: ci('name'), width: '16%' },
      { key: 'status', label: 'Status', width: '14%', text: (r) => cap(r.status), sig: (r) => `${r.status}|${fx.get(r.name) || ''}`,
        render: (r) => {
          const v = fx.get(r.name);
          if (v === 'working' || v === 'pending') return h('span', { class: 'st-work' }, v === 'pending' ? 'Starting…' : 'Working…');
          const pill = h('span', { class: [r.status === 'running' ? 'st-run' : r.status === 'stopped' ? 'st-stop' : 'st-pend', v && 'st-pop'] }, cap(r.status));
          return v === 'verified' ? [pill, h('span', { class: 'st-chk', title: 'Windows confirmed the change' }, icon('check', 11, { 'stroke-width': 2.2 }), 'verified')] : pill;
        } },
      { key: 'start', label: 'Startup', width: '12%', text: (r) => START[r.start] || cap(r.start), sig: (r) => `${r.start}|${fx.get(r.name) || ''}` },
      { key: 'path', label: 'Path', mono: true, cls: 'path', text: (r) => r.path || '—', sort: ci('path') },
    ],
  });
  const search = ui.searchBox({ placeholder: 'Search services…', onInput: debounce((v) => { q = v.trim().toLowerCase(); apply(); }, 150) });
  const sel = ui.select({ options: [['all', 'All'], ['running', 'Running'], ['stopped', 'Stopped']], value: 'all', onChange: (v) => { filter = v; apply(); } });
  const refreshBtn = ui.button({ label: 'Refresh', icon: 'refresh', onClick: () => load(true) });

  const mk = (label, action, extra = {}) => ui.button({ label, onClick: () => run(action), ...extra });
  const bStart = ui.button({ label: 'Start', icon: 'play', onClick: () => run('start') });
  const bStop = ui.button({ label: 'Stop', icon: 'stop', onClick: () => run('stop') });
  const bRestart = ui.button({ label: 'Restart', icon: 'refresh', onClick: () => run('restart') });
  const bEnable = mk('Enable (automatic)', 'enable'), bManual = mk('Manual', 'manual');
  const bDisable = ui.button({ label: 'Disable', kind: 'danger', onClick: () => run('disable') });
  const all6 = [bStart, bStop, bRestart, bEnable, bManual, bDisable];

  function paintButtons() {
    const rows = table.selectedRows();
    const none = !rows.length || busy;
    all6.forEach((b) => { b.disabled = none; });
    if (!none) {
      bStart.disabled = rows.every((r) => r.status === 'running');
      bStop.disabled = rows.every((r) => r.status === 'stopped');
    }
  }

  function apply() {
    const rows = all.filter((s) => (filter === 'all' || s.status === filter)
      && (!q || s.name.toLowerCase().includes(q) || s.display.toLowerCase().includes(q) || (s.path || '').toLowerCase().includes(q)));
    table.setRows(rows);
    head.setSub(`${rows.length} of ${all.length} services${rows.length > table.shown ? ` · showing first ${table.shown}` : ''}`);
    paintButtons();
  }

  async function load(manual) {
    refreshBtn.disabled = true;
    try { all = (await ctx.api.invoke('list_services')).services; apply(); }
    catch (e) { ctx.toast(`Could not read services: ${e.message}`, 'bad'); head.setSub('Could not read services'); }
    finally { refreshBtn.disabled = false; }
  }

  async function run(action) {
    const rows = table.selectedRows();
    if (!rows.length) { ctx.toast('Select one or more services first'); return; }
    const system = rows.filter((r) => r.system);
    const [verb, past] = VERB[action];
    if (['stop', 'restart', 'disable'].includes(action) && system.length) {
      const ok = await ui.confirmDialog({ title: 'Windows service', danger: true, confirmLabel: `${verb} anyway`,
        text: `${system.length} selected service(s) belong to Windows (e.g. ${system[0].display}). ${verb} anyway? This can affect system stability.` });
      if (!ok) return;
    } else if (action === 'disable' && rows.length > 1) {
      const ok = await ui.confirmDialog({ title: 'Disable services', danger: true, confirmLabel: 'Disable', text: `Stop and disable ${rows.length} services?` });
      if (!ok) return;
    }
    busy = true; paintButtons();
    rows.forEach((r) => setFx(r.name, 'working'));
    apply();
    try {
      const { results } = await ctx.api.invoke('set_service_state', { names: rows.map((r) => r.name), action });
      const disp = new Map(rows.map((r) => [r.name, r.display]));
      const byName = new Map(all.map((s) => [s.name, s]));
      const slow = [];
      // show what Windows REPORTS after the action (the backend re-reads it), not what we hoped for
      for (const r of results) {
        const s = byName.get(r.name);
        if (s && r.status && r.status !== 'unknown') s.status = r.status;
        if (s && r.start && r.start !== 'unknown') s.start = r.start;
        if (r.pending) { slow.push(r.name); setFx(r.name, 'pending'); } else setFx(r.name, r.ok && r.verified ? 'verified' : r.ok ? 'ok' : 'bad', r.ok ? 3500 : 3000);
      }
      apply();
      const bad = results.filter((r) => !r.ok);
      const say = (r) => `${disp.get(r.name) || r.name}: ${r.ok ? (r.verified ? `${past.toLowerCase()} - confirmed, now ${r.status} · ${START[r.start] || r.start}` : r.pending ? 'still working…' : past.toLowerCase()) : r.message}`;
      results.slice(0, 4).forEach((r) => ctx.toast(say(r), r.ok ? 'ok' : 'bad'));
      if (results.length > 4) ctx.toast(`${results.length - 4} more: ${results.length - bad.length} of ${results.length} succeeded`, bad.length ? 'bad' : 'ok');
      if (slow.length) watchSlow(slow, action);
    } catch (e) { rows.forEach((r) => setFx(r.name, null)); ctx.toast(e.message, 'bad'); }
    busy = false;
    paintButtons();
    await load();
  }

  /** A service that is still starting/stopping: poll its real state (max ~20 s) and settle the row when it gets there. */
  async function watchSlow(names, action) {
    const want = { start: 'running', restart: 'running', stop: 'stopped' }[action];
    let left = names.slice();
    for (let i = 0; i < 20 && left.length && alive; i++) {
      await new Promise((r) => setTimeout(r, 1000));
      if (!alive) return;
      let st;
      try { st = (await ctx.api.invoke('service_status', { names: left })).services; } catch { break; }
      const byName = new Map(all.map((s) => [s.name, s]));
      for (const x of st) {
        const s = byName.get(x.name);
        if (s) { s.status = x.status; s.start = x.start; }
        if (x.status !== 'pending') { left = left.filter((n) => n !== x.name); setFx(x.name, !want || x.status === want ? 'verified' : 'bad', 3000); }
      }
      apply();
    }
    left.forEach((n) => setFx(n, null));
    apply();
  }

  const card = h('div', { class: 'card tcard' }, table.el);
  paintButtons();
  await load();
  return [head, h('div', { class: 'toolbar' }, search, sel, refreshBtn), card,
    h('div', { class: 'actions' }, bStart, bStop, bRestart, h('span', { class: 'vsep' }), bEnable, bManual, bDisable)];
}
