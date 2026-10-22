// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Shared widgets. Keep page-specific pieces inside the page file; extend this file only for things 2+ pages need.
import { h, setChildren } from './dom.js';
import { icon } from './icons.js';

export const button = ({ label, icon: ic, kind, small, onClick, disabled, title } = {}) =>
  h('button', { class: ['btn', kind === 'primary' && 'btn-p', kind === 'danger' && 'btn-danger', kind === 'ghost' && 'btn-ghost', small && 'btn-sm'], type: 'button', disabled, title, onclick: onClick },
    ic ? icon(ic, small ? 13 : 15) : null, label);

export const card = (props, ...kids) => h('div', { ...props, class: ['card', props?.class] }, ...kids);

export const pageHead = ({ title, sub, back, right } = {}) => {
  const subEl = h('p', {}, sub ?? '');
  const el = h('header', { class: 'page-head' },
    back ? h('a', { class: 'back', onclick: () => (location.hash = `#/${back.to}`) }, icon('back', 14), back.label) : null,
    h('div', { class: 'head-row' }, h('div', { class: 'col', style: { '--g': '6px' } }, h('h1', {}, title), subEl), right || null));
  el.setSub = (t) => { subEl.textContent = t; };
  return el;
};

/** Search box with magnifier. onInput gets the string. */
export const searchBox = ({ placeholder = 'Search…', onInput, width } = {}) =>
  h('label', { class: 'search' }, icon('search', 14), h('input', { type: 'search', placeholder, style: width ? { width } : null, oninput: (e) => onInput?.(e.target.value) }));

export const select = ({ options, value, onChange }) =>
  h('select', { class: 'select', onchange: (e) => onChange?.(e.target.value) }, options.map((o) => {
    const [v, l] = Array.isArray(o) ? o : [o, o];
    return h('option', { value: v, selected: v === value }, l);
  }));

export const checkbox = ({ checked, onChange, label, disabled } = {}) => {
  const box = h('input', { type: 'checkbox', class: 'chk', checked, disabled, onchange: (e) => onChange?.(e.target.checked) });
  return label ? h('label', { class: 'row', style: { '--g': '10px', cursor: 'pointer' } }, box, label) : box;
};

export function segmented({ options, value, onChange }) {
  const el = h('div', { class: 'seg', role: 'tablist' });
  const paint = (v) => [...el.children].forEach((b) => b.classList.toggle('on', b.dataset.v === v));
  options.forEach(([v, l]) => el.append(h('button', { dataset: { v }, type: 'button', onclick: () => { paint(v); onChange?.(v); } }, l)));
  paint(value);
  el.set = paint;
  return el;
}

export const dot = (kind = 'ok') => h('span', { class: ['dot', kind !== 'ok' && kind] });
export const pill = (...kids) => h('span', { class: 'pill' }, ...kids);
export const badge = (t) => h('span', { class: 'badge' }, t);
export const spinner = () => h('span', { class: 'spin', role: 'status' });
export const empty = (msg) => h('div', { class: 'empty' }, msg);

/** Spec list: specList([['Cores','10'],['Base','2.5 GHz', 'ok']]) -> <dl class="spec"> (3rd item = tone class) */
export const specList = (rows) => h('dl', { class: 'spec' }, rows.map(([k, v, tone]) => [h('dt', {}, k), h('dd', { class: tone || null }, v)]));

/**
 * Sortable table. columns: [{key,label,num,mono,sort:(a,b)=>n | true,render:(row)=>Node|string,width}]
 *   t = dataTable({columns, rows, rowKey:'pid', onSelect(row), selected: key, sortKey, sortDir:1|-1, empty:'Nothing here'})
 *   t.setRows(rows)  t.select(key)  t.el  (wrap it in a card; the header is sticky inside .tbl-wrap)
 */
export function dataTable({ columns, rows = [], rowKey = 'id', onSelect, onDblClick, selected, sortKey, sortDir = 1, emptyText = 'Nothing to show', dense } = {}) {
  let data = rows, sel = selected, key = sortKey, dir = sortDir;
  const tbody = h('tbody'), thead = h('thead');
  const el = h('div', { class: 'tbl-wrap' }, h('table', { class: 'tbl' }, thead, tbody));
  const cmp = (c) => (typeof c.sort === 'function' ? c.sort : (a, b) => (a[c.key] > b[c.key]) - (a[c.key] < b[c.key]));
  const paintHead = () => setChildren(thead, h('tr', {}, columns.map((c) => {
    const sortable = c.sort !== false;
    return h('th', { class: [c.num && 'num', sortable && 'sortable'], style: c.width ? { width: c.width } : null,
      onclick: sortable ? () => { dir = key === c.key ? -dir : 1; key = c.key; render(); } : null },
    c.label, key === c.key ? ` ${dir > 0 ? '↑' : '↓'}` : '');
  })));
  function render() {
    paintHead();
    const col = columns.find((c) => c.key === key);
    const list = col ? [...data].sort((a, b) => dir * cmp(col)(a, b)) : data;
    if (!list.length) { setChildren(tbody, h('tr', {}, h('td', { colspan: columns.length, class: 'dim', style: { textAlign: 'center', height: '80px' } }, emptyText))); return; }
    const frag = document.createDocumentFragment();
    for (const r of list) {
      const k = r[rowKey];
      frag.append(h('tr', { class: [onSelect && 'click', k === sel && 'sel'], dataset: { k }, onclick: onSelect ? () => { sel = k; render(); onSelect(r); } : null, ondblclick: onDblClick ? () => onDblClick(r) : null },
        columns.map((c) => h('td', { class: [c.num && 'num tnum', c.mono && 'mono', c.dim && 'dim'], title: c.title ? c.title(r) : null }, c.render ? c.render(r) : (r[c.key] ?? '—')))));
    }
    tbody.replaceChildren(frag);
  }
  render();
  return { el, setRows(r) { data = r; render(); }, select(k) { sel = k; render(); }, get rows() { return data; } };
}

/* ---------- toasts & modal (mounted lazily on <body>) ---------- */
let toastHost;
export function toast(message, kind = '', ms = 4200) {
  toastHost ||= document.body.appendChild(h('div', { class: 'toasts', role: 'status', 'aria-live': 'polite' }));
  const t = h('div', { class: ['toast', kind] }, message);
  toastHost.append(t);
  setTimeout(() => t.remove(), ms);
}

/** modal({title, body: Node, actions:[Node], steps:['Review',...], current:0}) -> {el, close, setBody, setActions, setStep} */
export function modal({ title, body, actions = [], steps, current = 0, onClose, dismissable = true } = {}) {
  const head = h('div', { class: 'modal-head' }, h('h2', {}, title));
  const stepsEl = h('div', { class: 'steps' });
  const bodyEl = h('div', { class: 'modal-body' }, body);
  const foot = h('div', { class: 'modal-foot' }, actions);
  const overlay = h('div', { class: 'overlay', onmousedown: (e) => { if (dismissable && e.target === overlay) api.close(); } },
    h('div', { class: 'modal', role: 'dialog', 'aria-modal': 'true', 'aria-label': title }, head, steps ? stepsEl : null, bodyEl, foot));
  const onKey = (e) => { if (e.key === 'Escape' && dismissable) api.close(); };
  const api = {
    el: overlay,
    close() { document.removeEventListener('keydown', onKey); overlay.remove(); if (prevFocus?.isConnected) prevFocus.focus?.(); onClose?.(); },
    setBody(n) { setChildren(bodyEl, n); }, setActions(a) { setChildren(foot, a); },
    setTitle(t) { head.firstChild.textContent = t; },
    setStep(i) { setChildren(stepsEl, steps.map((s, n) => h('span', { class: n < i ? 'done' : n === i ? 'on' : '' }, `${n < i ? '✓' : n + 1}  ${s}`))); },
  };
  if (steps) api.setStep(current);
  document.addEventListener('keydown', onKey);
  const prevFocus = document.activeElement;
  document.body.append(overlay);
  // keyboard users: move focus into the dialog (the primary action, else the dialog itself) and give it back on close
  const modalEl = overlay.firstChild;
  modalEl.tabIndex = -1;
  queueMicrotask(() => { if (overlay.isConnected && !overlay.contains(document.activeElement)) (foot.querySelector('.btn-p, .btn-danger') || modalEl).focus(); });
  return api;
}

/** Confirm dialog -> Promise<boolean> */
export function confirmDialog({ title, text, confirmLabel = 'Continue', danger = false } = {}) {
  return new Promise((resolve) => {
    let ok = false;
    const m = modal({ title, body: h('p', { class: 'muted', style: { lineHeight: 1.5 } }, text), onClose: () => resolve(ok) });
    m.setActions([
      button({ label: 'Cancel', onClick: () => m.close() }),
      button({ label: confirmLabel, kind: danger ? 'danger' : 'primary', onClick: () => { ok = true; m.close(); } }),
    ]);
  });
}
