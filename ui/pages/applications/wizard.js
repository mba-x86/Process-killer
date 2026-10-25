// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Uninstall wizard: Review > Uninstall > Clean up > Finish. Behaviour mirrors app.py `Wizard`.
// Everything the backend sends is untrusted text: only h() / textContent, never innerHTML.

const KIND = { process: 'Process', service: 'Service', startup: 'Startup entry', task: 'Scheduled task', folder: 'Folder' };
const STEPS = ['Review', 'Uninstall', 'Clean up', 'Finish'];
const EDGE_WARNING = 'Microsoft Edge is built into Windows, and Microsoft does not officially support removing it. Some Windows features (Widgets, search results, Help links) may still try to open Edge or fall back to another browser, and Windows Update can install it again. The “Microsoft Edge WebView2 Runtime” is NOT removed - other apps depend on it.';

/** Tick-box list. isOn(item) -> default ticked. Protected rows are locked and never ticked. */
function checkList(ctx, items, isOn, onChange) {
  const { h, ui } = ctx;
  const boxes = items.map((r) => ui.checkbox({ checked: !r.protected && isOn(r), disabled: r.protected, onChange: () => paint() }));
  const cnt = h('span', { class: 'cnt' });
  const paint = () => { cnt.textContent = `${checked().length} of ${items.length} selected`; onChange?.(); };
  const checked = () => items.filter((r, i) => boxes[i].checked && !r.protected).map((r) => r.id);
  const setAll = (v) => { boxes.forEach((b, i) => { if (!items[i].protected) b.checked = v; }); paint(); };
  const rows = items.map((r, i) => h('label', { class: ['wz-row', r.protected && 'locked'] }, boxes[i],
    h('div', { style: { minWidth: 0 } },
      h('div', { class: 'wz-name' }, r.name, r.count > 1 ? h('span', { class: 'dim' }, ` × ${r.count}`) : null),
      h('div', { class: 'wz-sub' }, r.detail || KIND[r.kind] || r.kind)),
    h('span', { class: ['wz-tag', r.reason === 'path' ? 'path' : 'name'], title: r.reason === 'path' ? 'Found in the install folder - certain' : 'Matched by name only - a guess' },
      r.reason === 'path' ? 'Install path' : 'Name only'),
    h('span', { class: 'wz-act' }, r.protected ? 'Protected' : r.action_label || '')));
  const head = h('div', { class: 'wz-head lbl' }, h('span'), h('span', {}, 'Item'), h('span', {}, 'Match'), h('span', { class: 'wz-act' }, 'What will happen'));
  paint();
  return { el: h('div', { class: 'col', style: { '--g': '8px' } },
    h('div', { class: 'wz-tools' }, cnt, h('div', { class: 'row', style: { '--g': '4px' } },
      ui.button({ label: 'Select all', kind: 'ghost', small: true, onClick: () => setAll(true) }),
      ui.button({ label: 'Select none', kind: 'ghost', small: true, onClick: () => setAll(false) }))),
    h('div', { class: 'wz-list' }, head, rows)), checked };
}

export function openWizard(ctx, app, onClose) {
  const { h, ui, api, icon } = ctx;
  const broken = app.uninstaller_missing, edge = app.is_edge;
  const verb = edge ? 'remove Edge' : broken ? 'remove entry' : 'uninstall';
  const Verb = verb[0].toUpperCase() + verb.slice(1);
  let busy = false, closed = false, step = 0;
  const unlisten = [];

  const m = ui.modal({ title: `Uninstall ${app.name}`, steps: STEPS, dismissable: false, body: h('div'),
    onClose: () => { closed = true; unlisten.forEach((f) => f()); document.removeEventListener('keydown', onKey); onClose?.(); } });
  const onKey = (e) => {   // Escape closes the wizard unless it is working, or a notice/confirm dialog sits on top of it (that one closes itself)
    if (e.key !== 'Escape' || busy || step === 1) return;
    if ([...document.querySelectorAll('.overlay')].pop() !== m.el) return;
    m.close();
  };
  document.addEventListener('keydown', onKey);
  m.el.firstChild.classList.toggle('wide', true);
  const setStep = (i) => { step = i; m.setStep(i); };
  const body = (...kids) => m.setBody(h('div', { class: 'wz' }, kids));
  const cancel = (label = 'Cancel') => ui.button({ label, kind: 'ghost', onClick: () => m.close() });
  const left = (b) => { b.classList.add('wz-foot-left'); return b; };
  const spinner = (text) => {
    m.setBody(h('div', { class: 'wz-center' }, ui.spinner(), h('h3', {}, text)));
  };
  const glyph = (kind, name) => h('span', { class: ['glyph', kind] }, icon(name, 26));
  const warnIcon = () => icon('warn', 16);

  /* ---- step 1: scan + review ---- */
  async function stepScan() {
    setStep(0); busy = false;
    spinner('Checking what is running…');
    m.setActions([cancel()]);
    try {
      const [rel, br] = await Promise.all([api.invoke('get_related', { app_id: app.id }),
        edge ? api.invoke('other_browsers') : Promise.resolve({ names: [] })]);
      if (!closed) stepReview(rel.items || [], br.names || []);
    } catch (e) { if (!closed) { ctx.toast(e.message, 'bad'); m.close(); } }
  }

  function stepReview(items, others) {
    setStep(0);
    const kids = [];
    if (broken) kids.push(h('div', { class: 'wz-banner' }, warnIcon(), h('span', {},
      `The uninstaller for ${app.name} is missing, so Windows’ Uninstall button fails. We’ll remove the dead entry from “Installed apps” (a backup is saved so it can be restored) and then clean up the leftover files.`)));
    let ack, block, silent, list = null;
    if (edge) {
      kids.push(h('div', { class: 'wz-banner' }, warnIcon(), h('span', {}, EDGE_WARNING)));
      kids.push(others.length
        ? h('div', { class: 'wz-line ok' }, icon('check', 16), h('span', {}, `Other browsers on this PC: ${others.join(', ')}`))
        : h('div', { class: 'wz-line bad' }, icon('x', 16), h('span', {}, 'No other web browser is installed. Install one first - removal is blocked until you do, so you are never left without a browser.')));
    }
    if (items.length) {
      kids.push(h('h3', {}, 'Stop related items before uninstalling?'),
        h('p', {}, `We found ${items.length} item${items.length === 1 ? '' : 's'} connected to ${app.name}. Running programs can block the uninstaller, so tick the ones to stop first. “Install path” matches are certain; “Name only” matches are guesses and start unticked.`));
      list = checkList(ctx, items, (r) => r.reason === 'path');
      kids.push(list.el);
    } else {
      kids.push(h('div', { class: 'wz-center', style: { paddingTop: '32px' } }, glyph('ok', 'check'), h('h3', {}, 'Nothing related is running'),
        h('p', {}, 'No background processes, services, startup items or tasks were found. You can continue straight away.')));
    }
    if (app.can_silent && !broken && !edge) silent = ui.checkbox({ checked: false, label: 'Use the silent uninstaller (no prompts)' });
    if (silent) kids.push(silent);
    if (edge) {
      ack = ui.checkbox({ checked: false });
      block = ui.checkbox({ checked: true });
      kids.push(h('div', { class: 'wz-opts' },
        h('label', { class: 'row', style: { '--g': '10px', cursor: 'pointer' } }, ack, 'I understand the risks and want to remove Microsoft Edge'),
        h('label', { class: 'row', style: { '--g': '10px', cursor: 'pointer' } }, block, 'Also tell Edge’s updater not to install Edge again (recommended)')));
    }
    body(kids);

    const go = (ids) => async () => {
      if (edge) {
        if (!others.length) return notice('Install another browser first', 'No other web browser was found. Install one (Chrome, Brave, Firefox...) before removing Microsoft Edge.');
        if (!ack.checked) return notice('Please confirm', 'Tick “I understand the risks…” to continue.');
        const ov = await ctx.overview().catch(() => ({ is_admin: true }));
        if (!ov.is_admin) {
          const yes = await ui.confirmDialog({ title: 'Administrator needed', text: 'Removing Microsoft Edge needs Administrator rights. Restart Task Reaper as Administrator now?', confirmLabel: 'Restart as Administrator' });
          if (yes) api.invoke('relaunch_as_admin').catch((e) => ctx.toast(e.message, 'bad'));
          return;
        }
      }
      stepRun(ids(), !!silent?.checked, !!block?.checked);
    };
    m.setActions(items.length
      ? [left(cancel()), ui.button({ label: `${Verb} without stopping`, onClick: go(() => []) }),
        ui.button({ label: `Stop selected & ${verb}`, kind: 'primary', onClick: go(() => list.checked()) })]
      : [left(cancel()), ui.button({ label: Verb, kind: 'primary', onClick: go(() => []) })]);
  }

  function notice(title, text) {
    const n = ui.modal({ title, body: h('p', { class: 'muted', style: { lineHeight: 1.5 } }, text) });
    n.setActions([ui.button({ label: 'OK', kind: 'primary', onClick: () => n.close() })]);
  }

  /* ---- step 2: stop + uninstall (live log from events) ---- */
  async function stepRun(stopIds, silent, blockEdge) {
    setStep(1); busy = true;
    const log = h('div', { class: 'wz-log', role: 'log' });
    body(h('h3', {}, broken ? 'Removing entry…' : 'Uninstalling…'),
      h('p', {}, broken ? 'Saving a backup, then removing the dead entry.' : 'Follow any window the uninstaller opens. This page continues by itself when it is finished.'),
      h('div', { class: 'wz-bar' }), log);
    m.setActions([]);
    let job = null, finished = false, restart = false;
    const buf = [];
    const say = (level, text) => {
      log.append(h('div', { class: level }, `${level === 'ok' ? '✓' : level === 'fail' ? '✕' : '•'} ${text}`));
      log.scrollTop = log.scrollHeight;
    };
    const onProg = (p) => { if (job === null) buf.push(['p', p]); else if (p.job === job && !closed) say(p.level, p.text); };
    const onDone = (p) => {
      if (job === null) { buf.push(['d', p]); return; }
      if (p.job !== job || finished || closed) return;
      finished = true; busy = false; restart = !!p.needs_restart;
      if (p.ok) stepCleanup(restart); else stepFailed(p.message, restart);
    };
    unlisten.push(await api.listen('uninstall:progress', onProg), await api.listen('uninstall:done', onDone));
    try {
      job = (await api.invoke('uninstall_start', { app_id: app.id, stop_ids: stopIds, silent, block_edge_reinstall: edge && blockEdge })).job;
    } catch (e) { busy = false; finished = true; stepFailed(e.message, false); return; }
    for (const [t, p] of buf.splice(0)) (t === 'p' ? onProg : onDone)(p);
  }

  function stepFailed(msg, restart) {
    setStep(1.5); busy = false;
    body(h('div', { class: 'wz-center' }, glyph('bad', 'x'), h('h3', {}, 'The uninstall did not complete'), h('p', {}, msg)));
    m.setActions([cancel('Close'), ui.button({ label: 'Look for leftovers anyway', kind: 'primary', onClick: () => stepCleanup(restart) })]);
  }

  /* ---- step 3: leftovers ---- */
  async function stepCleanup(restart) {
    setStep(2); busy = true;
    spinner('Looking for leftovers…');
    m.setActions([]);
    let items = [];
    try { items = (await api.invoke('find_leftovers', { app_id: app.id })).items || []; } catch { items = []; }
    busy = false;
    if (closed) return;
    if (!items.length) return stepDone([], 'Nothing was left behind.', restart);
    const list = checkList(ctx, items, (r) => (r.default_on ?? r.reason === 'path'));
    body(h('h3', {}, 'Clean up leftovers'),
      h('p', {}, 'The app is gone, but these related items remain. Folders go to the Recycle Bin so you can restore them. Review the list and untick anything you want to keep.'),
      list.el);
    m.setActions([left(ui.button({ label: 'Keep everything', kind: 'ghost', onClick: () => stepDone([], 'Leftovers were kept.', restart) })),
      ui.button({ label: 'Clean up selected', kind: 'primary', onClick: () => doCleanup(items, list.checked(), restart) })]);
  }

  async function doCleanup(items, ids, restart) {
    if (!ids.length) return stepDone([], 'No leftovers were selected.', restart);
    busy = true;
    spinner('Cleaning up…');
    m.setActions([]);
    try {
      const res = (await api.invoke('clean_leftovers', { app_id: app.id, ids })).results || [];
      const name = (id) => items.find((x) => x.id === id)?.name || id;
      stepDone(res.map((r) => ({ ...r, name: name(r.id) })), null, restart);
    } catch (e) { stepDone([], e.message, restart); }
  }

  /* ---- step 4: done ---- */
  function stepDone(results, note, restart) {
    busy = false; setStep(4);
    const bad = results.filter((r) => !r.ok);
    const summary = note || `${results.length - bad.length} leftover item${results.length - bad.length === 1 ? '' : 's'} cleaned${bad.length ? `, ${bad.length} could not be removed.` : '.'}`;
    const log = bad.length ? h('div', { class: 'wz-log', style: { height: '140px', width: '100%', textAlign: 'left' } },
      bad.map((r) => h('div', { class: 'fail' }, `✕ ${r.name} — ${r.message}`))) : null;
    body(h('div', { class: 'wz-center', style: { paddingTop: '28px' } }, glyph('ok', 'check'), h('h3', {}, `${app.name} has been removed`), h('p', {}, summary),
      restart ? h('p', { class: 'warn' }, 'Windows needs to restart to finish removing it.') : null, log));
    m.setActions([ui.button({ label: 'Close', kind: 'primary', onClick: () => m.close() })]);
  }

  stepScan();
  return m;
}
