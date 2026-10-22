const rows = (T) => T.$$('#page tbody tr.click');
const search = async (T, v) => { await T.type('.search input', v); await T.sleep(250); };
const byName = (T, n) => rows(T).filter((tr) => T.text(tr.children[0]) === n);
async function ready(T) { await T.open('processes'); await T.waitFor(() => rows(T).length > 100, { msg: 'process rows' }); }
const btn = (T, l) => T.byText(l, { root: T.$('.actions'), tags: 'button' });

export default [
  { name: 'renders, searches by name/pid, sorts', async run(T) {
    await ready(T);
    T.has(T.text('#page header p'), 'processes');
    await T.step('search', async () => {
      await search(T, 'code.exe'); T.eq(rows(T).length, 2); await search(T, '12880'); T.eq(rows(T).length, 1, 'by pid'); await search(T, ''); T.assert(rows(T).length > 250, 'all rows back');
    });
    await T.step('sort', async () => {
      await T.click(T.$$('#page thead th')[1]); T.has(T.text('#page thead'), 'PID ↓');
      const pids = rows(T).slice(0, 5).map((tr) => Number(T.text(tr.children[1]))); T.assert(pids.every((p, i) => i === 0 || pids[i - 1] >= p), `PID desc order ${pids}`);
    });
    await T.step('buttons disabled with no selection', async () => { T.assert(btn(T, 'End task').disabled && btn(T, 'End process tree').disabled && btn(T, 'Open file location').disabled); });
  } },

  { name: 'protected process cannot be ended (no dialog, no backend call)', async run(T) {
    await ready(T);
    await search(T, 'explorer.exe');
    const tr = rows(T)[0]; T.assert(tr.classList.contains('prot'), 'explorer.exe marked protected'); T.assert(tr.querySelector('svg.lock'), 'lock icon');
    await T.click(tr);
    const m = T.mark();
    await T.click(btn(T, 'End task'));
    await T.waitToast(/protected system process/);
    T.assert(!T.modal(), 'no confirm dialog for a protected process'); T.eq(T.calls('kill_process', m).length, 0);
    await T.click(btn(T, 'End process tree'));
    T.assert(!T.modal() && T.calls('kill_process', m).length === 0, 'tree variant also blocked');
  } },

  { name: 'End task: confirm -> cancel -> confirm; mixed selection skips protected', async run(T) {
    await ready(T);
    await search(T, 'Code.exe');
    await T.click(byName(T, 'Code.exe')[0]);
    const m = T.mark();
    await T.click(btn(T, 'End task'));
    await T.waitFor(() => T.modal(), { msg: 'confirm' }); T.has(T.modalText(), 'Code.exe'); T.has(T.modalText(), 'Unsaved data');
    await T.clickText('Cancel'); T.eq(T.calls('kill_process', m).length, 0, 'cancel = no call');
    await T.click(btn(T, 'End task')); await T.waitFor(() => T.modal()); await T.key('Escape'); T.eq(T.calls('kill_process', m).length, 0, 'Escape = cancel');
    await T.click(btn(T, 'End task')); await T.waitFor(() => T.modal()); await T.click({ text: 'End task' }, { root: T.$('.modal-foot') });
    await T.waitToast(/Ended 1 of 1/);
    const c = T.calls('kill_process', m); T.eq(c.length, 1); T.eq(c[0].args.tree, false);
    await T.waitFor(() => byName(T, 'Code.exe').length === 1, { msg: 'one Code.exe left' });
    await T.step('tree + mixed selection', async () => {
      await search(T, '');
      const prot = rows(T).find((tr) => tr.classList.contains('prot')), norm1 = byName(T, 'claude.exe')[0];
      await T.click(prot); await T.click(norm1, { ctrl: true });
      const m2 = T.mark();
      await T.click(btn(T, 'End process tree'));
      await T.waitToast(/1 protected system process/);
      await T.waitFor(() => T.modal()); T.has(T.modalText(), 'claude.exe'); T.has(T.modalText(), 'everything they started');
      await T.click({ text: 'End tree' }, { root: T.$('.modal-foot') });
      await T.waitFor(() => T.calls('kill_process', m2).length === 1, { msg: 'kill call' });
      T.eq(T.calls('kill_process', m2)[0].args.pids.length, 1, 'protected pid not sent'); T.eq(T.calls('kill_process', m2)[0].args.tree, true);
    });
  } },

  { name: 'auto-refresh polls; toggle stops and restarts it', async run(T) {
    await ready(T);
    let m = T.mark(); await T.sleep(7000);
    const on = T.calls('list_processes', m).length; T.assert(on >= 2, `expected >=2 polls in 7s, got ${on}`);
    await T.click('#page input.chk');            // untick Auto-refresh
    m = T.mark(); await T.sleep(7000);
    T.eq(T.calls('list_processes', m).length, 0, 'no polling while auto-refresh is off');
    await T.click('#page input.chk');
    await T.waitFor(() => T.calls('list_processes', m).length >= 1, { msg: 'refresh when re-enabled' });
  } },

  { name: 'selection survives live refresh; Open file location', async run(T) {
    await ready(T);
    await search(T, 'Discord.exe'); await T.click(rows(T)[0]);
    await T.sleep(7000);
    T.eq(T.count('#page tbody tr.sel'), 1, 'selection must survive polls');
    const m = T.mark(); await T.click(btn(T, 'Open file location'));
    await T.waitFor(() => T.calls('open_location', m).length === 1); T.has(T.calls('open_location', m)[0].args.path, 'Discord');
  } },
];
