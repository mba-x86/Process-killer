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
      T.has(T.text('#page header'), '15.8 GB'); T.assert(!T.$('#page header a.back'), 'no back link inside tabs');
    });
    await T.step('live update', async () => { const a = dvals(T); await T.sleep(4200); T.assert(dvals(T) !== a, 'chart path never changed'); await changes(T, 'memory'); });
    await T.step('module details tooltip', async () => { T.assert(T.$$('#page dd').some((d) => /DIMM_A1/.test(d.title)), 'slots tooltip lists modules'); });
    await T.step('Boost RAM', async () => { const m = T.mark(); await T.clickText('Boost RAM'); await T.waitToast(/RAM boosted/); T.eq(T.calls('boost_ram', m).length, 1); });
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

  { name: 'performance: one page with Processor/Memory/Network/Graphics tabs; URL follows the tab; old links land on the right tab', async run(T) {
    const tab = (l) => T.byText(l, { root: T.$('.ptabs'), tags: 'button' });
    const sel = () => T.$$('.ptabs button').filter((b) => b.getAttribute('aria-selected') === 'true').map((b) => T.text(b));
    await T.open('performance');
    await T.step('default tab + sidebar', async () => {
      await T.waitText('#page', /Specifications|UTILIZATION/i, { timeout: 8000 });
      T.eq(T.$$('.ptabs button').length, 4, 'four tabs'); T.eq(sel().join(), 'Processor');
      T.eq(T.$$('a.nav').filter((a) => /Performance/.test(a.textContent)).length, 1, 'one Performance entry'); T.assert(T.$('.nav[data-route="performance"]').classList.contains('on'), 'sidebar item active');
      T.assert(!T.$('.nav[data-route="memory"]'), 'old sidebar entries are gone');
    });
    await T.step('switching tabs swaps the content in place', async () => {
      await T.click(tab('Memory')); await T.waitText('#page', /IN USE\s*\d/, { timeout: 8000 });
      T.eq(sel().join(), 'Memory'); T.eq(T.hash(), '#/performance/memory'); T.eq(T.title(), 'Memory');
      await T.click(tab('Network')); await T.waitText('#page', 'Realtek PCIe 2.5GbE', { timeout: 8000 }); T.eq(T.hash(), '#/performance/network');
      await T.click(tab('Graphics')); await T.waitFor(() => T.title() === 'Graphics', { msg: 'graphics tab' });
      await T.click(tab('Processor')); await T.waitFor(() => T.title() === 'Processor', { msg: 'processor tab' });
    });
    await T.step('leaving a tab stops its timers (no leak)', async () => {
      await T.click(tab('Memory')); await T.waitText('#page', /IN USE\s*\d/); await T.sleep(300);
      const base = T.intervals();
      for (let i = 0; i < 3; i++) { await T.click(tab('Network')); await T.sleep(250); await T.click(tab('Memory')); await T.sleep(250); }
      await T.sleep(600); T.assert(T.intervals() <= base + 1, `intervals grew from ${base} to ${T.intervals()}`);
    });
    await T.step('keyboard: arrows move between tabs', async () => {
      T.$('.ptabs button.on').focus(); await T.key('ArrowRight', T.$('.ptabs button.on')); await T.waitFor(() => sel().join() === 'Network', { msg: 'ArrowRight -> Network' });
      await T.key('ArrowLeft', T.$('.ptabs button.on')); await T.waitFor(() => sel().join() === 'Memory', { msg: 'ArrowLeft -> Memory' });
    });
    await T.step('no back link inside a tab; the tab survives a reload of the URL', async () => {
      T.assert(!T.$('#page header a.back'), 'no back link');
      await T.open('performance/graphics'); await T.waitFor(() => T.title() === 'Graphics' && /Graphics/.test(sel().join()), { msg: 'graphics opened by URL' });
    });
    await T.step('old links (#/memory) land on the Memory tab', async () => {
      await T.open('memory'); await T.waitFor(() => sel().join() === 'Memory' && T.title() === 'Memory', { msg: 'legacy route' });
      T.assert(T.$('.nav[data-route="performance"]').classList.contains('on'), 'sidebar shows Performance');
    });
  } },

  { name: 'performance: Processor stats stay plain text on one line even after Memory/Network styles were loaded', async run(T) {
    await T.open('performance/memory');
    await T.waitText('#page', /IN USE\s*\d/, { timeout: 8000 });
    await T.click(T.byText('Processor', { root: T.$('.ptabs'), tags: 'button' }));
    await T.waitText('#page', 'UPTIME', { timeout: 8000 });
    const stats = T.$$('#page .pstat, #page .stat');
    T.eq(stats.length, 3, 'three stats');
    for (const el of stats) {
      const cs = T.win.getComputedStyle(el), v = el.querySelector('b');
      T.eq(cs.borderTopWidth, '0px', 'no box leaked from the Memory/Network stylesheet');
      T.assert(v.getBoundingClientRect().height < 30, `value wraps onto several lines (${T.text(el)})`);
    }
  } },
];
