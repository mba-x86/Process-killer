// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Duplicate finder panel of the storage analyzer. API: docs/api/storage.md (dupes_scan_start / dupes_recycle).
//   const d = createDupes({ ctx, roots: () => string[], onBusy: (busy) => void })
//   d.el  -> the panel      d.start()  -> scan the current roots      d.stop()      d.busy
const MB = 1048576;
const SIZES = [['1', '≥ 1 MB'], ['10', '≥ 10 MB'], ['100', '≥ 100 MB']];
const KEEP = [['oldest', 'Keep the oldest'], ['newest', 'Keep the newest'], ['short', 'Keep the shortest path']];
const PAGE = 40;

export function createDupes({ ctx, roots, onBusy }) {
  const { h, ui, fmt, api, icon } = ctx;
  const S = { job: null, starting: false, busy: false, working: false, groups: [], sel: new Set(), shown: PAGE, stats: null, done: false };
  const un = [];

  /* ---------------- controls ---------------- */
  const sizeSel = ui.select({ options: SIZES, value: '10' });
  sizeSel.classList.add('field'); sizeSel.style.width = '110px'; sizeSel.setAttribute('aria-label', 'Minimum duplicate size');
  const keepSel = ui.select({ options: KEEP, value: 'oldest' });
  keepSel.classList.add('field'); keepSel.style.width = '190px'; keepSel.setAttribute('aria-label', 'Which copy to keep');
  const bAuto = ui.button({ label: 'Select the extra copies', icon: 'check', onClick: autoSelect });
  const bNone = ui.button({ label: 'Clear selection', onClick: () => { S.sel = new Set(); paintGroups(); } });
  const bDel = ui.button({ label: 'Move to Recycle Bin', icon: 'trash', kind: 'danger', onClick: recycle });
  const note = h('span', { class: 'note' });
  const bar = h('div', { class: 'st-tools' }, h('span', { class: 'lbl-t' }, 'Minimum size'), sizeSel,
    h('span', { class: 'muted', style: { marginLeft: '6px' } }, 'Finds files with exactly the same content'));
  const progFill = h('i'), prog = h('div', { class: 'st-bar dp-prog' }, progFill);
  const status = h('span', { class: 'st-status' }, 'Pick a partition and press Scan to look for duplicate files.');
  const progBox = h('div', { class: 'col' }, prog, status);
  const summary = h('div', { class: 'dp-sum' });
  const tools = h('div', { class: 'st-tools dp-tools' }, keepSel, bAuto, bNone);
  const list = h('div', { class: 'dp-list' });
  const more = ui.button({ label: 'Show more', onClick: () => { S.shown += PAGE; paintGroups(); } });
  const acts = h('div', { class: 'st-acts' }, bDel, note);
  const el = h('div', { class: 'dp', hidden: true }, bar, progBox, summary, tools, list, acts);

  const nameOf = (p) => p.slice(p.lastIndexOf('\\') + 1);
  const dirOf = (p) => { const i = p.lastIndexOf('\\'); return i > 2 ? p.slice(0, i) : p.slice(0, i + 1); };
  const wasted = (g) => g.size * (g.count - 1);

  /* ---------------- selection ---------------- */
  function pickKeeper(g, mode) {
    const f = g.files.slice();
    if (mode === 'newest') f.sort((a, b) => b.mtime - a.mtime || a.path.length - b.path.length);
    else if (mode === 'short') f.sort((a, b) => a.path.length - b.path.length || a.mtime - b.mtime);
    else f.sort((a, b) => a.mtime - b.mtime || a.path.length - b.path.length);
    return f[0];
  }
  function autoSelect() {
    S.sel = new Set();
    for (const g of S.groups) { const keep = pickKeeper(g, keepSel.value); g.files.forEach((f) => { if (f !== keep) S.sel.add(f.path); }); }
    paintGroups();
    ctx.toast(`Selected ${S.sel.size} extra cop${S.sel.size === 1 ? 'y' : 'ies'} - one copy of every file is kept`);
  }
  function toggle(g, f, on, box) {
    if (on && g.files.every((x) => x === f || S.sel.has(x.path))) {   // would remove the last copy
      box.checked = false;
      ctx.toast('Keep at least one copy of each file', 'bad');
      return;
    }
    if (on) S.sel.add(f.path); else S.sel.delete(f.path);
    paintGroups();
  }

  /* ---------------- rendering ---------------- */
  function paintSummary() {
    if (!S.done) { summary.replaceChildren(); return; }
    const st = S.stats;
    const total = S.groups.reduce((s, g) => s + wasted(g), 0);
    summary.replaceChildren(S.groups.length
      ? h('div', { class: 'dp-card' }, h('div', {}, h('b', {}, fmt.bytes(total)), ' can be freed'), h('div', { class: 'muted' }, `${fmt.num(S.groups.length)} sets of identical files${st.total_groups > S.groups.length + (S.removedGroups || 0) ? ` (showing the ${fmt.num(S.groups.length + (S.removedGroups || 0))} biggest of ${fmt.num(st.total_groups)})` : ''}`))
      : h('div', { class: 'dp-card' }, h('div', {}, icon('check', 14, { 'stroke-width': 2 }), ' No duplicates found'), h('div', { class: 'muted' }, 'Nothing here is byte-for-byte identical at this size.')));
  }
  function paintGroups() {
    const frag = document.createDocumentFragment();
    const selBytes = S.groups.reduce((s, g) => s + g.size * g.files.filter((f) => S.sel.has(f.path)).length, 0);
    for (const g of S.groups.slice(0, S.shown)) {
      const body = g.files.map((f) => {
        const on = S.sel.has(f.path);
        const box = h('input', { type: 'checkbox', class: 'chk', checked: on, 'aria-label': `Remove ${f.path}` });
        box.onchange = () => toggle(g, f, box.checked, box);
        return h('label', { class: ['dp-file', on && 'rm'], title: f.path }, box,
          h('span', { class: 'dp-name' }, nameOf(f.path)), h('span', { class: 'mono dim dp-dir' }, dirOf(f.path)),
          h('span', { class: 'dim dp-date' }, fmt.date(f.mtime)), h('span', { class: ['dp-tag', on ? 'rm' : 'keep'] }, on ? 'Remove' : 'Keep'));
      });
      frag.append(h('section', { class: 'card dp-group' },
        h('header', {}, h('b', {}, nameOf(g.files[0].path)), h('span', { class: 'muted' }, `${g.count} copies · ${fmt.bytes(g.size)} each`), h('span', { class: 'dp-waste' }, `${fmt.bytes(wasted(g))} wasted`)),
        ...body, g.count > g.files.length ? h('div', { class: 'muted dp-more' }, `+ ${g.count - g.files.length} more copies not listed`) : null));
    }
    if (S.groups.length > S.shown) frag.append(h('div', { class: 'dp-showmore' }, more));
    list.replaceChildren(frag);
    paintSummary();
    const n = S.sel.size;
    const lock = S.busy || S.working;
    bDel.disabled = !n || lock;
    bAuto.disabled = bNone.disabled = !S.groups.length || lock;
    keepSel.disabled = !S.groups.length || lock;
    tools.hidden = acts.hidden = !S.groups.length;
    bDel.replaceChildren(icon('trash', 15), n ? `Move ${n} file${n > 1 ? 's' : ''} to Recycle Bin (${fmt.bytes(selBytes)})` : 'Move to Recycle Bin');
    note.textContent = S.groups.length ? 'One copy of every file always stays.' : '';
  }

  /* ---------------- scanning ---------------- */
  const cur = (p) => (p.length < 64 ? p : `…${p.slice(-62)}`);
  function onProgress(p) {
    if (S.job == null) { if (!S.starting) return; S.job = p.job; } else if (p.job !== S.job) return;
    if (p.phase === 'walk') {
      prog.classList.add('ind');
      status.textContent = `Looking for files of the same size… ${fmt.num(p.files)} files checked · ${cur(p.current || '')}`;
    } else {
      prog.classList.remove('ind');
      progFill.style.width = `${Math.min(99, p.hash_total ? (100 * p.hashed) / p.hash_total : 0)}%`;
      status.textContent = p.phase === 'head'
        ? `Comparing the start of ${fmt.num(p.candidates)} candidate files…`
        : `Reading the remaining files to confirm they are identical… ${fmt.bytes(p.hashed)} of ${fmt.bytes(p.hash_total)}`;
    }
  }
  function onDone(p) {
    if (p.job !== S.job) return;
    S.job = null; S.starting = false; S.done = !p.cancelled; S.stats = p; S.removedGroups = 0;
    S.groups = p.cancelled ? [] : p.groups;
    S.sel = new Set(); S.shown = PAGE;
    setBusy(false);
    prog.classList.remove('ind'); progFill.style.width = p.cancelled ? '0%' : '100%';
    const cloud = p.skipped_cloud ? ` ${fmt.num(p.skipped_cloud)} cloud-only file${p.skipped_cloud > 1 ? 's were' : ' was'} skipped so nothing gets downloaded.` : '';
    status.textContent = p.cancelled ? 'Stopped.' : `Finished: ${fmt.num(p.files)} files checked, ${fmt.num(p.errors)} protected folders skipped.${cloud}`;
    paintGroups();
  }
  function setBusy(b) { S.busy = b; onBusy?.(b); }

  async function start() {
    if (S.busy) return;
    const r = roots();
    if (!r.length) { ctx.toast('No partition to scan'); return; }
    S.groups = []; S.sel = new Set(); S.done = false; S.job = null; S.starting = true;
    setBusy(true); paintGroups();
    prog.classList.add('ind'); progFill.style.width = '0%'; status.textContent = 'Starting…';
    try {
      const { job } = await api.invoke('dupes_scan_start', { roots: r, min_size: Number(sizeSel.value) * MB });
      if (S.job == null) S.job = job;
    } catch (e) { S.starting = false; ctx.toast(e.message, 'bad'); S.job = '__none'; onDone({ job: '__none', cancelled: true, groups: [] }); status.textContent = e.message; }
  }
  async function stop() {
    if (!S.busy || !S.job) return;
    try { await api.invoke('storage_scan_cancel', { job: S.job }); } catch (e) { ctx.toast(e.message, 'bad'); }
  }

  /* ---------------- delete ---------------- */
  async function recycle() {
    const payload = [];
    for (const g of S.groups) {
      const rm = g.files.filter((f) => S.sel.has(f.path)), keep = g.files.find((f) => !S.sel.has(f.path));
      if (!rm.length) continue;
      if (!keep) { ctx.toast('Keep at least one copy of each file', 'bad'); return; }
      payload.push({ size: g.size, keep: { path: keep.path, mtime: keep.mtime }, remove: rm.map((f) => ({ path: f.path, mtime: f.mtime })) });
    }
    if (!payload.length) return;
    const count = payload.reduce((s, g) => s + g.remove.length, 0), bytes = payload.reduce((s, g) => s + g.size * g.remove.length, 0);
    const yes = await ui.confirmDialog({ title: 'Move duplicates to the Recycle Bin', danger: true, confirmLabel: 'Move to Recycle Bin',
      text: `Move ${count} duplicate file${count > 1 ? 's' : ''} (${fmt.bytes(bytes)}) to the Recycle Bin? One identical copy of each stays where it is. Files too large for the Recycle Bin are not removed.` });
    if (!yes) return;
    S.working = true; paintGroups();
    try {
      const { results } = await api.invoke('dupes_recycle', { groups: payload });
      const gone = new Set(results.filter((r) => r.ok).map((r) => r.path));
      const freed = payload.reduce((s, g) => s + g.size * g.remove.filter((f) => gone.has(f.path)).length, 0);
      for (const g of S.groups) { const left = g.files.filter((f) => !gone.has(f.path)); g.count -= g.files.length - left.length; g.files = left; }
      const before = S.groups.length;
      S.groups = S.groups.filter((g) => g.files.length > 1);
      S.removedGroups = (S.removedGroups || 0) + (before - S.groups.length);
      S.sel = new Set([...S.sel].filter((p) => !gone.has(p)));
      const bad = results.find((r) => !r.ok);
      ctx.toast(`Moved ${gone.size} of ${results.length} file${results.length > 1 ? 's' : ''} - ${fmt.bytes(freed)} freed${bad ? ` (${bad.message})` : ''}`, bad ? 'bad' : 'ok');
    } catch (e) { ctx.toast(e.message, 'bad'); }
    S.working = false; paintGroups();
  }

  api.listen('dupes:progress', onProgress).then((u) => un.push(u));
  api.listen('dupes:done', onDone).then((u) => un.push(u));
  paintGroups();
  return {
    el, start, stop,
    get busy() { return S.busy; },
    get job() { return S.job; },
    destroy() { un.forEach((u) => u?.()); if (S.busy && S.job) api.invoke('storage_scan_cancel', { job: S.job }).catch(() => {}); },
  };
}
