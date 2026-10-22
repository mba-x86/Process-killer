const jrows = (T) => T.$$('.jrow');
const total = (T) => T.text('.opt-head span:last-child');
const b = (T, l) => T.byText(l, { tags: 'button' });
async function ready(T) { await T.open('optimizer'); await T.waitFor(() => jrows(T).length === 5, { msg: '5 categories' }); }
const sizes = (T) => jrows(T).map((r) => T.text('.sz', r));

export default [
  { name: 'renders gauge, categories (browser caches unticked), live memory text', async run(T) {
    await ready(T);
    const on = jrows(T).map((r) => r.querySelector('input').checked);
    T.eq(JSON.stringify(on), JSON.stringify([true, true, true, true, false]), 'default ticks');
    await T.waitText('.opt-mem', /of .* used/); await T.waitText('.opt-gauge', /\d+%/);
    T.has(T.text('.opt-head'), 'Press Scan');
  } },

  { name: 'Boost RAM -> busy state -> result + toast', async run(T) {
    await ready(T);
    const btn = T.$('.opt-boost');
    await T.click(btn);
    T.assert(btn.disabled, 'Boost disabled while running'); T.assert(T.$('.opt-gauge').classList.contains('busy'), 'gauge shows busy');
    await T.waitToast(/RAM boosted/);
    await T.waitFor(() => /Freed 612 MB/.test(T.text('.opt-freed')), { msg: 'Freed 612 MB', timeout: 4000 });
    T.has(T.text('.opt-note'), '41 apps trimmed');
    T.assert(!btn.disabled, 'Boost enabled again'); T.assert(!T.$('.opt-gauge').classList.contains('busy'));
  } },

  { name: 'Scan fills sizes; ticking updates the selected total', async run(T) {
    await ready(T);
    await T.click(b(T, 'Scan'));
    T.assert(b(T, 'Scan').disabled && b(T, 'Clean selected').disabled, 'buttons disabled during scan');
    await T.waitFor(() => sizes(T).every((s) => /\d/.test(s) && s !== '…'), { msg: 'all sizes', timeout: 12000 });
    T.eq(JSON.stringify(sizes(T)), JSON.stringify(['842 MB', '1.3 GB', '214 MB', '3.2 GB', '1.1 GB']), 'sizes');
    await T.waitFor(() => /found/.test(total(T)), { msg: 'total line' });
    const before = total(T);
    await T.click(jrows(T)[4].querySelector('input'));   // tick browser caches
    T.assert(total(T) !== before, `selected total must change (${before} -> ${total(T)})`);
    T.assert(!b(T, 'Scan').disabled, 'buttons enabled after scan');
  } },

  { name: 'Clean selected: confirm lists ticked categories, cancel is a no-op, confirm cleans', async run(T) {
    await ready(T);
    await T.click(b(T, 'Scan'));
    await T.waitFor(() => /found/.test(total(T)), { msg: 'scan done', timeout: 12000 });
    const m = T.mark();
    await T.click(b(T, 'Clean selected'));
    await T.waitFor(() => T.modal(), { msg: 'confirm modal' });
    const txt = T.modalText();
    for (const c of ['User temp files', 'Windows temp files', 'Crash dumps', 'Recycle Bin']) T.has(txt, c, 'lists category');
    T.lacks(txt, 'Browser caches', 'unticked category must not be listed');
    await T.clickText('Cancel'); T.eq(T.calls('clean_start', m).length, 0, 'cancel = no clean');
    await T.click(b(T, 'Clean selected')); await T.waitFor(() => T.modal()); await T.key('Escape'); T.eq(T.calls('clean_start', m).length, 0, 'Escape = cancel');
    await T.click(b(T, 'Clean selected')); await T.waitFor(() => T.modal());
    await T.click({ text: 'Clean' }, { root: T.$('.modal-foot') });
    await T.waitFor(() => /Freed/.test(total(T)), { msg: 'freed total', timeout: 15000 });
    T.has(total(T), '3 items in use skipped');
    T.eq(T.calls('clean_start', m)[0].args.ids.length, 4, 'ids sent');
    T.assert(jrows(T).slice(0, 4).every((r) => /Cleaned/.test(T.text('.sz', r))), 'rows show Cleaned');
    T.eq(T.text('.sz', jrows(T)[4]), '1.1 GB', 'unticked category untouched');
    await T.waitToast(/Cleaned .* of junk/);
    T.assert(!b(T, 'Clean selected').disabled, 'buttons re-enabled');
  } },

  { name: 'Clean without scanning scans first; nothing ticked toasts', async run(T) {
    await ready(T);
    const m = T.mark();
    await T.click(b(T, 'Clean selected'));
    await T.waitFor(() => T.modal(), { msg: 'confirm after implicit scan', timeout: 15000 });
    T.assert(T.calls('scan_junk', m).length === 5, 'implicit scan covers all categories');
    await T.clickText('Cancel');
    for (const r of jrows(T)) if (r.querySelector('input').checked) await T.click(r.querySelector('input'));
    await T.click(b(T, 'Clean selected'));
    await T.waitToast('Tick at least one category');
  } },

  { name: 'Boost everything = clean then boost RAM', async run(T) {
    await ready(T);
    await T.click(b(T, 'Scan')); await T.waitFor(() => /found/.test(total(T)), { timeout: 12000 });
    const m = T.mark();
    await T.click(b(T, 'Boost everything'));
    await T.waitFor(() => T.modal()); await T.click({ text: 'Clean' }, { root: T.$('.modal-foot') });
    await T.waitFor(() => T.calls('boost_ram', m).length === 1, { msg: 'boost after clean', timeout: 15000 });
    await T.waitToast(/RAM boosted/);
  } },

  { name: 'leaving mid-clean does not throw or leak listeners', async run(T) {
    await ready(T);
    await T.click(b(T, 'Scan')); await T.waitFor(() => /found/.test(total(T)), { timeout: 12000 });
    await T.click(b(T, 'Clean selected')); await T.waitFor(() => T.modal()); await T.click({ text: 'Clean' }, { root: T.$('.modal-foot') });
    await T.sleep(300);
    await T.nav('activity');
    await T.sleep(4000);
    T.eq(T.listeners('clean:progress'), 0, 'clean:progress listeners left behind');
    T.eq(T.listeners('clean:done'), 0, 'clean:done listeners left behind');
  } },
];
