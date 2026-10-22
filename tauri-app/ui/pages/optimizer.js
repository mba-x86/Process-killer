// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Optimizer: RAM gauge + Boost RAM, junk cleaner. API: docs/api/optimizer.md
export default async function (ctx) {
  const { h, ui, charts, fmt, api, icon } = ctx;
  ctx.css('css/pages/optimizer.css');
  const setT = (el, t) => { if (el.textContent !== t) el.textContent = t; };

  /* ---------------- memory card ---------------- */
  const gauge = charts.arcGauge({ size: 190, stroke: 13, color: 'var(--teal)' });
  gauge.set(0, '—', 'RAM in use');
  const gaugeBox = h('div', { class: 'opt-gauge' }, gauge);
  const usedL = h('span', { style: { fontWeight: 500 } }, '—'), availL = h('span', { class: 'muted' }, '—');
  const hist = charts.history(60);
  const spark = charts.sparkline({ w: 300, h: 80, color: 'var(--teal)', max: 100 });
  const btnRam = ui.button({ label: 'Boost RAM', icon: 'bolt', kind: 'primary', onClick: () => boostRam() });
  btnRam.classList.add('opt-boost');
  const freedL = h('div', { class: 'opt-freed' });
  const detail = h('p', { class: 'opt-note' }, 'Releases idle memory held by running apps.', h('br'), 'The effect is temporary.');
  const memCard = ui.card({ class: 'opt-mem' },
    h('h2', {}, 'Memory'), gaugeBox,
    h('div', { class: 'col', style: { alignItems: 'center', '--g': '4px' } }, usedL, availL),
    h('div', { class: 'opt-chart' }, h('div', { class: 'plot' }, spark), h('span', { class: 'muted', style: { textAlign: 'center', fontSize: '12px' } }, 'last 60 seconds')),
    btnRam, freedL, detail);

  let ramBusy = false, timers = [];
  ctx.onCleanup(() => timers.forEach(clearTimeout));
  const tick = async () => {
    const m = (await api.invoke('get_live')).mem;
    setT(usedL, `${fmt.bytes(m.used)} of ${fmt.bytes(m.total)} used`);
    setT(availL, `${fmt.bytes(m.available)} available`);
    hist.push(m.percent); spark.update(hist.values);
    if (!ramBusy) gauge.set(m.percent, `${Math.round(m.percent)}%`, 'RAM in use');
  };
  ctx.every(1000, tick);

  function countUp(to, ms, paint) {   // short JS stepping (<= 25 updates) for the "Freed X" figure
    const steps = 25;
    for (let i = 1; i <= steps; i++) timers.push(setTimeout(() => paint(to * (1 - (1 - i / steps) ** 3)), (ms * i) / steps));
  }

  async function boostRam() {
    if (ramBusy) return;
    ramBusy = true; syncButtons();
    freedL.textContent = ''; setT(detail, 'Optimizing memory…');
    gaugeBox.classList.add('busy');
    try {
      const [r, ov] = await Promise.all([api.invoke('boost_ram'), ctx.overview().catch(() => ({}))]);
      gaugeBox.classList.remove('busy');
      gauge.set(r.percent, `${Math.round(r.percent)}%`, 'RAM in use');
      const note = !r.purged && !ov.is_admin ? ' · Run as Administrator for a deeper boost' : '';
      setT(detail, `${r.trimmed} apps trimmed${note}`);
      countUp(r.freed, 1100, (v) => setT(freedL, `Freed ${fmt.bytes(v)}`));
      ctx.toast(`RAM boosted — ${fmt.bytes(r.freed)} freed`, 'ok');
    } catch (e) {
      gaugeBox.classList.remove('busy');
      setT(detail, 'Boost failed'); ctx.toast(e.message, 'bad');
    } finally { ramBusy = false; syncButtons(); }
  }

  /* ---------------- junk cleaner ---------------- */
  const cats = (await api.invoke('get_junk_categories')).categories;
  const rows = cats.map((c) => {
    const chk = h('input', { type: 'checkbox', class: 'chk', checked: c.default_on, onchange: updateTotal });
    const size = h('span', { class: 'sz' }, '—');
    const bar = charts.bar(0, 'var(--amber)');
    const el = h('label', { class: 'jrow' },
      h('span', { class: 'top' }, chk, h('span', { class: 'info' }, h('span', { class: 't' }, c.title), h('span', { class: ['d', c.warn && 'warn'] }, c.desc)), size), bar);
    return { c, chk, size, bar, el, bytes: null };
  });
  const totalL = h('span', { style: { fontSize: '12px', color: 'var(--muted)' } }, 'Press Scan to see what can be removed');
  const btnScan = ui.button({ label: 'Scan', icon: 'search', onClick: () => scan() });
  const btnClean = ui.button({ label: 'Clean selected', kind: 'danger', onClick: () => clean() });
  const btnAll = ui.button({ label: 'Boost everything', icon: 'bolt', kind: 'primary', onClick: () => boostAll() });
  let busy = false;
  function syncButtons() { [btnScan, btnClean].forEach((b) => { b.disabled = busy; }); btnAll.disabled = busy || ramBusy; btnRam.disabled = ramBusy || busy; }
  const junkCard = ui.card({ class: 'opt-junk' },
    h('div', { class: 'opt-head' }, h('h2', {}, 'Junk cleaner'), totalL),
    h('div', { class: 'opt-list' }, rows.map((r) => r.el)),
    h('div', { class: 'opt-foot' }, h('div', { class: 'row' }, btnScan, btnClean), btnAll));

  function updateTotal() {
    if (busy || rows.every((r) => r.bytes == null)) return;
    const found = rows.reduce((s, r) => s + (r.bytes || 0), 0), sel = rows.reduce((s, r) => s + (r.chk.checked ? r.bytes || 0 : 0), 0);
    setT(totalL, `${fmt.bytes(found)} found · ${fmt.bytes(sel)} selected`);
  }

  async function scan() {
    if (busy) return;
    busy = true; syncButtons();
    rows.forEach((r) => { r.bytes = null; r.el.classList.remove('done'); r.size.className = 'sz'; setT(r.size, '…'); r.bar.set(0, 'var(--amber)'); });
    setT(totalL, 'Scanning…');
    for (const r of rows) {
      r.el.classList.add('scanning');
      try { r.bytes = (await api.invoke('scan_junk', { id: r.c.id })).bytes || 0; } catch { r.bytes = 0; setT(r.size, 'n/a'); }
      r.el.classList.remove('scanning');
      if (r.bytes) { r.size.className = 'sz has'; setT(r.size, fmt.bytes(r.bytes)); } else if (r.size.textContent !== 'n/a') setT(r.size, 'Empty');
    }
    const max = Math.max(1, ...rows.map((r) => r.bytes));
    rows.forEach((r) => r.bar.set((r.bytes / max) * 100));
    busy = false; syncButtons(); updateTotal();
  }

  async function clean() {
    if (busy) return false;
    const sel = rows.filter((r) => r.chk.checked);
    if (!sel.length) { ctx.toast('Tick at least one category first'); return false; }
    if (sel.some((r) => r.bytes == null)) { await scan(); }
    const todo = rows.filter((r) => r.chk.checked && r.bytes);
    if (!todo.length) { ctx.toast("Nothing to clean — you're already tidy"); return true; }
    const body = h('div', { class: 'col', style: { '--g': '10px' } },
      h('p', { class: 'muted' }, 'Permanently delete:'),
      h('ul', { style: { margin: 0, paddingLeft: '18px', lineHeight: 1.7 } }, todo.map((r) => h('li', {}, `${r.c.title} (${fmt.bytes(r.bytes)})`))),
      h('p', { class: 'muted' }, 'Files that are in use are skipped. The Recycle Bin cannot be restored.'));
    const ok = await new Promise((resolve) => {
      let res = false;
      const m = ui.modal({ title: 'Clean junk files', body, onClose: () => resolve(res) });
      m.setActions([ui.button({ label: 'Cancel', onClick: () => m.close() }), ui.button({ label: 'Clean', kind: 'danger', onClick: () => { res = true; m.close(); } })]);
    });
    if (!ok) return false;
    busy = true; syncButtons(); setT(totalL, 'Cleaning…');
    const byId = new Map(todo.map((r) => [r.c.id, r]));
    let un1, un2, jobId, done;
    const finished = new Promise((r) => { done = r; });
    const early = [];
    const onP = (p) => { if (jobId && p.job !== jobId) return; const r = byId.get(p.id); if (r) r.bar.set(p.total_bytes ? (p.done_bytes / p.total_bytes) * 100 : 0, 'var(--teal)'); };
    const onD = (p) => { if (!jobId) { early.push(p); return; } if (p.job === jobId) done(p); };
    un1 = await api.listen('clean:progress', onP); un2 = await api.listen('clean:done', onD);
    try {
      jobId = (await api.invoke('clean_start', { ids: todo.map((r) => r.c.id) })).job;
      const e = early.find((p) => p.job === jobId); if (e) done(e);
      const res = await finished;
      let failed = 0;
      for (const x of res.results) {
        const r = byId.get(x.id); if (!r) continue;
        failed += x.failed || 0; r.bytes = 0;
        r.el.classList.add('done'); r.bar.set(100, 'var(--ok)'); r.size.className = 'sz done'; setT(r.size, `Cleaned ${fmt.bytes(x.freed)}`);
      }
      setT(totalL, `✓ Freed ${fmt.bytes(res.total_freed)}${failed ? ` · ${failed} item${failed > 1 ? 's' : ''} in use skipped` : ''}`);
      totalL.style.color = 'var(--ok-text)';
      ctx.toast(`Cleaned ${fmt.bytes(res.total_freed)} of junk`, 'ok');
      return true;
    } catch (e) { setT(totalL, 'Cleaning failed'); ctx.toast(e.message, 'bad'); return false; } finally {
      un1?.(); un2?.(); busy = false; syncButtons();
    }
  }
  ctx.onCleanup(() => {});

  async function boostAll() {
    if (rows.some((r) => r.chk.checked)) { if (!(await clean())) return; }
    boostRam();
  }

  const head = ui.pageHead({ title: 'Optimizer', sub: 'Free up memory and clear junk files in one click' });
  void icon;
  return [head, h('div', { class: 'opt-split' }, memCard, junkCard)];
}
