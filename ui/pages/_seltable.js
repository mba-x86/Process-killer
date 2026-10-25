// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Multi-select, sortable, keyed in-place table used by services / processes / startup (not a route).
//   columns: [{ key, label, num, mono, cls, width, sort:(a,b)=>n|false, text:(row)=>string, render?:(row)=>Node }]
//   t = seltable({ columns, rowKey, sortKey, sortDir, limit, emptyText, rowClass:(row)=>string, rowTitle:(row)=>string, onSelect:(Set)=>void })
//   t.setRows(rows)  -> rows already filtered; sorted + limited here; DOM rows are reused by key.
import { h, setChildren } from '../js/dom.js';

export function seltable({ columns, rowKey = 'id', sortKey, sortDir = 1, limit = 300, emptyText = 'Nothing to show', rowClass, rowTitle, onSelect } = {}) {
  let data = [], shown = [], key = sortKey, dir = sortDir, anchor = null;
  const sel = new Set(), trs = new Map();
  const thead = h('thead'), tbody = h('tbody');
  const el = h('div', { class: 'tbl-wrap' }, h('table', { class: 'tbl sel-tbl' }, thead, tbody));
  const emptyRow = h('tr', {}, h('td', { colspan: columns.length, class: 'dim', style: { textAlign: 'center', height: '80px', maxWidth: 'none' } }, emptyText));
  const cmp = (c) => (typeof c.sort === 'function' ? c.sort : (a, b) => (a[c.key] > b[c.key]) - (a[c.key] < b[c.key]));

  function paintHead() {
    setChildren(thead, h('tr', {}, columns.map((c) => {
      const sortable = c.sort !== false;
      return h('th', { class: [c.num && 'num', sortable && 'sortable'], style: c.width ? { width: c.width } : null,
        onclick: sortable ? () => { dir = key === c.key ? -dir : (c.num ? -1 : 1); key = c.key; paintHead(); layout(); } : null },
      c.label, key === c.key ? ` ${dir > 0 ? '↑' : '↓'}` : '');
    })));
  }
  const fill = (td, c, row) => {
    const sig = c.text ? c.text(row) : String(row[c.key] ?? '—');
    const full = c.sig ? c.sig(row) : sig;
    if (td._s === full) return;
    td._s = full;
    if (c.render) setChildren(td, c.render(row)); else td.textContent = sig;
    if (c.title) td.title = c.title(row);
  };
  function makeRow(k) {
    const tr = h('tr', { class: 'click', dataset: { k } },
      columns.map((c) => h('td', { class: [c.num && 'num tnum', c.mono && 'mono', c.cls] })));
    trs.set(k, tr);
    return tr;
  }
  function paintRow(tr, row) {
    columns.forEach((c, i) => fill(tr.children[i], c, row));
    const cls = `click${sel.has(String(row[rowKey])) ? ' sel' : ''}${rowClass ? ' ' + (rowClass(row) || '') : ''}`;
    if (tr.getAttribute('class') !== cls) tr.setAttribute('class', cls);
    const t = rowTitle ? rowTitle(row) : '';
    if (tr._t !== t) { tr._t = t; if (t) tr.title = t; else tr.removeAttribute('title'); }
  }
  function layout() {
    const col = columns.find((c) => c.key === key);
    const list = col ? [...data].sort((a, b) => dir * cmp(col)(a, b)) : data;
    shown = list.slice(0, limit);
    const alive = new Set();
    let prev = null;
    if (!shown.length) { tbody.replaceChildren(emptyRow); trs.clear(); return; }
    if (tbody.firstChild === emptyRow) tbody.replaceChildren();
    for (const row of shown) {
      const k = String(row[rowKey]);
      alive.add(k);
      const tr = trs.get(k) || makeRow(k);
      tr._row = row;
      paintRow(tr, row);
      const want = prev ? prev.nextSibling : tbody.firstChild;
      if (want !== tr) tbody.insertBefore(tr, want);
      prev = tr;
    }
    for (const [k, tr] of trs) if (!alive.has(k)) { tr.remove(); trs.delete(k); }
  }
  const paintSel = () => { for (const [k, tr] of trs) tr.classList.toggle('sel', sel.has(k)); };

  tbody.addEventListener('click', (e) => {
    const tr = e.target.closest('tr');
    if (!tr || !tr._row) return;
    const k = tr.dataset.k;
    if (e.shiftKey && anchor != null) {
      const ks = shown.map((r) => String(r[rowKey])), a = ks.indexOf(anchor), b = ks.indexOf(k);
      if (a >= 0 && b >= 0) { if (!e.ctrlKey) sel.clear(); ks.slice(Math.min(a, b), Math.max(a, b) + 1).forEach((x) => sel.add(x)); }
    } else if (e.ctrlKey || e.metaKey) {
      if (sel.has(k)) sel.delete(k); else sel.add(k);
      anchor = k;
    } else { sel.clear(); sel.add(k); anchor = k; }
    paintSel(); onSelect?.(sel);
  });

  paintHead(); layout();
  return {
    el, selected: sel,
    get total() { return data.length; },
    get shown() { return shown.length; },
    selectedRows() { return data.filter((r) => sel.has(String(r[rowKey]))); },
    setRows(rows) {
      data = rows;
      const present = new Set(rows.map((r) => String(r[rowKey])));
      let changed = false;
      for (const k of [...sel]) if (!present.has(k)) { sel.delete(k); changed = true; }
      layout();
      if (changed) onSelect?.(sel);
    },
    clearSelection() { sel.clear(); paintSel(); onSelect?.(sel); },
  };
}

/** Run an async batch action and toast one line per item (max 4, then a summary). */
export function toastResults(ctx, results, label) {
  const bad = results.filter((r) => !r.ok);
  const good = results.length - bad.length;
  bad.slice(0, 4).forEach((r) => ctx.toast(`${r.label || r.name || r.id || r.pid}: ${r.message}`, 'bad'));
  if (bad.length > 4) ctx.toast(`${bad.length - 4} more failed`, 'bad');
  if (good) ctx.toast(`${label} ${good} of ${results.length}`, 'ok');
}
