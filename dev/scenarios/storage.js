const trs = (T) => T.$$('#page .st-table tbody tr');
const dataRows = (T) => trs(T).filter((tr) => tr.children.length > 1 && !tr.querySelector('td[colspan]'));
const status = (T) => T.text('.st-status');
const scanBtn = (T) => T.byText(/^(Scan|Stop|Stopping…)$/, { root: T.$('.st-tools'), tags: 'button' });
const act = (T, l) => T.byText(l, { root: T.$('.st-acts'), tags: 'button' });
async function ready(T, q = '') { await T.open('storage', q); await T.waitFor(() => T.count('.st-drive') === 3, { msg: '3 partition cards' }); }
async function scanDone(T) { await T.waitFor(() => /^Finished/.test(status(T)), { msg: 'scan to finish', timeout: 30000 }); }
const setType = (T, v) => T.select(T.$('select[aria-label="Type"]'), v);
const setMin = (T, v) => T.select(T.$('select[aria-label="Minimum size"]'), v);
const filter = async (T, v) => { await T.type('input[aria-label="Filter"]', v); await T.sleep(300); };

export default [
  { name: 'initial state: partition cards, hint, Scan', async run(T) {
    await ready(T);
    T.has(T.text('.st-drives'), 'All partitions'); T.has(T.text('.st-drives'), 'C:\\'); T.has(T.text('.st-drives'), 'E:\\');
    T.has(status(T), 'press Scan'); T.has(T.text('.st-table'), 'Run a scan');
    T.assert(act(T, 'Move to Recycle Bin').disabled, 'delete disabled without selection');
    T.eq(T.$('.st-drive[aria-pressed="true"]').textContent.includes('All partitions'), true, 'All partitions selected');
    await T.click(T.$$('.st-drive')[1]);
    T.has(T.text('.st-tools .muted'), 'Scope: C:\\');
  } },

  { name: 'scan runs: Stop label, progress, rows, Finished', async run(T) {
    await ready(T);
    const m = T.mark();
    await T.click(scanBtn(T));
    await T.step('running state', async () => {
      T.eq(T.text(scanBtn(T)), 'Stop'); T.assert(T.byText('Choose folder…').disabled, 'Choose folder disabled during scan');
      await T.waitFor(() => /files · .* scanned/.test(status(T)), { msg: 'progress text' });
      T.assert(T.count('.st-bar-prog') === 1, 'progress bar');
      const w = () => parseFloat(T.$('.st-bar-prog i').style.width || '0');
      await T.waitFor(() => w() > 0, { msg: 'progress fill > 0' });
    });
    await T.step('rows appear while scanning', async () => { await T.waitFor(() => dataRows(T).length > 3, { msg: 'rows', timeout: 8000 }); });
    await T.step('finishes', async () => {
      await scanDone(T);
      T.eq(T.text(scanBtn(T)), 'Scan'); T.assert(!scanBtn(T).disabled);
      T.eq(T.calls('storage_scan_start', m)[0].args.roots.length, 2, 'both partitions scanned');
      T.assert(dataRows(T).length > 10, 'rows after scan');
      const sizes = dataRows(T).slice(0, 4).map((tr) => tr.children[0].textContent); T.note(`top sizes: ${sizes}`);
    });
  } },

  { name: 'filters: type, min size, text; Files|Folders toggle', async run(T) {
    await ready(T, 'autoscan');
    await scanDone(T);
    await T.step('type filter', async () => {
      await setType(T, 'Videos');
      await T.waitFor(() => dataRows(T).length > 0 && dataRows(T).every((tr) => T.text(tr.children[3]) === 'Videos'), { msg: 'only Videos rows' });
      await setType(T, 'all');
    });
    await T.step('min size', async () => {
      const n10 = dataRows(T).length; await setMin(T, '1024');
      const rows = dataRows(T); T.assert(rows.length > 0 && rows.length < n10, `>=1GB rows ${rows.length} vs ${n10}`);
      T.assert(rows.every((tr) => /GB|TB/.test(tr.children[0].textContent)), 'all rows >= 1 GB'); await setMin(T, '10');
    });
    await T.step('text filter', async () => {
      await filter(T, '.iso'); const r = dataRows(T); T.assert(r.length > 0 && r.every((tr) => /\.iso/i.test(tr.children[1].textContent + tr.children[2].textContent)), 'iso only'); await filter(T, 'zzzz');
      T.has(T.text('.st-table'), 'No files match'); await filter(T, '');
    });
    await T.step('Folders view', async () => {
      await T.click({ text: 'Folders' }, { root: T.$('.st-seg') });
      await T.waitFor(() => /Share of scan/.test(T.text('.st-table thead')), { msg: 'folder headers' });
      T.assert(dataRows(T).length > 2, 'folder rows'); T.has(T.text('.st-table'), '%');
      T.assert(T.byText('Scan inside this folder', { tags: 'button' }), 'drill button');
      await T.click({ text: 'Files' }, { root: T.$('.st-seg') });
      await T.waitFor(() => /Name/.test(T.text('.st-table thead')), { msg: 'file headers back' });
      T.assert(dataRows(T).length > 10, 'file rows back');
    });
  } },

  { name: 'delete: protected files refused, cancel no-op, confirm removes', async run(T) {
    await ready(T, 'autoscan');
    await scanDone(T);
    await setMin(T, '1');
    await T.step('pagefile.sys is protected', async () => {
      await filter(T, 'pagefile'); await T.waitFor(() => dataRows(T).length === 1, { msg: 'pagefile row' });
      T.assert(dataRows(T)[0].querySelector('svg[aria-label="Protected system file"]'), 'lock icon');
      await T.click(dataRows(T)[0]);
      const m = T.mark(); await T.click(act(T, 'Move to Recycle Bin'));
      await T.waitToast('Protected Windows files cannot be deleted'); T.assert(!T.modal(), 'no confirm for protected'); T.eq(T.calls('recycle_files', m).length, 0);
    });
    await T.step('normal file: cancel, then confirm', async () => {
      await filter(T, 'trip-2025'); await T.waitFor(() => dataRows(T).length === 1, { msg: 'trip row' });
      await T.click(dataRows(T)[0]);
      const m = T.mark(); await T.click(act(T, 'Move to Recycle Bin'));
      await T.waitFor(() => T.modal()); T.has(T.modalText(), 'Move 1 file'); await T.clickText('Cancel'); T.eq(T.calls('recycle_files', m).length, 0);
      await T.click(act(T, 'Move to Recycle Bin')); await T.waitFor(() => T.modal()); await T.key('Escape'); T.eq(T.calls('recycle_files', m).length, 0, 'Escape cancels');
      await T.click(act(T, 'Move to Recycle Bin')); await T.waitFor(() => T.modal()); await T.click({ text: 'Move to Recycle Bin' }, { root: T.$('.modal-foot') });
      await T.waitToast(/Moved 1 of 1 file/);
      await T.waitFor(() => dataRows(T).length === 0, { msg: 'row gone' }); T.assert(act(T, 'Move to Recycle Bin').disabled, 'selection cleared');
    });
    await T.step('mixed: protected skipped, normal sent', async () => {
      await filter(T, 'C:\\');
      const r = dataRows(T).filter((tr) => /pagefile|stream-vod/.test(tr.textContent));
      T.eq(r.length, 2, 'rows'); await T.click(r[0]); await T.click(r[1], { ctrl: true });
      const m = T.mark(); await T.click(act(T, 'Move to Recycle Bin'));
      await T.waitFor(() => T.modal()); T.has(T.modalText(), '1 protected system file is skipped');
      await T.click({ text: 'Move to Recycle Bin' }, { root: T.$('.modal-foot') });
      await T.waitFor(() => T.calls('recycle_files', m).length === 1, { msg: 'recycle call' });
      T.eq(T.calls('recycle_files', m)[0].args.paths.length, 1); T.has(T.calls('recycle_files', m)[0].args.paths[0], 'stream-vod');
    });
  } },

  { name: 'Stop cancels the job; leaving the page mid-scan cancels and unlistens', async run(T) {
    await ready(T);
    let m = T.mark();
    await T.click(scanBtn(T)); await T.waitFor(() => /files · /.test(status(T)), { timeout: 5000 });
    await T.click(scanBtn(T));
    await T.waitFor(() => /^Stopped/.test(status(T)), { msg: 'Stopped status', timeout: 10000 });
    T.eq(T.calls('storage_scan_cancel', m).length, 1); T.eq(T.text(scanBtn(T)), 'Scan');
    m = T.mark();
    await T.click(scanBtn(T)); await T.waitFor(() => /files · /.test(status(T)), { timeout: 5000 });
    await T.nav('dashboard');
    await T.waitFor(() => T.calls('storage_scan_cancel', m).length === 1, { msg: 'cancel when leaving' });
    T.eq(T.listeners('storage:progress'), 0, 'storage:progress listener leaked'); T.eq(T.listeners('storage:done'), 0, 'storage:done listener leaked');
    await T.sleep(3000);
  } },

  { name: 'Choose folder -> scoped scan; Up; drill into a folder from Folders view', async run(T) {
    await ready(T);
    const m = T.mark();
    await T.clickText('Choose folder…');
    await T.waitFor(() => T.calls('storage_scan_start', m).length === 1, { msg: 'scan starts' });
    T.eq(JSON.stringify(T.calls('storage_scan_start', m)[0].args.roots), JSON.stringify(['C:\\Users\\mba\\Videos']));
    T.has(T.text('.st-tools .muted'), 'Folder: C:\\Users\\mba\\Videos');
    await scanDone(T);
    T.assert(!T.byText('Up', { root: T.$('.st-tools'), tags: 'button' }).disabled, 'Up enabled after scan');
    await T.clickText('Up', { root: T.$('.st-tools') });
    await T.waitFor(() => T.calls('storage_scan_start', m).length === 2, { msg: 'second scan' });
    T.eq(T.calls('storage_scan_start', m)[1].args.roots[0], 'C:\\Users\\mba');
    await scanDone(T);
    await T.click({ text: 'Folders' }, { root: T.$('.st-seg') });
    await T.waitFor(() => dataRows(T).length > 0);
    await T.click(dataRows(T)[0], { dbl: true });
    await T.waitFor(() => T.calls('storage_scan_start', m).length === 3, { msg: 'drill scan' });
    await scanDone(T);
  } },

  { name: 'duplicates: own tab + scan, keep-one rule, auto-select, delete payload keeps a copy', async run(T) {
    await ready(T);
    const groups = () => T.$$('.dp .dp-group');
    const dpBtn = (re) => T.byText(re, { root: T.$('.dp'), tags: 'button' });
    await T.step('tab swaps the panel', async () => {
      await T.click({ text: 'Duplicates' }, { root: T.$('.st-seg') });
      T.assert(T.$('.st-table').hidden && !T.$('.dp').hidden, 'table hidden, duplicates panel shown');
      T.has(T.text('.dp .st-status'), 'press Scan');
    });
    await T.step('scan runs and finds groups', async () => {
      await T.click(scanBtn(T)); T.eq(T.text(scanBtn(T)), 'Stop');
      await T.waitFor(() => /Looking for files of the same size/.test(T.text('.dp .st-status')), { msg: 'walk phase', timeout: 8000 });
      await T.waitFor(() => /^Finished/.test(T.text('.dp .st-status')), { msg: 'finished', timeout: 30000 });
      T.eq(T.text(scanBtn(T)), 'Scan'); T.assert(groups().length > 3, 'groups rendered');
      T.has(T.text('.dp .dp-sum'), 'can be freed'); T.has(T.text('.dp .st-status'), 'cloud-only');
      T.eq(T.$$('.dp .dp-tag.rm').length, 0, 'nothing selected yet');
    });
    await T.step('cannot tick every copy of a file', async () => {
      const n = T.$$('input.chk', groups()[0]).length;
      for (let i = 0; i < n; i++) await T.click(T.$$('input.chk', groups()[0])[i]);
      await T.waitToast(/Keep at least one copy/);
      T.eq(T.$$('.dp-tag.keep', groups()[0]).length, 1, 'one copy still marked Keep');
      await T.click(dpBtn('Clear selection'));
      T.eq(T.$$('.dp .dp-tag.rm').length, 0, 'selection cleared');
    });
    await T.step('auto-select keeps one per group', async () => {
      await T.click(dpBtn('Select the extra copies'));
      T.assert(groups().every((g) => T.$$('.dp-tag.keep', g).length === 1), 'exactly one Keep per visible group');
      T.assert(/Move \d+ files? to Recycle Bin/.test(T.text(dpBtn(/Move \d+/))), 'delete button counts the files');
    });
    await T.step('delete: cancel is a no-op, confirm sends keep + remove per group', async () => {
      const m = T.mark();
      await T.click(dpBtn(/Move \d+/)); await T.waitFor(() => T.modal()); T.has(T.modalText(), 'One identical copy of each');
      await T.clickText('Cancel'); T.eq(T.calls('dupes_recycle', m).length, 0, 'cancel = no call');
      await T.click(dpBtn(/Move \d+/)); await T.waitFor(() => T.modal()); await T.click({ text: 'Move to Recycle Bin' }, { root: T.$('.modal-foot') });
      await T.waitFor(() => T.calls('dupes_recycle', m).length === 1, { msg: 'dupes_recycle call' });
      const g = T.calls('dupes_recycle', m)[0].args.groups;
      T.assert(g.length > 3 && g.every((x) => x.keep && x.keep.path && x.remove.length >= 1 && x.size > 0), 'every group has a keep + removals');
      T.assert(g.every((x) => !x.remove.some((r) => r.path.toLowerCase() === x.keep.path.toLowerCase())), 'keep is never in remove');
      await T.waitToast(/Moved \d+ of \d+ files/);
      await T.waitFor(() => /No duplicates found/.test(T.text('.dp .dp-sum')), { msg: 'list empties', timeout: 8000 });
    });
  } },

  { name: 'duplicates: Stop cancels the scan; switching back to Files restores the table', async run(T) {
    await ready(T);
    await T.click({ text: 'Duplicates' }, { root: T.$('.st-seg') });
    await T.click(scanBtn(T));
    await T.waitFor(() => /Looking for files|Comparing|Reading/.test(T.text('.dp .st-status')), { msg: 'scan started', timeout: 8000 });
    const m = T.mark();
    await T.click(scanBtn(T));
    await T.waitFor(() => T.calls('storage_scan_cancel', m).length === 1, { msg: 'cancel call' });
    await T.waitFor(() => T.text('.dp .st-status') === 'Stopped.', { msg: 'Stopped', timeout: 8000 });
    T.eq(T.text(scanBtn(T)), 'Scan'); T.eq(T.$$('.dp .dp-group').length, 0, 'no partial results');
    await T.click({ text: 'Files' }, { root: T.$('.st-seg') });
    T.assert(!T.$('.st-table').hidden && T.$('.dp').hidden, 'files table back, duplicates panel hidden');
  } },
];
