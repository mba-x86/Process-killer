// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Storage analyzer. API: docs/api/storage.md
import { createDupes } from './storage/dupes.js';
const MB = 1048576;
const TYPES = {
  Videos: 'mp4 mkv avi mov wmv flv webm m4v mpg mpeg ts', Audio: 'mp3 wav flac aac ogg m4a wma',
  Images: 'jpg jpeg png gif bmp tif tiff psd heic webp raw cr2 svg', Archives: 'zip rar 7z tar gz bz2 xz cab',
  'Disk images': 'iso img vhd vhdx vmdk wim esd bin dmg', Documents: 'pdf doc docx xls xlsx ppt pptx txt csv log', Programs: 'exe msi dll sys apk',
};
const EXT = {};
for (const [k, v] of Object.entries(TYPES)) v.split(' ').forEach((e) => { EXT[e] = k; });
const SIZES = [['1', '≥ 1 MB'], ['10', '≥ 10 MB'], ['100', '≥ 100 MB'], ['500', '≥ 500 MB'], ['1024', '≥ 1 GB']];
const ROOT_FILES = '  (files in this folder)';
const MAX_ROWS = 300, REFRESH_MS = 1500;
const UNDELETABLE = new Set(['pagefile.sys', 'hiberfil.sys', 'swapfile.sys']);

const decorate = (f) => {   // cache derived fields once per item
  const i = f.path.lastIndexOf('\\');
  f.name = f.path.slice(i + 1); f.dir = i > 2 ? f.path.slice(0, i) : f.path.slice(0, i + 1);
  const d = f.name.lastIndexOf('.');
  const ext = d > 0 ? f.name.slice(d + 1).toLowerCase() : '';
  f.type = UNDELETABLE.has(f.name.toLowerCase()) ? 'System' : EXT[ext] || 'Other';
  f.low = f.path.toLowerCase();
  f.prot = UNDELETABLE.has(f.name.toLowerCase()) || /^[a-z]:\\windows\\/i.test(f.path);
  return f;
};

export default async function (ctx) {
  const { h, ui, fmt, api, icon } = ctx;
  ctx.css('css/pages/storage.css');
  const setT = (el, t) => { if (el.textContent !== t) el.textContent = t; };

  const S = { drives: [], key: 'ALL', custom: null, job: null, starting: false, scanning: false, view: 'files',
    files: [], folders: [], stats: null, sel: new Set(), sortF: ['size', -1], sortD: ['size', -1], last: 0, timer: 0, dirty: true };
  const cardEls = new Map();

  /* ---------------- partition cards + toolbar ---------------- */
  const cardsHost = h('div', { class: 'st-drives', role: 'group', 'aria-label': 'Partitions' });
  const partSel = ui.select({ options: [['ALL', 'All partitions']], value: 'ALL', onChange: (v) => selectScope(v) });
  partSel.classList.add('field'); partSel.style.width = '240px';
  const btnScan = ui.button({ label: 'Scan', icon: 'play', kind: 'primary', onClick: () => (S.scanning ? stopScan() : startScan()) });
  const btnChoose = ui.button({ label: 'Choose folder…', icon: 'folder', onClick: chooseFolder });
  const btnUp = ui.button({ label: 'Up', icon: 'up', onClick: goUp }); btnUp.disabled = true;
  const scopeL = h('span', { class: 'muted', style: { marginLeft: '6px' } }, 'Scope: all partitions');
  const tools = h('div', { class: 'st-tools' }, h('span', { class: 'lbl-t' }, 'Partition'), partSel, btnScan, btnChoose, btnUp, scopeL);

  const typeSel = ui.select({ options: [['all', 'All types'], ...Object.keys(TYPES).map((t) => [t, t]), ['Other', 'Other']], value: 'all', onChange: () => refresh(true) });
  typeSel.classList.add('field'); typeSel.style.width = '130px'; typeSel.setAttribute('aria-label', 'Type');
  const sizeSel = ui.select({ options: SIZES, value: '10', onChange: () => refresh(true) });
  sizeSel.classList.add('field'); sizeSel.style.width = '110px'; sizeSel.setAttribute('aria-label', 'Minimum size');
  let qTimer;
  const search = h('input', { class: 'field', type: 'search', placeholder: 'Filter by name or path…', style: { width: '230px' }, 'aria-label': 'Filter',
    oninput: () => { clearTimeout(qTimer); qTimer = setTimeout(() => refresh(true), 150); } });
  const seg = ui.segmented({ options: [['files', 'Files'], ['folders', 'Folders'], ['dupes', 'Duplicates']], value: 'files', onChange: setView });
  const filterTools = h('div', { class: 'st-tools' }, typeSel, sizeSel, search);
  const filters = h('div', { class: 'st-filters' }, filterTools, h('div', { class: 'st-seg' }, seg));

  /* ---------------- progress + status ---------------- */
  const progFill = h('i'), prog = h('div', { class: 'st-bar st-bar-prog' }, progFill);
  const stCount = h('span'), stCur = h('span', { class: 'mono' });
  const status = h('span', { class: 'st-status' }, 'Pick a partition and press Scan.');
  const progBox = h('div', { class: 'col' }, prog, status);

  /* ---------------- tables ---------------- */
  const thead = h('thead'), tbody = h('tbody');
  const table = h('table', { class: 'tbl' }, thead, tbody);
  const tableCard = ui.card({ class: 'st-table' }, table);
  const FCOLS = [['size', 'Size', '22%'], ['name', 'Name'], ['dir', 'Folder'], ['type', 'Type'], ['mod', 'Modified']];
  const DCOLS = [['path', 'Folder'], ['size', 'Size', '120px'], ['share', 'Share of scan', '38%']];
  const sortOf = () => (S.view === 'files' ? S.sortF : S.sortD);
  function paintHead() {
    const cols = S.view === 'files' ? FCOLS : DCOLS, [k, d] = sortOf();
    thead.replaceChildren(h('tr', {}, cols.map(([key, label, w]) => h('th', { class: 'sortable', style: w ? { width: w } : null,
      onclick: () => { const s = sortOf(); if (s[0] === key) s[1] = -s[1]; else { s[0] = key; s[1] = key === 'size' || key === 'mod' || key === 'share' ? -1 : 1; } refresh(true); } },
    label, k === key ? ` ${d > 0 ? '↑' : '↓'}` : ''))));
  }
  const lock = () => h('svg', { width: 13, height: 13, viewBox: '0 0 16 16', fill: 'none', stroke: 'currentColor', 'stroke-width': 1.7, role: 'img', 'aria-label': 'Protected system file' },
    h('rect', { x: 3.5, y: 7, width: 9, height: 6.5, rx: 1.2 }), h('path', { d: 'M5.5 7V5a2.5 2.5 0 0 1 5 0v2' }));
  const cmp = (a, b, k) => (typeof a[k] === 'string' ? a[k].localeCompare(b[k], undefined, { sensitivity: 'base' }) : (a[k] > b[k]) - (a[k] < b[k]));

  let shown = [], rowEls = new Map(), anchor = null;
  const note = h('span', { class: 'note' });
  function filtered() {
    const min = Number(sizeSel.value) * MB, type = typeSel.value, q = search.value.trim().toLowerCase();
    return S.files.filter((f) => f.size >= min && (type === 'all' || f.type === type || (type === 'Other' && f.type === 'System')) && (!q || f.low.includes(q)));
  }
  function renderFiles() {
    const all = filtered(), [k, d] = S.sortF;
    all.sort((a, b) => d * cmp(a, b, k === 'mod' ? 'mtime' : k) || b.size - a.size);
    shown = all.slice(0, MAX_ROWS);
    const max = Math.max(1, ...shown.map((f) => f.size)), frag = document.createDocumentFragment();
    rowEls = new Map();
    for (const f of shown) {
      const tr = h('tr', { class: [S.sel.has(f.path) && 'sel'], onclick: (e) => pick(f, e), ondblclick: () => openLoc(f.path) },
        h('td', {}, h('div', { class: 'st-sizecell' }, h('span', { class: 'n' }, fmt.bytes(f.size)), h('div', { class: 'b' }, h('i', { style: { width: `${(f.size / max) * 100}%` } })))),
        h('td', { title: f.path }, h('span', { class: 'st-name' }, f.prot ? lock() : null, f.name)),
        h('td', { class: 'mono dim', title: f.dir }, f.dir), h('td', { class: 'dim' }, f.type), h('td', { class: 'dim' }, fmt.date(f.mtime)));
      rowEls.set(f.path, tr); frag.append(tr);
    }
    tbody.replaceChildren(frag.childNodes.length ? frag : h('tr', {}, h('td', { colspan: 5, class: 'dim', style: { textAlign: 'center', height: '80px' } }, S.stats ? 'No files match the filters' : 'Run a scan to list the biggest files')));
    setT(note, all.length > MAX_ROWS ? `Showing ${MAX_ROWS} of ${fmt.num(all.length)} files` : `${fmt.num(all.length)} files`);
  }
  function renderFolders() {
    const q = search.value.trim().toLowerCase(), total = Math.max(S.stats?.bytes || 0, 1), [k, d] = S.sortD;
    const all = S.folders.filter((f) => !q || f.path.toLowerCase().includes(q)).map((f) => ({ ...f, share: f.size / total }));
    all.sort((a, b) => d * cmp(a, b, k === 'share' ? 'size' : k) || b.size - a.size);
    shown = all.slice(0, MAX_ROWS);
    const max = Math.max(1, ...shown.map((f) => f.size)), frag = document.createDocumentFragment();
    rowEls = new Map();
    for (const f of shown) {
      const tr = h('tr', { class: [S.sel.has(f.path) && 'sel'], onclick: (e) => pick(f, e), ondblclick: () => drill(f) },
        h('td', { class: 'mono', title: f.path }, f.path), h('td', { class: 'tnum' }, fmt.bytes(f.size)),
        h('td', {}, h('div', { class: 'st-sizecell' }, h('div', { class: 'b' }, h('i', { style: { width: `${(f.size / max) * 100}%` } })), h('span', { class: 'dim tnum', style: { width: '46px' } }, `${(f.share * 100).toFixed(1)}%`))));
      rowEls.set(f.path, tr); frag.append(tr);
    }
    tbody.replaceChildren(frag.childNodes.length ? frag : h('tr', {}, h('td', { colspan: 3, class: 'dim', style: { textAlign: 'center', height: '80px' } }, 'Run a scan to see folder sizes')));
    setT(note, all.length > MAX_ROWS ? `Showing ${MAX_ROWS} of ${fmt.num(all.length)} folders` : `${fmt.num(all.length)} folders`);
  }
  function refresh(now) {
    const t = Date.now();
    if (!now && t - S.last < REFRESH_MS) { if (!S.timer) S.timer = setTimeout(() => { S.timer = 0; refresh(true); }, REFRESH_MS - (t - S.last)); return; }
    clearTimeout(S.timer); S.timer = 0; S.last = t;
    paintHead(); (S.view === 'files' ? renderFiles : renderFolders)();
    syncActs();
  }
  ctx.onCleanup(() => { clearTimeout(S.timer); clearTimeout(qTimer); });

  function pick(row, e) {
    const key = row.path;
    if (e.shiftKey && anchor && rowEls.has(anchor)) {
      const a = shown.findIndex((x) => x.path === anchor), b = shown.findIndex((x) => x.path === key);
      S.sel = new Set(shown.slice(Math.min(a, b), Math.max(a, b) + 1).map((x) => x.path));
    } else if (e.ctrlKey || e.metaKey) { S.sel.has(key) ? S.sel.delete(key) : S.sel.add(key); anchor = key; }
    else { S.sel = new Set([key]); anchor = key; }
    rowEls.forEach((tr, p) => tr.classList.toggle('sel', S.sel.has(p)));
    syncActs();
  }

  /* ---------------- actions ---------------- */
  const selFiles = () => S.files.filter((f) => S.sel.has(f.path));
  const bOpen = ui.button({ label: 'Open location', icon: 'folder', onClick: () => { const p = [...S.sel][0]; if (p) openLoc(p); } });
  const bCopy = ui.button({ label: 'Copy path', icon: 'copy', onClick: copyPath });
  const bDel = ui.button({ label: 'Move to Recycle Bin', icon: 'trash', kind: 'danger', onClick: recycle });
  const bDrill = ui.button({ label: 'Scan inside this folder', icon: 'search', kind: 'primary', onClick: () => { const p = [...S.sel][0]; if (p) drill({ path: p }); } });
  const bOpenF = ui.button({ label: 'Open folder', icon: 'folder', onClick: () => { const p = [...S.sel][0]; if (p) openLoc(p.replace(ROOT_FILES, '')); } });
  const acts = h('div', { class: 'st-acts' });
  function syncActs() {
    const n = S.sel.size;
    if (acts.dataset.v !== S.view) { acts.dataset.v = S.view; acts.replaceChildren(...(S.view === 'files' ? [bOpen, bCopy, bDel] : [bDrill, bOpenF, bCopy]), note); }
    bOpen.disabled = bCopy.disabled = bDel.disabled = bDrill.disabled = bOpenF.disabled = !n;
    bDrill.disabled = !n || S.scanning || [...S.sel][0]?.endsWith(ROOT_FILES);
  }
  function setView(v) {
    S.view = v; S.sel = new Set(); anchor = null;
    const d = v === 'dupes';   // the duplicates tab has its own panel; the scope cards + Scan button above are shared
    dup.el.hidden = !d; filterTools.hidden = progBox.hidden = tableCard.hidden = acts.hidden = d;
    refresh(true);
  }
  async function openLoc(path) { try { await api.invoke('open_location', { path }); } catch (e) { ctx.toast(e.message, 'bad'); } }
  async function copyPath() {
    const t = [...S.sel].map((p) => p.replace(ROOT_FILES, '')).join('\n');
    try { await navigator.clipboard.writeText(t); } catch { try { await api.invoke('copy_text', { text: t }); } catch (e) { ctx.toast(e.message, 'bad'); return; } }
    ctx.toast(`Copied ${S.sel.size} path${S.sel.size > 1 ? 's' : ''}`);
  }
  async function recycle() {
    const all = selFiles();
    if (!all.length) { ctx.toast('Select one or more files first'); return; }
    const ok = all.filter((f) => !f.prot), skipped = all.length - ok.length;
    if (!ok.length) { ctx.toast('Protected Windows files cannot be deleted', 'bad'); return; }
    const total = ok.reduce((s, f) => s + f.size, 0);
    const yes = await ui.confirmDialog({ title: 'Move to Recycle Bin', danger: true, confirmLabel: 'Move to Recycle Bin',
      text: `Move ${ok.length} file${ok.length > 1 ? 's' : ''} (${fmt.bytes(total)}) to the Recycle Bin? Files larger than the Recycle Bin allows are deleted permanently by Windows.${skipped ? ` ${skipped} protected system file${skipped > 1 ? 's are' : ' is'} skipped.` : ''}` });
    if (!yes) return;
    try {
      const { results } = await api.invoke('recycle_files', { paths: ok.map((f) => f.path) });
      const gone = new Set(results.filter((r) => r.ok).map((r) => r.path));
      const freed = ok.filter((f) => gone.has(f.path)).reduce((s, f) => s + f.size, 0);
      S.files = S.files.filter((f) => !gone.has(f.path)); gone.forEach((p) => S.sel.delete(p));
      const bad = results.find((r) => !r.ok);
      ctx.toast(`Moved ${gone.size} of ${results.length} file${results.length > 1 ? 's' : ''} — ${fmt.bytes(freed)} freed${bad ? ` (${bad.message})` : ''}`, bad ? 'bad' : 'ok');
      refresh(true);
    } catch (e) { ctx.toast(e.message, 'bad'); }
  }

  /* ---------------- scope ---------------- */
  const GB_T = (n) => fmt.bytes(n);
  function buildCards() {
    const tot = S.drives.reduce((s, d) => s + d.total, 0) || 1, used = S.drives.reduce((s, d) => s + d.used, 0);
    const entries = [['ALL', h('b', {}, 'All partitions'), `${S.drives.length} drives · ${GB_T(tot - used)} free of ${GB_T(tot)}`, (100 * used) / tot, 'All partitions'],
      ...S.drives.map((d) => { const name = d.label || (d.removable ? 'Removable disk' : 'Local disk');
        return [d.mount, h('b', {}, d.mount, ' ', h('span', {}, name)), `${d.fstype} · ${GB_T(d.free)} free of ${GB_T(d.total)}`, d.percent, `${d.mount} ${name}`]; })];
    cardEls.clear();
    cardsHost.replaceChildren(...entries.map(([key, title, sub, pct]) => {
      const el = h('button', { class: 'st-drive', type: 'button', 'aria-pressed': 'false', onclick: () => selectScope(key) },
        title, h('small', {}, sub), h('span', { class: 'st-bar' }, h('i', { style: { width: `${pct}%`, background: fmt.usageColor(pct, 'var(--amber)') } })));
      cardEls.set(key, el); return el;
    }));
    partSel.replaceChildren(...entries.map(([key, , , , label]) => h('option', { value: key }, label)));
    if (!cardEls.has(S.key)) S.key = 'ALL';
    paintScope();
  }
  function paintScope() {
    cardEls.forEach((el, k) => el.setAttribute('aria-pressed', String(k === S.key && !S.custom)));
    partSel.value = S.key;
    setT(scopeL, S.custom ? `Folder: ${S.custom}` : S.key === 'ALL' ? 'Scope: all partitions' : `Scope: ${S.key}`);
    btnUp.disabled = S.scanning || !S.custom;
  }
  function selectScope(key) { if (S.scanning) { paintScope(); return; } S.key = key; S.custom = null; paintScope(); }
  async function chooseFolder() {
    if (S.scanning) return;
    try { const { path } = await api.invoke('choose_folder'); if (path) { S.custom = path; paintScope(); startScan(); } } catch (e) { ctx.toast(e.message, 'bad'); }
  }
  function goUp() {
    if (S.scanning || !S.custom) return;
    const p = S.custom.replace(/\\+$/, ''), i = p.lastIndexOf('\\');
    if (i < 0 || p.length <= 2 || i <= 2) { const drive = `${p.slice(0, 2).toUpperCase()}\\`; S.custom = null; S.key = cardEls.has(drive) ? drive : 'ALL'; } else S.custom = p.slice(0, i);
    paintScope(); startScan();
  }
  function drill(f) {
    if (S.scanning || f.path.endsWith(ROOT_FILES)) return;
    S.custom = f.path; paintScope(); startScan();
  }

  /* ---------------- scanning ---------------- */
  const cur = (p) => (p.length < 70 ? p : `…${p.slice(-68)}`);
  function paintStatus(p) {
    status.replaceChildren(stCount, ' · ', stCur);
    setT(stCount, `${fmt.num(p.files)} files · ${fmt.bytes(p.bytes)} scanned`); stCur.textContent = cur(p.current || '');
  }
  function onProgress(p) {
    if (S.job == null) { if (!S.starting) return; S.job = p.job; } else if (p.job !== S.job) return;
    S.stats = p;
    if (p.top_files) { S.files = p.top_files.map(decorate); } if (p.folders) S.folders = p.folders;
    if (p.total_bytes) { prog.classList.remove('ind'); progFill.style.width = `${Math.min(99, (100 * p.bytes) / p.total_bytes)}%`; } else prog.classList.add('ind');
    paintStatus(p);
    refresh(false);
  }
  function onDone(p) {
    if (p.job !== S.job) return;
    S.scanning = false; S.job = null; S.starting = false;
    prog.classList.remove('ind'); progFill.style.width = p.cancelled ? '0%' : '100%';
    btnScan.replaceChildren(icon('play', 15), 'Scan'); btnScan.className = 'btn btn-p';
    btnScan.disabled = false; btnChoose.disabled = false; paintScope();
    const st = S.stats || { files: 0, bytes: 0 };
    status.replaceChildren(`${p.cancelled ? 'Stopped' : 'Finished'}: ${fmt.num(st.files)} files, ${fmt.bytes(st.bytes)} analysed (${fmt.num(p.errors ?? st.errors ?? 0)} protected folders skipped)`);
    refresh(true);
  }
  const currentRoots = () => (S.custom ? [S.custom] : S.drives.filter((d) => S.key === 'ALL' || d.mount === S.key).map((d) => d.mount));
  /** Scan button + scope controls while a duplicate scan runs (the files scan sets these itself in startScan/onDone). */
  function dupBusy(on) {
    S.scanning = on;
    if (on) { btnScan.replaceChildren(icon('stop', 12), 'Stop'); btnScan.className = 'btn btn-danger'; btnScan.disabled = false; btnChoose.disabled = true; btnUp.disabled = true; }
    else { btnScan.replaceChildren(icon('play', 15), 'Scan'); btnScan.className = 'btn btn-p'; btnScan.disabled = false; btnChoose.disabled = false; paintScope(); }
  }
  const dup = createDupes({ ctx, roots: currentRoots, onBusy: dupBusy });
  ctx.onCleanup(() => dup.destroy());
  async function startScan() {
    if (S.scanning) return;
    if (S.view === 'dupes') return dup.start();
    const roots = currentRoots();
    if (!roots.length) { ctx.toast('No partition to scan'); return; }
    S.scanning = true; S.starting = true; S.job = null; S.files = []; S.folders = []; S.stats = null; S.sel = new Set();
    btnScan.replaceChildren(icon('stop', 12), 'Stop'); btnScan.className = 'btn btn-danger'; btnChoose.disabled = true; btnUp.disabled = true;
    prog.classList.add('ind'); progFill.style.width = '0%'; status.replaceChildren('Starting…');
    refresh(true);
    try {
      const { job } = await api.invoke('storage_scan_start', { roots, min_size: MB });
      if (S.job == null) S.job = job;
    } catch (e) { S.scanning = false; S.starting = false; ctx.toast(e.message, 'bad'); onDone({ job: S.job, cancelled: true, errors: 0 }); }
  }
  async function stopScan() {
    if (dup.busy) { btnScan.disabled = true; btnScan.replaceChildren('Stopping…'); return dup.stop(); }
    if (!S.scanning || !S.job) return;
    btnScan.disabled = true; btnScan.replaceChildren('Stopping…');
    try { await api.invoke('storage_scan_cancel', { job: S.job }); } catch (e) { ctx.toast(e.message, 'bad'); }
  }

  const un1 = await api.listen('storage:progress', onProgress), un2 = await api.listen('storage:done', onDone);
  ctx.onCleanup(() => { un1?.(); un2?.(); if (S.scanning && S.job) api.invoke('storage_scan_cancel', { job: S.job }).catch(() => {}); });

  try { S.drives = await api.invoke('get_drives'); } catch (e) { ctx.toast(e.message, 'bad'); }
  buildCards(); refresh(true);
  if (!api.inTauri && /autoscan/.test(location.search)) setTimeout(startScan, 100);   // test hook (browser/mock only)
  return [ui.pageHead({ title: 'Storage analyzer', sub: 'Find the heaviest files, folders and duplicate files on any partition' }),
    cardsHost, tools, filters, progBox, tableCard, acts, dup.el];
}
