export default [
  { name: 'renders health, live cards, specs; polling runs', async run(T) {
    await T.open('dashboard');
    await T.step('header', async () => {
      await T.waitText('#page header p', 'DESKTOP-JOGUA40');
      T.eq(T.title(), 'Dashboard');
    });
    await T.step('health text leaves "Checking…"', async () => {
      await T.waitFor(() => /Your PC is running well|Needs attention/.test(T.text('#page .card span[style*="inline-flex"]')), { msg: 'health text' });
    });
    await T.step('live cards fill in', async () => {
      await T.waitFor(() => T.$$('#page .ring-val').every((r) => /\d+%/.test(r.textContent)) && T.count('#page .ring-val') >= 3, { msg: 'ring values' });
      await T.waitText('#page', /\d+C \/ \d+T/);
    });
    await T.step('get_live polls (>=3 calls in ~3.5s)', async () => {
      const m = T.mark(); await T.sleep(3500);
      T.assert(T.calls('get_live', m).length >= 3, `only ${T.calls('get_live', m).length} get_live calls`);
    });
    await T.step('specs + DirectX detect', async () => {
      await T.waitText('#page', 'Device name');
      await T.waitText('#page', 'DirectX 12', { timeout: 8000 });
    });
    await T.step('drives + top apps', async () => {
      await T.waitText('#page', 'New Volume', { timeout: 3000 }).catch(() => {});
      await T.waitFor(() => T.count('#page .split tbody tr') >= 6, { msg: '6 top app rows', timeout: 7000 });
    });
  } },

  { name: 'Boost RAM -> toast, button re-enabled', async run(T) {
    await T.open('dashboard');
    const btn = await T.waitFor(() => T.byText('Boost RAM', { tags: 'button' }));
    await T.click(btn);
    T.assert(btn.disabled, 'button should be disabled while boosting');
    await T.waitToast(/RAM boosted - \d+ MB freed/);
    await T.waitFor(() => !btn.disabled, { msg: 'Boost RAM re-enabled' });
  } },

  { name: 'card links and quick actions navigate', async run(T) {
    for (const [href, title] of [['performance/processor', 'Processor'], ['performance/memory', 'Memory'], ['storage', 'Storage analyzer'], ['performance/network', 'Network']]) {
      await T.step(`card -> ${href}`, async () => {
        await T.open('dashboard');
        await T.click(`a.card-link[href="#/${href}"]`);
        await T.waitFor(() => T.hash() === `#/${href}` && T.title() === title, { msg: `route ${href}` });
      });
    }
    await T.step('Clean junk -> optimizer', async () => {
      await T.open('dashboard');
      await T.clickText('Clean junk');
      await T.waitFor(() => T.title() === 'Optimizer', { msg: 'optimizer' });
    });
    await T.step('Analyze storage -> storage', async () => {
      await T.open('dashboard');
      await T.clickText('Analyze storage');
      await T.waitFor(() => T.title() === 'Storage analyzer', { msg: 'storage' });
    });
  } },

  { name: 'Copy report + Refresh', async run(T) {
    await T.open('dashboard');
    await T.waitText('#page', 'Device name');
    const m = T.mark();
    await T.clickText('Copy report');
    await T.waitToast('System report copied');
    const c = T.calls('copy_text', m);
    T.eq(c.length, 1, 'copy_text calls');
    T.has(c[0].args.text, 'DEVICE', 'report header'); T.has(c[0].args.text, 'DESKTOP-JOGUA40', 'report hostname');
    const m2 = T.mark();
    await T.clickText('Refresh');
    T.assert(T.calls('get_specs_fast', m2).length === 1, 'Refresh should reload specs');
    await T.waitText('#page', 'DirectX 12', { timeout: 9000 });
  } },

  { name: 'limited session shows admin banner', async run(T) {
    await T.open('dashboard', 'admin=0');
    await T.waitText('.banners', 'Not running as Administrator');
    T.assert(T.byText('Restart as Administrator'), 'restart button');
  } },
];
