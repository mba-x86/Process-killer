const rows = (T) => T.$$('#page tbody tr.click');
const removeBtn = (T) => T.byText('Remove selected', { root: T.$('.actions'), tags: 'button' });

export default [
  { name: 'renders startup entries and tasks; remove button gated by selection', async run(T) {
    await T.open('startup');
    await T.waitFor(() => rows(T).length >= 19, { msg: 'rows' });
    T.has(T.text('#page header p'), 'startup entries'); T.has(T.text('#page header p'), 'scheduled tasks');
    T.assert(removeBtn(T).disabled, 'Remove disabled with nothing selected');
    await T.click(rows(T)[0]);
    T.assert(!removeBtn(T).disabled, 'Remove enabled with a selection');
  } },
  { name: 'remove selected -> confirm (cancel/Escape no-op) -> partial failure reported', async run(T) {
    await T.open('startup');
    await T.waitFor(() => rows(T).length >= 19);
    const n0 = rows(T).length;
    const rtk = rows(T).find((tr) => T.text(tr).includes('RtkAudUService')), disc = rows(T).find((tr) => T.text(tr).includes('WallpaperEngine'));
    await T.click(rtk); await T.click(disc, { ctrl: true });
    const m = T.mark();
    await T.click(removeBtn(T));
    await T.waitFor(() => T.modal()); T.has(T.modalText(), 'Permanently remove 2 item(s)?');
    await T.clickText('Cancel'); T.eq(T.calls('remove_startup', m).length, 0);
    await T.click(removeBtn(T)); await T.waitFor(() => T.modal()); await T.key('Escape'); T.eq(T.calls('remove_startup', m).length, 0);
    await T.click(removeBtn(T)); await T.waitFor(() => T.modal()); await T.click({ text: 'Remove' }, { root: T.$('.modal-foot') });
    await T.waitToast(/RtkAudUService: Access is denied/);
    await T.waitToast(/Removed 1 of 2/);
    await T.waitFor(() => rows(T).length === n0 - 1, { msg: 'one row removed' });
    T.assert(rows(T).some((tr) => T.text(tr).includes('RtkAudUService')), 'failed item stays in the list');
    T.assert(!rows(T).some((tr) => T.text(tr).includes('WallpaperEngine')), 'removed item is gone');
  } },
];
