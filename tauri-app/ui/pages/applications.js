// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Applications: installed-apps table + detail panel + uninstall wizard (pages/applications/wizard.js).
import { debounce } from '../js/dom.js';
import { openWizard } from './applications/wizard.js';

const LIMIT = 300;                       // never render more rows than this
const NOTE = {
  broken: 'The uninstaller file is missing, so Windows can’t remove this. Process Killer can remove the dead entry and its leftover files.',
  edge: 'Built into Windows. Microsoft doesn’t officially support removing Edge, and Windows Update can bring it back. Process Killer can remove it safely (WebView2 is kept).',
};
const COLS = [['name', 'Name'], ['publisher', 'Publisher'], ['version', 'Version'], ['size', 'Size', true], ['date', 'Installed']];
const text = (a, b) => String(a || '').localeCompare(String(b || ''), undefined, { numeric: true, sensitivity: 'base' });
const CMP = { name: (a, b) => text(a.name, b.name), publisher: (a, b) => text(a.publisher, b.publisher), version: (a, b) => text(a.version, b.version),
  size: (a, b) => (a.size || 0) - (b.size || 0), date: (a, b) => text(a.date, b.date) };

export default async function (ctx) {
  const { h, ui, api, fmt, icon } = ctx;
  ctx.css('css/pages/applications.css');
  let apps = [], loaded = false, cur = null, q = '', drive = 'All drives', sortKey = 'name', sortDir = 1, token = 0, dead = false;
  ctx.onCleanup(() => { dead = true; });

  const head = ui.pageHead({ title: 'Applications', sub: 'Loading installed applications…' });

  /* ---------- toolbar ---------- */
  const search = ui.searchBox({ placeholder: 'Search by name or publisher…', onInput: debounce((v) => { q = v.trim().toLowerCase(); apply(); }, 150) });
  search.style.flex = '0 1 300px';
  search.querySelector('input').style.width = '100%';
  const driveSel = h('select', { class: 'select', style: { width: '120px' }, 'aria-label': 'Drive', onchange: (e) => { drive = e.target.value; apply(); } },
    h('option', { value: 'All drives' }, 'All drives'));
  const toolbar = h('div', { class: 'row wrap' }, search, driveSel, ui.button({ label: 'Refresh', icon: 'refresh', onClick: () => reload() }));

  /* ---------- table (own tiny table: sorting happens BEFORE the 300-row cap) ---------- */
  const thead = h('thead'), tbody = h('tbody');
  const tblWrap = h('div', { class: 'tbl-wrap' }, h('table', { class: 'tbl apps-tbl' }, thead, tbody));
  const paintHead = () => thead.replaceChildren(h('tr', {}, COLS.map(([k, label, num]) =>
    h('th', { class: ['sortable', num && 'num'], onclick: () => { sortDir = sortKey === k ? -sortDir : 1; sortKey = k; paintHead(); apply(); } },
      label, sortKey === k ? ` ${sortDir > 0 ? '↑' : '↓'}` : ''))));
  const dash = (cls) => h('td', { class: ['d', cls] }, '—');
  const row = (a) => h('tr', { class: ['click', cur?.id === a.id && 'sel'], dataset: { id: a.id } },
    h('td', { class: 'name', title: a.name }, a.name),
    a.publisher ? h('td', { class: 'pub', title: a.publisher }, a.publisher) : dash(),
    a.version ? h('td', { class: 'mono' }, a.version) : dash(),
    a.size > 0 ? h('td', { class: 'num' }, fmt.bytes(a.size)) : dash('num'),
    !a.date ? dash() : a.date === 'Unknown' ? h('td', { class: 'd', title: 'The registry holds a date that is not valid' }, 'Unknown') : h('td', { class: 'dt' }, a.date));
  const message = (t) => tbody.replaceChildren(h('tr', {}, h('td', { class: 'msg', colspan: 5 }, t)));
  tbody.addEventListener('click', (e) => { const a = appOf(e); if (a) select(a); });
  tbody.addEventListener('dblclick', (e) => { const a = appOf(e); if (a) uninstall(a); });
  const appOf = (e) => { const id = e.target.closest('tr')?.dataset.id; return id && apps.find((a) => a.id === id); };

  function apply() {
    if (!loaded) return;
    const list = apps.filter((a) => (!q || a.name.toLowerCase().includes(q) || a.publisher.toLowerCase().includes(q)) && (drive === 'All drives' || a.drive === drive));
    const cmp = CMP[sortKey];
    list.sort((a, b) => sortDir * cmp(a, b));
    head.setSub(`${list.length} of ${apps.length} applications${q ? ` matching “${q}”` : ''}${drive !== 'All drives' ? ` on ${drive}` : ''}`);
    if (!list.length) return message(apps.length ? 'No applications match.' : 'No installed applications were found.');
    const frag = document.createDocumentFragment();
    for (const a of list.slice(0, LIMIT)) frag.append(row(a));
    if (list.length > LIMIT) frag.append(h('tr', { class: 'more' }, h('td', { colspan: 5 }, `Showing the first ${LIMIT} of ${list.length} — refine the search to see the rest.`)));
    tbody.replaceChildren(frag);
  }

  /* ---------- detail panel (built once, updated in place) ---------- */
  const el = {
    empty: h('div', { class: 'a-empty' }, 'Select an application to see what is running in the background and uninstall it safely.'),
    tile: h('span', { class: 'apps-tile' }), name: h('h2'), meta: h('span', { class: 'meta' }),
    open: ui.button({ icon: 'folder', kind: 'ghost', small: true, title: 'Open install folder', onClick: () => cur?.location && api.invoke('open_folder', { path: cur.location }).catch((e) => ctx.toast(e.message, 'bad')) }),
    note: h('div', { class: 'apps-note' }), noteText: h('span'),
    rel: h('div', { class: 'apps-rel' }), hint: h('p'),
    btn: h('button', { class: 'btn btn-danger btn-block', type: 'button', onclick: () => cur && uninstall(cur) }),
  };
  el.note.append(icon('warn', 14), el.noteText);
  const detail = h('div', { class: 'apps-detail' },
    h('div', { class: 'apps-head' }, el.tile, h('div', { class: 'col', style: { '--g': '3px', minWidth: 0, flex: 1 } }, el.name, el.meta)),
    el.note,
    h('div', { class: 'col' }, h('div', { class: 'row between' }, h('h3', { class: 'lbl' }, 'Running in the background'), el.open), el.rel),
    h('div', { class: 'apps-foot' }, el.hint, el.btn));
  detail.hidden = true;
  const panel = h('aside', { class: 'card apps-panel', 'aria-label': 'Application details' }, el.empty, detail);

  const relRow = (cls, ...kids) => h('div', { class: ['r', cls] }, ...kids);
  function paintRelated(items) {
    if (!items.length) return el.rel.replaceChildren(relRow('info good', icon('check', 14), 'Nothing is running or registered in the background.'));
    el.rel.replaceChildren(...items.map((r) => r.kind === 'process'
      ? relRow('', h('span', {}, r.name, r.count > 1 ? h('span', { class: 'dim' }, ` × ${r.count}`) : null), r.mem ? h('span', { class: 'mem' }, fmt.bytes(r.mem)) : null)
      : relRow('', h('div', { class: 'l' }, h('span', {}, r.name), r.detail ? h('span', { class: 'sub' }, r.detail) : null),
        h('span', { class: ['st', /^(running|ready|enabled)$/i.test(r.state) && 'on'] }, r.state))));
  }

  function select(a) {
    cur = a;
    const t = ++token;
    tbody.querySelector('tr.sel')?.classList.remove('sel');
    tbody.querySelector(`tr[data-id="${CSS.escape(a.id)}"]`)?.classList.add('sel');
    el.empty.hidden = true; detail.hidden = false;
    el.tile.textContent = ([...a.name.trim()][0] || '?').toUpperCase();
    el.name.textContent = a.name;
    el.meta.textContent = [a.publisher, a.version].filter(Boolean).join(' · ') || 'No details published';
    el.meta.title = a.location || '';
    el.open.hidden = !a.location;
    const warn = a.is_edge ? NOTE.edge : a.uninstaller_missing ? NOTE.broken : '';
    el.note.hidden = !warn; el.noteText.textContent = warn;
    el.hint.textContent = a.is_edge ? 'Needs Administrator rights and another web browser. WebView2 is not touched.'
      : a.uninstaller_missing ? 'A backup of the registry entry is saved first so it can be restored.'
        : `Uninstalling closes ${a.name} first and backs up its registry entries.`;
    el.btn.replaceChildren(...(a.is_edge || a.uninstaller_missing ? [] : [icon('trash', 15)]),
      a.is_edge ? 'Remove Microsoft Edge…' : a.uninstaller_missing ? 'Remove broken entry' : 'Uninstall safely');
    el.rel.replaceChildren(relRow('info', ui.spinner(), 'Checking processes, services and startup items…'));
    api.invoke('get_related', { app_id: a.id }).then((r) => { if (t === token && !dead) paintRelated(r.items || []); })
      .catch((e) => { if (t === token && !dead) el.rel.replaceChildren(relRow('info', e.message)); });
  }

  const uninstall = (a) => openWizard(ctx, a, () => { if (!dead) reload(); });

  /* ---------- data ---------- */
  async function reload() {
    head.setSub('Loading installed applications…');
    if (!loaded) message('Loading installed applications…');
    try {
      apps = (await api.invoke('list_apps')).apps || [];
      if (dead) return;
      loaded = true;
      const drives = [...new Set(apps.map((a) => a.drive).filter(Boolean))].sort();
      if (!drives.includes(drive)) drive = 'All drives';
      driveSel.replaceChildren(...['All drives', ...drives].map((d) => h('option', { value: d, selected: d === drive }, d)));
      if (cur) { const again = apps.find((a) => a.id === cur.id); if (again) cur = again; else { cur = null; token++; detail.hidden = true; el.empty.hidden = false; } }
      apply();
    } catch (e) { if (!dead) { head.setSub('Could not load applications'); message(e.message); ctx.toast(e.message, 'bad'); } }
  }

  paintHead();
  reload().then(demo);              // render the shell instantly; rows arrive when the backend answers

  /** Browser-only helper for screenshots: ?demo=panel-edge|panel-broken|wizard|wizard-edge|wizard-broken|wizard-run */
  function demo() {
    const d = !api.inTauri && new URLSearchParams(location.search).get('demo');
    if (!d) return;
    const [kind, what] = d.split('-');
    const a = apps.find((x) => (what === 'edge' ? x.is_edge : what === 'broken' ? x.uninstaller_missing : x.name === 'Brave'));
    if (!a) return;
    select(a);
    if (kind === 'wizard') {
      const m = openWizard(ctx, a, () => {});
      if (what === 'run') setTimeout(() => [...m.el.querySelectorAll('.modal-foot .btn-p')].forEach((b) => b.click()), 900);
    }
  }

  const card = h('div', { class: 'card apps-tbl-card' }, tblWrap);
  return [head, toolbar, h('div', { class: 'split' }, card, panel)];
}
