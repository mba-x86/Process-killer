// Memory / Network / Graphics / Processor: render, live values change over a few ticks, graceful degradation.
const snap = (T) => T.$('#page').innerHTML;
const dvals = (T) => T.$$('#page svg path').map((p) => p.getAttribute('d')).join('|');
async function changes(T, label, ms = 4200) {
  const a = snap(T); await T.sleep(ms); const b = snap(T);
  T.assert(a !== b, `${label}: page did not change over ${ms} ms (live updates dead)`);
}

export default [
  { name: 'memory: renders, live numbers/chart update, details fill, Boost RAM', async run(T) {
    await T.open('memory');
    await T.step('render', async () => {
      await T.waitText('#page', /IN USE\s*\d/); await T.waitText('#page', 'DDR4'); await T.waitText('#page', /Committed\s*\d/);
      T.has(T.text('#page'), 'Top apps by memory'); await T.waitFor(() => T.count('#page .pbar') >= 6, { msg: 'top apps rows', timeout: 6000 });
      T.has(T.text('#page header'), '15.8 GB'); T.assert(T.$('#page header a.back'), 'back link');
    });
    await T.step('live update', async () => { const a = dvals(T); await T.sleep(4200); T.assert(dvals(T) !== a, 'chart path never changed'); await changes(T, 'memory'); });
    await T.step('module details tooltip', async () => { T.assert(T.$$('#page dd').some((d) => /DIMM_A1/.test(d.title)), 'slots tooltip lists modules'); });
    await T.step('Boost RAM', async () => { const m = T.mark(); await T.clickText('Boost RAM'); await T.waitToast(/RAM boosted/); T.eq(T.calls('boost_ram', m).length, 1); });
    await T.step('back link navigates', async () => { await T.click('#page header a.back'); await T.waitFor(() => T.title() === 'Dashboard'); });
  } },

  { name: 'network: renders adapters, live values change, adapter switch', async run(T) {
    await T.open('network');
    await T.waitText('#page', 'Realtek PCIe 2.5GbE', { timeout: 6000 });
    T.has(T.text('#page'), 'This session'); T.has(T.text('#page'), 'All adapters');
    await T.step('live update', async () => { await changes(T, 'network'); });
    await T.step('switch adapter via row click', async () => {
      await T.click(T.row('Intel(R) Wi-Fi 6'));
      await T.waitText('#page header p', 'Wi-Fi'); await T.waitText('#page header', 'Disconnected');
      T.has(T.text('#page'), 'F4-6A-DD-01-22-B7');
    });
    await T.step('switch via select', async () => {
      const sel = T.$('select[aria-label="Adapter"]'); T.assert(T.visible(sel), 'select visible with 2 adapters');
      await T.select(sel, '__auto'); await T.waitText('#page header p', 'Realtek');
    });
    await T.step('session totals', async () => { T.assert(T.$$('#page .stat .perf-mid').every((e) => e.textContent !== '—'), 'session tiles filled'); });
  } },

  { name: 'graphics: renders, live updates; legacy GPU degrades gracefully', async run(T) {
    await T.open('graphics');
    await T.step('modern GPU', async () => {
      await T.waitText('#page', 'GeForce RTX 5060'); await T.waitText('#page', /GPU UTILIZATION\s*\d+%/);
      T.has(T.text('#page'), '32.0.15.8157'); T.has(T.text('#page'), 'Video memory'); await T.waitFor(() => T.count('#page .eng') === 5, { msg: '5 engines' });
      await changes(T, 'graphics');
      T.eq(T.text('#page header .pill'), 'Live');
    });
    await T.open('graphics', 'gpu=legacy');
    await T.step('legacy GPU', async () => {
      await T.waitText('#page', 'GT 430');
      await T.waitText('#page', 'Live usage is not available for this GPU');
      T.has(T.text('#page'), 'Per-engine usage is not available'); T.has(T.text('#page'), 'N/A'); T.eq(T.text('#page header .pill'), 'Static info');
      T.has(T.text('#page'), '21.21.13.4201'); T.has(T.text('#page'), 'Live video memory use is not available');
      await T.sleep(4500);   // several ticks without errors
    });
  } },

  { name: 'processor: renders cores/specs, live values change, top processes', async run(T) {
    await T.open('processor');
    await T.waitText('#page', 'i5-14400F'); T.has(T.text('#page'), 'socket U3E1');
    await T.waitFor(() => T.count('#page .core') === 16, { msg: '16 logical processors' });
    T.eq(T.count('#page .core .tag-p'), 12); T.eq(T.count('#page .core .tag-e'), 4);
    await T.waitFor(() => T.count('#page .split:last-child tbody tr') === 6, { msg: '6 top processes', timeout: 6000 });
    await T.waitText('#page', /UTILIZATION\s*\d+%/);
    T.has(T.text('#page'), 'Specifications'); T.has(T.text('#page'), '10 MB');
    await changes(T, 'processor', 5000);
    const m = T.mark(); await T.sleep(3200);
    T.assert(T.calls('get_live', m).length >= 2, 'get_live polling'); T.assert(T.calls('get_top_processes', m).length >= 1, 'top processes polling');
  } },
];
