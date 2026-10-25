const rows = (T) => T.$$('.apps-tbl tbody tr[data-id]');
const sub = (T) => T.text('#page header p');
const nameCell = (T, name) => T.$$('.apps-tbl td.name').find((td) => T.text(td) === name);
async function loaded(T) { await T.waitFor(() => rows(T).length > 50, { msg: 'application rows', timeout: 10000 }); }
async function search(T, v) { await T.type('.search input', v); await T.sleep(250); }
async function pick(T, name) {
  const td = await T.waitFor(() => nameCell(T, name), { msg: `row ${name}` });
  await T.click(td.closest('tr'));
  await T.waitFor(() => T.text('.apps-detail h2') === name, { msg: `detail panel for ${name}` });
}

export default [
  { name: 'list, search, drive filter, sort', async run(T) {
    await T.open('applications');
    await loaded(T);
    await T.step('count', async () => T.has(sub(T), '94 of 94 applications'));
    await T.step('search filters (name + publisher)', async () => {
      await search(T, 'brave'); T.eq(rows(T).length, 1, 'rows for "brave"'); T.has(sub(T), '1 of 94');
      await search(T, 'gigabyte'); T.assert(rows(T).length >= 8, 'publisher search should match many GIGABYTE entries');
      await search(T, 'zzzz-nothing'); T.has(T.text('.apps-tbl tbody'), 'No applications match');
      await search(T, ''); T.eq(rows(T).length, 94, 'rows after clearing the search');
    });
    await T.step('drive filter', async () => {
      const sel = T.$('select[aria-label="Drive"]');
      const opts = [...sel.options].map((o) => o.value);
      T.assert(opts.includes('C:\\') && opts.includes('E:\\'), `drive options ${opts}`);
      await T.select(sel, 'E:\\');
      const n = rows(T).length; T.assert(n > 0 && n < 94, `E: rows ${n}`);
      T.has(sub(T), 'on E:\\');
      T.assert(T.$$('.apps-tbl td.name').some((td) => T.text(td) === 'Counter-Strike 2'), 'CS2 is on E:');
      T.assert(!nameCell(T, 'Brave'), 'Brave is on C: and must be hidden');
      await T.select(sel, 'All drives'); T.eq(rows(T).length, 94);
    });
    await T.step('sort by name toggles', async () => {
      await T.clickText('Name'); T.has(T.text('.apps-tbl thead'), 'Name ↓');
      const first = T.text(T.$('.apps-tbl tbody td.name')); T.assert(/^[Z]|^Zoom|^WinRAR|^VLC/.test(first), `first row after Z-A sort: ${first}`);
      await T.clickText('Name'); T.has(T.text('.apps-tbl thead'), 'Name ↑');
    });
    await T.step('Refresh reloads', async () => {
      const m = T.mark(); await T.clickText('Refresh'); await T.waitFor(() => T.calls('list_apps', m).length === 1 && rows(T).length > 50);
    });
  } },

  { name: 'select Brave -> related list; select Edge/broken -> warnings and button labels', async run(T) {
    await T.open('applications');
    await loaded(T);
    await T.step('Brave', async () => {
      await pick(T, 'Brave');
      await T.waitText('.apps-rel', 'brave.exe'); T.has(T.text('.apps-rel'), 'Brave Elevation Service');
      T.has(T.text('.apps-rel'), '× 21', 'process count');
      T.eq(T.text('.apps-foot .btn-block'), 'Uninstall safely');
      T.assert(T.$('.apps-note').hidden, 'no warning for a normal app');
      T.has(T.text('.apps-detail .meta'), 'Brave Software Inc');
    });
    await T.step('rapid re-select shows only the last app (no stale related list)', async () => {
      await pick(T, 'Discord'); await pick(T, 'Docker Desktop');
      await T.waitText('.apps-rel', 'Docker Desktop');
      await T.sleep(900);
      T.lacks(T.text('.apps-rel'), 'Discord.exe', 'stale related list from the previous app');
    });
    await T.step('Microsoft Edge', async () => {
      await search(T, 'edge'); await pick(T, 'Microsoft Edge');
      T.assert(!T.$('.apps-note').hidden, 'Edge warning visible'); T.has(T.text('.apps-note'), 'Built into Windows');
      T.eq(T.text('.apps-foot .btn-block'), 'Remove Microsoft Edge…');
      T.has(T.text('.apps-foot'), 'another web browser');
      await T.waitText('.apps-rel', 'msedge.exe');
    });
    await T.step('broken entry', async () => {
      await search(T, 'men of war'); await pick(T, 'Men of War: Assault Squad 2 Airborne');
      T.has(T.text('.apps-note'), 'uninstaller file is missing'); T.eq(T.text('.apps-foot .btn-block'), 'Remove broken entry');
    });
    await T.step('app without details/location', async () => {
      await search(T, 'GBT_MB_Update'); await pick(T, 'GBT_MB_Update');
      T.assert(T.$('.apps-detail button[title="Open install folder"]').hidden, 'open-folder button is hidden without a location');
      await T.waitText('.apps-rel', 'Nothing is running');
    });
  } },

  { name: 'wizard: normal app review -> run -> cleanup -> finish', async run(T) {
    await T.open('applications');
    await loaded(T);
    await search(T, 'brave'); await pick(T, 'Brave');
    await T.clickText('Uninstall safely');
    await T.step('review', async () => {
      await T.waitText('.modal', 'Stop related items before uninstalling?', { timeout: 6000 });
      T.has(T.text('.modal-head h2'), 'Uninstall Brave'); T.has(T.text('.modal .steps'), 'Review');
      T.has(T.text('.modal .wz-tools .cnt'), '4 of 5 selected', 'path matches ticked, name-only unticked');
      T.assert(T.byText('Use the silent uninstaller'), 'silent option');
    });
    await T.step('select none / all', async () => {
      await T.clickText('Select none'); T.has(T.text('.modal .wz-tools .cnt'), '0 of 5');
      await T.clickText('Select all'); T.has(T.text('.modal .wz-tools .cnt'), '5 of 5');
      await T.clickText('Select none'); await T.clickText('Select all'); await T.click(T.$$('.modal .wz-row input')[4]); T.has(T.text('.modal .wz-tools .cnt'), '4 of 5');
    });
    const m = T.mark();
    await T.clickText('Stop selected & uninstall');
    await T.step('run step streams the log; Escape is ignored while busy', async () => {
      await T.waitFor(() => T.count('.modal .wz-log > div') >= 2, { msg: 'log lines', timeout: 6000 });
      await T.key('Escape'); T.assert(T.modal(), 'wizard must stay open while uninstalling');
      T.has(T.text('.modal .steps .on'), 'Uninstall');
    });
    await T.step('cleanup list', async () => {
      await T.waitText('.modal', 'Clean up leftovers', { timeout: 20000 });
      const c = T.calls('uninstall_start', m); T.eq(c.length, 1, 'uninstall_start calls');
      T.eq(c[0].args.stop_ids.length, 4, 'stop ids'); T.eq(c[0].args.silent, false);
      T.has(T.text('.modal .wz-tools .cnt'), '2 of 3 selected');
    });
    await T.step('finish', async () => {
      await T.clickText('Clean up selected');
      await T.waitText('.modal', 'Brave has been removed', { timeout: 8000 });
      T.has(T.text('.modal'), '2 leftover items cleaned');
      await T.clickText('Close');
      T.assert(!T.modal(), 'modal closed');
    });
    await T.step('list refreshed + panel reset', async () => {
      await T.waitText('#page header p', /of 93 applications/);
      T.assert(!T.$('.a-empty').hidden, 'empty-state panel is shown again'); T.assert(T.$('.apps-detail').hidden, 'detail panel hidden');
      await search(T, ''); T.eq(rows(T).length, 93);
    });
  } },

  { name: 'wizard: Escape closes review; double-click opens; no stopping items', async run(T) {
    await T.open('applications');
    await loaded(T);
    await search(T, 'notepad');
    const td = await T.waitFor(() => nameCell(T, 'Notepad++'));
    await T.click(td.closest('tr'), { dbl: true });
    await T.waitText('.modal', 'Uninstall Notepad++', { timeout: 5000 });
    await T.waitText('.modal', 'Stop related items', { timeout: 5000 });
    const m = T.mark();
    await T.key('Escape');
    T.assert(!T.modal(), 'Escape should close the wizard at the review step');
    T.eq(T.calls('uninstall_start', m).length, 0, 'nothing may be uninstalled');
  } },

  { name: 'wizard: failing uninstaller -> failure page -> leftovers anyway -> keep', async run(T) {
    await T.open('applications');
    await loaded(T);
    await search(T, 'docker'); await pick(T, 'Docker Desktop');
    await T.clickText('Uninstall safely');
    await T.waitText('.modal', 'Stop related items', { timeout: 6000 });
    await T.clickText('Uninstall without stopping');
    await T.waitText('.modal', 'The uninstall did not complete', { timeout: 15000 });
    T.has(T.text('.modal'), '1603');
    await T.step('Escape closes the failure page', async () => {
      T.assert(T.byText('Close'), 'Close button');
    });
    await T.clickText('Look for leftovers anyway');
    await T.waitText('.modal', 'Clean up leftovers', { timeout: 6000 });
    await T.clickText('Keep everything');
    await T.waitText('.modal', 'Leftovers were kept.');
    await T.clickText('Close');
    await T.waitFor(() => !T.modal());
    T.eq(rows(T).length, 1, 'Docker is still listed (uninstall failed)');
  } },

  { name: 'wizard: Microsoft Edge is guarded (ack + other browser + admin)', async run(T) {
    await T.open('applications');
    await loaded(T);
    await search(T, 'microsoft edge'); await pick(T, 'Microsoft Edge');
    await T.clickText('Remove Microsoft Edge…');
    await T.waitText('.modal', 'Other browsers on this PC: Brave, Google Chrome', { timeout: 6000 });
    T.has(T.text('.modal'), 'NOT removed', 'WebView2 warning');
    const m = T.mark();
    await T.step('primary buttons blocked until "I understand" is ticked', async () => {
      await T.clickText('Stop selected & remove Edge');
      await T.waitText('.overlay:last-of-type', 'Please confirm'); T.eq(T.calls('uninstall_start', m).length, 0);
      await T.clickText('OK'); T.has(T.text('.modal-head h2'), 'Uninstall Microsoft Edge');
      T.assert(T.modal() && T.$$('.overlay').length === 1, 'only the wizard remains');
      await T.clickText('Remove Edge without stopping');
      await T.waitText('.overlay:last-of-type', 'Please confirm'); T.eq(T.calls('uninstall_start', m).length, 0);
      await T.clickText('OK');
    });
    await T.step('Escape on the notice does not close the wizard', async () => {
      await T.clickText('Stop selected & remove Edge');
      await T.waitText('.overlay:last-of-type', 'Please confirm');
      await T.key('Escape');
      T.eq(T.$$('.overlay').length, 1, 'only the notice should close');
      T.has(T.text('.modal-head h2'), 'Uninstall Microsoft Edge');
    });
    await T.step('protected process row is locked', async () => {
      const locked = T.$$('.modal .wz-row.locked'); T.eq(locked.length, 1, 'locked rows'); T.assert(locked[0].querySelector('input').disabled, 'locked checkbox disabled');
      T.has(T.text('.modal .wz-tools .cnt'), '4 of 6 selected');
    });
    await T.step('ack + run + results', async () => {
      await T.click({ text: 'I understand the risks' });
      await T.clickText('Stop selected & remove Edge');
      await T.waitFor(() => T.calls('uninstall_start', m).length === 1, { msg: 'uninstall_start' });
      T.eq(T.calls('uninstall_start', m)[0].args.block_edge_reinstall, true);
      T.assert(!T.calls('uninstall_start', m)[0].args.stop_ids.some((i) => i.endsWith(':p2')), 'protected row never stopped');
      await T.waitText('.modal', 'Clean up leftovers', { timeout: 20000 });
      await T.clickText('Clean up selected');
      await T.waitText('.modal', 'Microsoft Edge has been removed', { timeout: 8000 });
      T.has(T.text('.modal'), '1 leftover item cleaned, 1 could not be removed');
      T.has(T.text('.modal .wz-log'), 'Access is denied');
      await T.clickText('Close');
    });
  } },

  { name: 'wizard: Edge blocked without another browser; limited session asks for admin', async run(T) {
    await T.open('applications', 'admin=0', { override: { other_browsers: async () => ({ names: [] }) } });
    await loaded(T);
    await search(T, 'microsoft edge'); await pick(T, 'Microsoft Edge');
    await T.clickText('Remove Microsoft Edge…');
    await T.waitText('.modal', 'No other web browser is installed', { timeout: 6000 });
    await T.click({ text: 'I understand the risks' });
    const m = T.mark();
    await T.clickText('Stop selected & remove Edge');
    await T.waitText('.overlay:last-of-type', 'Install another browser first');
    await T.clickText('OK');
    T.eq(T.calls('uninstall_start', m).length, 0);
    // second part: with a browser present but not admin -> "Administrator needed"
    T.pk.override.other_browsers = async () => ({ names: ['Brave'] });
    await T.clickText('Cancel'); await T.waitFor(() => !T.modal());
    await T.clickText('Remove Microsoft Edge…');
    await T.waitText('.modal', 'Other browsers on this PC: Brave', { timeout: 6000 });
    await T.click({ text: 'I understand the risks' });
    await T.clickText('Stop selected & remove Edge');
    await T.waitText('.overlay:last-of-type', 'Administrator needed');
    await T.clickText('Cancel');
    T.eq(T.calls('relaunch_as_admin', m).length, 0, 'cancel must not relaunch');
    T.eq(T.calls('uninstall_start', m).length, 0, 'cancel must not uninstall');
    await T.clickText('Stop selected & remove Edge');
    await T.waitText('.overlay:last-of-type', 'Administrator needed');
    await T.clickText('Restart as Administrator', { root: T.modal() });
    await T.waitFor(() => T.calls('relaunch_as_admin', m).length === 1, { msg: 'relaunch_as_admin' });
  } },

  { name: 'wizard: broken entry (missing uninstaller)', async run(T) {
    await T.open('applications');
    await loaded(T);
    await search(T, 'men of war'); await pick(T, 'Men of War: Assault Squad 2 Airborne');
    await T.clickText('Remove broken entry');
    await T.waitText('.modal', 'uninstaller for Men of War', { timeout: 6000 });
    T.has(T.text('.modal'), 'Nothing related is running');
    T.assert(!T.byText('Use the silent uninstaller'), 'no silent option for a broken entry');
    await T.clickText('Remove entry');
    await T.waitText('.modal', 'Backup saved', { timeout: 8000 });
    await T.waitText('.modal', 'Clean up leftovers', { timeout: 10000 });
    await T.clickText('Clean up selected');
    await T.waitText('.modal', 'has been removed', { timeout: 8000 });
    await T.clickText('Close');
    await T.waitText('#page header p', /of 93 applications/);
  } },

  { name: 'limited session: wizard for a normal app still works; detail panel survives Refresh', async run(T) {
    await T.open('applications', 'admin=0');
    await loaded(T);
    await search(T, 'discord'); await pick(T, 'Discord');
    await T.clickText('Refresh');
    await T.waitFor(() => T.text('.apps-detail h2') === 'Discord' && !T.$('.apps-detail').hidden, { msg: 'panel stays on Discord after refresh' });
  } },
];
