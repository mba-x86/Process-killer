// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Activity log: newest at the bottom, live via 'activity:entry', at most MAX rows in the DOM.
const MAX = 500;
const LEVELS = {
  info: ['INFO', '#8DBBFF'], ok: ['OK', '#5BD69A'], fail: ['FAIL', '#FF8080'], slow: ['SLOW', '#F4B740'],
};
const pad = (n) => String(n).padStart(2, '0');
const clock = (ts) => { const d = new Date(ts * 1000); return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`; };

export default async function (ctx) {
  const { h, ui, api, icon } = ctx;
  const list = h('section', { class: 'card', 'aria-label': 'Log', style: { flex: '1', minHeight: '400px', padding: '12px 10px', overflow: 'auto' } });
  const emptyEl = h('div', { class: 'dim', style: { padding: '24px 8px', textAlign: 'center' } }, 'Nothing logged yet');

  const row = (e) => {
    const slow = e.level === 'info' && /^slow task/i.test(e.message || '');
    const [txt, color] = LEVELS[slow ? 'slow' : e.level] || LEVELS.info;
    return h('div', { class: 'ln' },
      h('span', { class: 'mono', style: { color: 'var(--muted)', flex: 'none' } }, clock(e.ts)),
      h('span', { class: 'lv', style: { color } }, txt),
      h('span', { class: 'mono', style: { color: 'var(--text)', userSelect: 'text', minWidth: 0, overflowWrap: 'anywhere' } }, String(e.message ?? '')));
  };

  const add = (items) => {
    if (!items.length) return;
    emptyEl.remove();
    const stick = list.scrollTop + list.clientHeight >= list.scrollHeight - 24;
    const frag = document.createDocumentFragment();
    for (const e of items.slice(-MAX)) frag.append(row(e));
    list.append(frag);
    while (list.childElementCount > MAX) list.firstElementChild.remove();
    if (stick) list.scrollTop = list.scrollHeight;
  };
  const clear = () => list.replaceChildren(emptyEl);

  // Page-local styles for log rows (design: .ln / .lv).
  const style = h('style', {}, `.ln{display:flex;align-items:baseline;gap:14px;padding:5px 8px;border-radius:6px;line-height:1.5}
.ln:hover{background:var(--row-hover)}.ln .mono{font-size:12.5px}.lv{flex:none;width:44px;font-size:11px;font-weight:600;letter-spacing:.04em}`);

  const bar = h('div', { class: 'row', style: { '--g': '8px' } },
    ui.button({ label: 'Open log file', icon: 'folder', onClick: () => api.invoke('open_log_file').catch((e) => ctx.toast(e.message, 'bad')) }),
    ui.button({ label: 'Clear view', onClick: () => { clear(); api.invoke('clear_activity').catch(() => {}); } }));

  clear();
  const first = await api.invoke('get_activity', { limit: MAX });
  add(first.entries || []);
  list.scrollTop = list.scrollHeight;
  api.listen('activity:entry', (e) => add([e])).then((un) => ctx.onCleanup(un));

  return [style, ui.pageHead({ title: 'Activity', sub: 'Everything this app has done' }), bar, list];
}
