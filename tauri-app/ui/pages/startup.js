// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { seltable, toastResults } from './_seltable.js';

const ci = (k) => (a, b) => String(a[k]).localeCompare(String(b[k]), undefined, { sensitivity: 'base' });

export default async function (ctx) {
  const { h, ui } = ctx;
  ctx.css('css/pages/seltable.css');
  let items = [], busy = false;

  const head = ui.pageHead({ title: 'Startup & tasks', sub: 'Programs that launch by themselves' });
  const table = seltable({
    rowKey: 'id', sortKey: 'kind', limit: 300, emptyText: 'Nothing starts automatically',
    onSelect: () => paint(),
    columns: [
      { key: 'kind', label: 'Type', width: '124px', sort: (a, b) => (a.kind > b.kind) - (a.kind < b.kind) || ci('name')(a, b),
        text: (r) => (r.kind === 'task' ? 'Scheduled task' : 'Startup entry'),
        render: (r) => h('span', { class: ['tk', r.kind === 'startup' && 'startup'] }, r.kind === 'task' ? 'Scheduled task' : 'Startup entry') },
      { key: 'name', label: 'Name', sort: ci('name'), width: '250px', text: (r) => r.name, title: (r) => r.name },
      { key: 'command', label: 'Command', mono: true, cls: 'path', sort: ci('command'), text: (r) => r.command || '—', title: (r) => r.command || '' },
      { key: 'location', label: 'Location', width: '140px', text: (r) => r.location || r.status || '—' },
    ],
  });
  const refreshBtn = ui.button({ label: 'Refresh', icon: 'refresh', onClick: load });
  const bRemove = ui.button({ label: 'Remove selected', icon: 'trash', kind: 'danger', onClick: remove });
  const paint = () => { bRemove.disabled = busy || !table.selected.size; };

  async function load() {
    refreshBtn.disabled = true; head.setSub('Loading…');
    try {
      items = (await ctx.api.invoke('list_startup')).items;
      table.setRows(items);
      const n = items.filter((i) => i.kind === 'startup').length;
      head.setSub(`${n} startup entries · ${items.length - n} scheduled tasks`);
    } catch (e) { ctx.toast(`Could not read startup entries: ${e.message}`, 'bad'); head.setSub('Could not read startup entries'); }
    refreshBtn.disabled = false; paint();
  }

  async function remove() {
    const rows = table.selectedRows();
    if (!rows.length) { ctx.toast('Select entries to remove first'); return; }
    const ok = await ui.confirmDialog({ title: 'Remove', danger: true, confirmLabel: 'Remove', text: `Permanently remove ${rows.length} item(s)?` });
    if (!ok) return;
    busy = true; paint();
    try {
      const { results } = await ctx.api.invoke('remove_startup', { ids: rows.map((r) => r.id) });
      const names = new Map(rows.map((r) => [r.id, r.name]));
      toastResults(ctx, results.map((r) => ({ ...r, label: names.get(r.id) })), 'Removed');
    } catch (e) { ctx.toast(e.message, 'bad'); }
    busy = false;
    await load();
  }

  paint();
  await load();
  return [head, h('div', { class: 'toolbar' }, refreshBtn), h('div', { class: 'card tcard' }, table.el),
    h('div', { class: 'actions', style: { gap: '16px' } }, bRemove, h('span', { class: 'hint' }, "Microsoft's built-in tasks are hidden."))];
}
