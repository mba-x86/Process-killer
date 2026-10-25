const rows = (T) => T.$$('#page tbody tr.click');
const rowOf = (T, name) => T.$$('#page tbody tr.click').find((tr) => T.text(tr.children[1]) === name);
const sub = (T) => T.text('#page header p');
const search = async (T, v) => { await T.type('.search input', v); await T.sleep(250); };
const pickRow = (T, name, o) => T.click(rowOf(T, name), o);
async function ready(T, q = '') { await T.open('services', q); await T.waitFor(() => rows(T).length > 100, { msg: 'service rows' }); }
const act = (T, label) => T.byText(label, { root: T.$('.actions'), tags: 'button' });
const dis = (T, label) => act(T, label).disabled;

export default [
  { name: 'renders capped list, search + status filter', async run(T) {
    await ready(T);
    await T.step('cap', async () => { T.eq(rows(T).length, 300); T.has(sub(T), '308 services'); T.has(sub(T), 'showing first 300'); });
    await T.step('search', async () => {
      await search(T, 'brave'); T.eq(rows(T).length, 2); T.has(sub(T), '2 of 308');
      await search(T, 'spooler'); T.eq(rows(T).length, 1);
      await search(T, 'zzz-none'); T.has(T.text('#page tbody'), 'No services match');
      await search(T, '');
    });
    await T.step('status filter', async () => {
      await T.select(T.$('select.select'), 'running');
      const r = rows(T); T.assert(r.length > 5, 'some running'); T.assert(r.every((tr) => T.text(tr.children[2]) === 'Running'), 'all rows running');
      await T.select(T.$('select.select'), 'stopped');
      T.assert(rows(T).every((tr) => T.text(tr.children[2]) === 'Stopped'), 'all rows stopped');
    });
    await T.step('sort by status header', async () => {
      await T.click(T.$$('#page thead th')[2]); T.has(T.text('#page thead'), 'Status ↑');
    });
  } },

  { name: 'selection: buttons disabled until selected; ctrl/shift multi-select', async run(T) {
    await ready(T);
    await T.step('no selection -> all action buttons disabled', async () => {
      for (const l of ['Start', 'Stop', 'Restart', 'Enable (automatic)', 'Manual', 'Disable']) T.assert(dis(T, l), `${l} should be disabled`);
    });
    await T.step('shift range + ctrl toggle', async () => {
      const r = rows(T);
      await T.click(r[0]); await T.click(r[3], { shift: true });
      T.eq(T.count('#page tbody tr.sel'), 4, 'shift range'); T.assert(!dis(T, 'Disable'), 'Disable enabled');
      await T.click(r[1], { ctrl: true }); T.eq(T.count('#page tbody tr.sel'), 3, 'ctrl toggles off');
      await T.click(r[10], { ctrl: true }); T.eq(T.count('#page tbody tr.sel'), 4);
      await T.click(r[5]); T.eq(T.count('#page tbody tr.sel'), 1, 'plain click replaces the selection');
    });
    await T.step('Start/Stop enablement follows status', async () => {
      await search(T, 'spooler'); await pickRow(T, 'Spooler');
      T.assert(dis(T, 'Start'), 'Start disabled for a running service'); T.assert(!dis(T, 'Stop'), 'Stop enabled');
    });
    await T.step('filtering away the selection disables buttons again', async () => {
      await search(T, 'brave'); T.eq(T.count('#page tbody tr.sel'), 0); T.assert(dis(T, 'Disable'), 'Disable disabled after selection vanished');
    });
  } },

  { name: 'Stop on a Windows service: confirm dialog, cancel does nothing, confirm stops', async run(T) {
    await ready(T);
    await search(T, 'appinfo'); await pickRow(T, 'Appinfo');
    const m = T.mark();
    await T.click(act(T, 'Stop'));
    await T.step('dialog', async () => {
      await T.waitFor(() => T.modal(), { msg: 'confirm dialog' });
      T.has(T.text('.modal-head h2'), 'Windows service'); T.has(T.modalText(), 'belong to Windows');
    });
    await T.step('Cancel does nothing', async () => {
      await T.clickText('Cancel'); T.assert(!T.modal(), 'dialog closed');
      T.eq(T.calls('set_service_state', m).length, 0, 'no backend call'); T.has(T.text(rowOf(T, 'Appinfo')), 'Running');
    });
    await T.step('Escape = cancel', async () => {
      await T.click(act(T, 'Stop')); await T.waitFor(() => T.modal());
      await T.key('Escape'); T.assert(!T.modal(), 'Escape closes'); T.eq(T.calls('set_service_state', m).length, 0);
    });
    await T.step('confirming stops it', async () => {
      await T.click(act(T, 'Stop')); await T.waitFor(() => T.modal());
      await T.clickText('Stop anyway');
      await T.waitToast(/stopped/i);
      await T.waitFor(() => T.text(rowOf(T, 'Appinfo')).includes('Stopped'), { msg: 'status becomes Stopped' });
      const c = T.calls('set_service_state', m); T.eq(c.length, 1); T.eq(c[0].args.action, 'stop'); T.eq(c[0].args.names[0], 'Appinfo');
    });
    await T.step('backend failure is shown', async () => {
      await search(T, 'spooler'); await pickRow(T, 'Spooler');
      await T.click(act(T, 'Stop')); await T.waitFor(() => T.modal()); await T.clickText('Stop anyway');
      await T.waitToast(/Access is denied/);
    });
  } },

  { name: 'non-system multi disable asks once; Start/Manual run without dialog', async run(T) {
    await ready(T);
    await search(T, 'brave');
    await T.click(rows(T)[0]); await T.click(rows(T)[1], { ctrl: true });
    const m = T.mark();
    await T.click(act(T, 'Disable'));
    await T.waitFor(() => T.modal()); T.has(T.modalText(), 'Stop and disable 2 services?');
    await T.clickText('Cancel'); T.eq(T.calls('set_service_state', m).length, 0);
    await T.click(act(T, 'Disable')); await T.waitFor(() => T.modal()); await T.click({ text: 'Disable' }, { root: T.$('.modal-foot') });
    await T.waitFor(() => T.calls('set_service_state', m).length === 1, { msg: 'disable call' });
    await T.waitFor(() => rows(T).every((tr) => T.text(tr.children[3]) === 'Disabled'), { msg: 'startup shows Disabled' });
    await T.click(rows(T)[0]);
    await T.click(act(T, 'Start'));
    await T.waitFor(() => T.text(rows(T)[0].children[2]).includes('Running'), { msg: 'running after start' });
    T.assert(!T.modal(), 'no dialog for Start');
    await T.click(act(T, 'Manual'));
    await T.waitFor(() => T.text(rows(T)[0].children[3]) === 'Manual', { msg: 'manual' });
  } },

  { name: 'verified: Start shows what Windows reports (confirmed toast, verified tag, row highlight that fades)', async run(T) {
    await ready(T);
    await search(T, 'ALG'); await pickRow(T, 'ALG');
    await T.click(act(T, 'Start'));
    await T.waitToast(/confirmed, now running/i);
    await T.waitFor(() => T.text(rowOf(T, 'ALG')).includes('Running') && T.text(rowOf(T, 'ALG')).includes('verified'), { msg: 'Running + verified tag' });
    T.assert(rowOf(T, 'ALG').classList.contains('fx-ok'), 'row highlighted');
    await T.waitFor(() => !T.text(rowOf(T, 'ALG')).includes('verified'), { msg: 'tag fades again', timeout: 8000 });
    T.assert(!rowOf(T, 'ALG').classList.contains('fx-ok'), 'highlight removed');
  } },

  { name: 'not verified: call "succeeded" but Windows still reports the old state -> shown as a failure', async run(T) {
    await ready(T);
    await search(T, 'wuauserv'); await pickRow(T, 'wuauserv');
    await T.click(act(T, 'Disable')); await T.waitFor(() => T.modal()); await T.clickText('Disable anyway');
    await T.waitToast(/still reports it as stopped \/ manual/);
    await T.waitFor(() => rowOf(T, 'wuauserv').classList.contains('fx-bad'), { msg: 'row marked as failed' });
    T.has(T.text(rowOf(T, 'wuauserv').children[3]), 'Manual');
    T.assert(!T.text(rowOf(T, 'wuauserv')).includes('verified'), 'no verified tag');
  } },

  { name: 'slow service: shows Starting…, polls the real state, settles as verified', async run(T) {
    await ready(T);
    await search(T, 'BITS'); await pickRow(T, 'BITS');
    const m = T.mark();
    await T.click(act(T, 'Start'));
    await T.waitFor(() => T.text(rowOf(T, 'BITS')).includes('Starting'), { msg: 'Starting… spinner' });
    await T.waitToast(/still working/i);
    await T.waitFor(() => T.calls('service_status', m).length >= 1, { msg: 'polls service_status', timeout: 8000 });
    await T.waitFor(() => T.text(rowOf(T, 'BITS')).includes('Running') && T.text(rowOf(T, 'BITS')).includes('verified'), { msg: 'settles as Running + verified', timeout: 12000 });
  } },
];
