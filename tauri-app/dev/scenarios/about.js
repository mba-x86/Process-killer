const st = (T) => T.text('#page section:first-of-type span[style*="border-radius: 13px"]');
const btn = (T, l) => T.$$('#page button').find((b) => T.text(b).includes(l));
const banner = (T) => T.text('.banners');

export default [
  { name: 'no update: latest-version status, release notes with tags, settings checkbox, no GitHub button', async run(T) {
    await T.open('about');
    T.eq(T.title(), 'About & updates');
    await T.waitFor(() => /latest version \(1\.0\.3\)/.test(st(T)), { msg: 'up-to-date status', timeout: 6000 });
    T.assert(T.count('#page ul li') === 3, `3 note rows, got ${T.count('#page ul li')}`);
    T.has(T.text('#page ul'), 'Guarded removal of Microsoft Edge'); T.has(T.text('#page ul li:first-child'), 'New');
    T.assert(T.text('#page ul li:last-child').startsWith('Fixed'), 'Fixed tag');
    T.assert(btn(T, 'Install update').style.display === 'none', 'no install button when up to date');
    T.eq(banner(T), '', 'no update banner');
    const m = T.mark();
    await T.click('#page input.chk');
    await T.waitFor(() => T.calls('set_settings', m).length === 1 && T.calls('set_settings', m)[0].args.auto_check === false, { msg: 'set_settings(auto_check:false)' });
    T.assert(!btn(T, 'GitHub') && !T.$$('#page button, #page a').some((e) => /github/i.test(e.textContent + (e.href || ''))), 'no GitHub button or link on the page');
    T.eq(T.calls('open_url', m).length, 0, 'nothing opens a web page');
    T.has(T.text('#page h2'), 'Task Reaper'); T.assert(T.$('#page svg path[d^="M17.8 7.3"]'), 'about page shows the scythe logo');
  } },

  { name: 'update available: status, notes, banner; Update now: cancel no-op, confirm -> progress -> Installing', async run(T) {
    await T.open('about', 'update=1');
    await T.waitFor(() => /Version 2\.0\.1 is available/.test(st(T)), { msg: 'available status', timeout: 6000 });
    T.has(T.text('#page ul'), 'Tauri rebuild'); T.has(T.text('#page'), 'in 2.0.1');
    T.assert(btn(T, 'Install update').style.display !== 'none', 'Install update visible');
    await T.waitFor(() => /Version 2\.0\.1 is available/.test(banner(T)), { msg: 'banner' });
    T.assert(btn(T, 'Check for updates') && !btn(T, 'Check for updates').disabled, 'check button usable');
    const m = T.mark();
    await T.click(T.byText('Update now', { root: T.$('.banners'), tags: 'button' }));
    await T.waitFor(() => T.modal(), { msg: 'confirm dialog' }); T.has(T.modalText(), 'Download and install version 2.0.1');
    await T.clickText('Cancel'); T.eq(T.calls('start_update', m).length, 0, 'cancel = no download'); T.assert(/available/.test(banner(T)), 'banner unchanged');
    await T.click(btn(T, 'Install update')); await T.waitFor(() => T.modal());
    await T.key('Escape'); T.eq(T.calls('start_update', m).length, 0, 'Escape = cancel');
    await T.click(btn(T, 'Install update')); await T.waitFor(() => T.modal()); await T.click({ text: 'Update now' }, { root: T.$('.modal-foot') });
    await T.waitFor(() => /Downloading/.test(banner(T)), { msg: 'downloading banner' });
    await T.waitFor(() => /\d+(\.\d)? MB of 14\.7 MB/.test(banner(T)), { msg: 'progress numbers', timeout: 8000 });
    await T.waitFor(() => /Installing/.test(banner(T)), { msg: 'installing banner', timeout: 15000 });
    T.has(st(T), 'Installing'); T.eq(T.calls('start_update', m).length, 1);
    T.assert(btn(T, 'Check for updates').disabled, 'cannot re-check while installing');
  } },

  { name: 'offline: friendly error, manual re-check toasts and stays usable', async run(T) {
    await T.open('about', 'update=offline');
    await T.waitFor(() => /Could not reach the update server/.test(st(T)), { msg: 'friendly error', timeout: 6000 });
    T.eq(banner(T), '', 'no banner for a failed check');
    await T.click(btn(T, 'Check for updates'));
    await T.waitToast(/Could not reach the update server/);
    await T.waitFor(() => !btn(T, 'Check for updates').disabled, { msg: 'button re-enabled' });
  } },

  { name: 'update banner works from any page (startup check)', async run(T) {
    await T.open('dashboard', 'update=1');
    await T.waitFor(() => /Version 2\.0\.1 is available/.test(banner(T)), { msg: 'startup update banner', timeout: 10000 });
    await T.click(T.byText("What's new", { root: T.$('.banners'), tags: 'button' }));
    await T.waitFor(() => T.title() === 'About & updates', { msg: 'navigates to About' });
    T.has(T.text('#page'), 'Tauri rebuild');
    await T.click(T.byText('Later', { root: T.$('.banners'), tags: 'button' }));
    T.eq(banner(T), '', 'Later hides the banner');
  } },
];
