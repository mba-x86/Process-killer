// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Dashboard: health banner, 4 live cards, system specifications, drives, top apps. Built once, updated in place.
import { setText } from '../js/dom.js';

const TONE_COLOR = { ok: 'var(--ok)', warn: 'var(--amber)', bad: 'var(--bad)' };

export default async function (ctx) {
  const { h, ui, fmt, charts, api } = ctx;
  const st = { live: null, drives: [], uptime0: 0, t0: Date.now(), dx: null, sections: [] };

  /* ---------- header ---------- */
  const head = ui.pageHead({ title: 'Dashboard', sub: ' ' });
  const ov = await ctx.overview().catch(() => null);
  if (ov) { st.uptime0 = ov.uptime_secs || 0; st.t0 = Date.now(); }
  const upNow = () => st.uptime0 + (Date.now() - st.t0) / 1000;
  const paintSub = () => ov && head.setSub([ov.hostname, ov.os, ov.model, `up ${fmt.uptime(upNow())}`].filter(Boolean).join(' · '));
  paintSub();
  ctx.every(30000, paintSub, { immediate: false });

  /* ---------- health banner ---------- */
  const healthDot = h('span', { style: { width: '8px', height: '8px', borderRadius: '50%', flex: 'none' } });
  const healthText = h('span', {}, 'Checking…');
  const healthMsg = h('span', { style: { display: 'inline-flex', alignItems: 'center', gap: '10px', fontSize: '14px', fontWeight: 600 } }, healthDot, healthText);
  const boostBtn = ui.button({ label: 'Boost RAM', icon: 'bolt', kind: 'primary', onClick: boost });
  const health = ui.card({ class: 'row between wrap', style: { gap: '12px', padding: '12px 12px 12px 18px' } }, healthMsg,
    h('div', { class: 'row wrap' }, boostBtn,
      ui.button({ label: 'Clean junk', icon: 'trash', onClick: () => ctx.navigate('optimizer') }),
      ui.button({ label: 'Analyze storage', icon: 'bars', onClick: () => ctx.navigate('storage') })));

  function paintHealth() {
    const issues = [];
    const l = st.live;
    if (l) {
      if (l.mem.percent >= 85) issues.push(`Memory is ${Math.round(l.mem.percent)}% full`);
      if (l.cpu.percent >= 90) issues.push(`CPU is at ${Math.round(l.cpu.percent)}%`);
    }
    for (const d of st.drives) if (d.percent >= 90) issues.push(`Drive ${d.mount.replace(/\\$/, '')} is ${Math.round(d.percent)}% full`);
    if (upNow() >= 14 * 86400) issues.push(`PC has been on for ${Math.floor(upNow() / 86400)} days - restart recommended`);
    const bad = issues.length > 0, col = bad ? 'var(--amber)' : 'var(--ok)';
    healthDot.style.background = col;
    healthDot.style.boxShadow = `0 0 0 4px ${bad ? 'rgba(244,183,64,.15)' : 'rgba(69,201,138,.15)'}`;
    healthText.style.color = bad ? 'var(--amber)' : 'var(--ok-text)';
    setText(healthText, bad ? `Needs attention: ${issues.join(' · ')}` : 'Your PC is running well');
  }
  async function boost() {
    boostBtn.disabled = true;
    try {
      const r = await api.invoke('boost_ram');
      ctx.toast(`RAM boosted - ${fmt.bytes(r.freed)} freed`, 'ok');
    } catch (e) { ctx.toast(e.message || String(e), 'bad'); } finally { boostBtn.disabled = false; }
  }

  /* ---------- live cards ---------- */
  const cardShell = (label, route, aria, ...kids) => ui.card({ style: { padding: '16px 18px', display: 'flex', flexDirection: 'column', gap: '12px' } },
    h('a', { class: 'card-link', href: `#/${route}`, 'aria-label': aria }),
    h('span', { class: 'lbl row between' }, label, h('span', { class: 'more', style: { textTransform: 'none' } }, 'Details ›')), ...kids);
  const stack = (...kids) => h('div', { class: 'col', style: { gap: '4px' } }, ...kids);
  const bigTxt = () => h('span', { class: 'big' });
  const dimTxt = () => h('span', { class: 'muted' });

  const cpuRing = charts.ring({ color: 'var(--blue)' }), cpuBig = bigTxt(), cpuSub = dimTxt();
  const memRing = charts.ring({ color: 'var(--teal)' }), memBig = bigTxt(), memOf = dimTxt(), memFree = dimTxt();
  const dskRing = charts.ring({ color: 'var(--teal)' }), dskBig = bigTxt(), dskSub = dimTxt(), dskLbl = h('span', {}, 'SYSTEM DRIVE');
  const arrowDown = h('span', { style: { color: 'var(--accent-2)' } }, '↓'), netDown = h('span'), netUp = dimTxt();
  netUp.className = 'muted tnum';
  const netHist = charts.history(60), netSpark = charts.sparkline({ w: 200, h: 36, color: 'var(--accent-2)', max: 1024 });
  const cards = h('section', { class: 'grid-auto', 'aria-label': 'Live usage', style: { '--min': '230px' } },
    cardShell('PROCESSOR', 'performance/processor', 'Open processor details', h('div', { class: 'row', style: { gap: '18px' } }, cpuRing, stack(cpuBig, cpuSub))),
    cardShell('MEMORY', 'performance/memory', 'Open memory details', h('div', { class: 'row', style: { gap: '18px' } }, memRing, stack(memBig, memOf, memFree))),
    cardShell(dskLbl, 'storage', 'Open storage analyzer', h('div', { class: 'row', style: { gap: '18px' } }, dskRing, stack(dskBig, dskSub))),
    cardShell('NETWORK', 'performance/network', 'Open network details', stack(h('span', { class: 'big tnum' }, arrowDown, ' ', netDown), netUp), netSpark));

  function paintLive(l) {
    st.live = l;
    const c = l.cpu, m = l.mem, d = l.disk_sys;
    cpuRing.set(c.percent, `${Math.round(c.percent)}%`, fmt.usageColor(c.percent, 'var(--blue)'));
    setText(cpuBig, `${c.cores}C / ${c.threads}T`); setText(cpuSub, `${fmt.ghz(c.freq_mhz)} now`);
    memRing.set(m.percent, `${Math.round(m.percent)}%`, fmt.usageColor(m.percent));
    setText(memBig, fmt.bytesFixed(m.used)); setText(memOf, `of ${fmt.bytesFixed(m.total)}`); setText(memFree, `${fmt.bytesFixed(m.available)} free`);
    dskRing.set(d.percent, `${Math.round(d.percent)}%`, fmt.usageColor(d.percent));
    setText(dskLbl, `SYSTEM DRIVE (${d.mount.replace(/\\$/, '')})`);
    setText(dskBig, `${fmt.bytesFixed(d.free)} free`); setText(dskSub, `${fmt.bytesFixed(d.used)} used of ${fmt.bytesFixed(d.total)}`);
    setText(netDown, fmt.rate(l.net.down_bps)); setText(netUp, `↑ ${fmt.rate(l.net.up_bps)}`);
    netHist.push(l.net.down_bps);
    netSpark.update(netHist.values, Math.max(1024, ...netHist.values));
    paintHealth();
  }

  /* ---------- system specifications ---------- */
  const specBody = h('div', { class: 'col', style: { gap: '22px', flex: 1, minHeight: 0, maxHeight: '560px', overflowY: 'auto', padding: '18px 20px 20px' } });
  const updating = h('span', { class: 'muted', style: { fontSize: '12px', display: 'none' } }, 'Updating…');
  const ddEls = [];

  function renderSpecs(sections) {
    st.sections = sections;
    const top = specBody.scrollTop;
    ddEls.length = 0;
    specBody.replaceChildren(...sections.map((s) => {
      const rows = s.title === 'Graphics' ? [...s.rows.filter((r) => r[0] !== 'DirectX'), ['DirectX', st.dx || 'Detecting…']] : s.rows;
      const dl = h('dl', { class: 'spec' });
      for (const [k, v, tone] of rows) {
        const dd = h('dd', { style: tone ? { display: 'inline-flex', alignItems: 'center', gap: '6px', color: tone === 'ok' ? 'var(--ok-text)' : tone === 'warn' ? 'var(--amber)' : 'var(--bad-text)' } : null },
          tone ? h('span', { style: { width: '6px', height: '6px', borderRadius: '50%', background: TONE_COLOR[tone] || TONE_COLOR.ok, flex: 'none' } }) : null, v ?? '—');
        if (k === 'DirectX') ddEls.push(dd);
        dl.append(h('dt', {}, k), dd);
      }
      return h('div', { class: 'col', style: { gap: '10px' } }, h('h3', { class: 'lbl lbl-blue' }, s.title.toUpperCase()), dl);
    }));
    specBody.scrollTop = top;
  }
  function reportText() {
    return st.sections.map((s) => `${s.title.toUpperCase()}\n${s.rows.map(([k, v]) => `  ${k}: ${v}`).join('\n')}`).join('\n\n');
  }
  async function copyReport() {
    const text = reportText();
    try { await api.invoke('copy_text', { text }); } catch { try { await navigator.clipboard.writeText(text); } catch { ctx.toast('Could not copy the report', 'bad'); return; } }
    ctx.toast('System report copied to clipboard', 'ok');
  }

  let alive = true; ctx.onCleanup(() => { alive = false; });
  let gen = 0;
  async function loadSpecs() {
    const my = ++gen;
    updating.style.display = '';
    try { const f = await api.invoke('get_specs_fast'); if (alive && my === gen) renderSpecs(f.sections); } catch (e) { console.error(e); }
    try { const f = await api.invoke('get_specs_full'); if (alive && my === gen) renderSpecs(f.sections); } catch (e) { console.error(e); }
    if (alive && my === gen) updating.style.display = 'none';
  }
  async function loadDx(force) {
    if (st.dx && !force) return;
    ddEls.forEach((d) => { d.textContent = 'Detecting…'; });
    try { const r = await api.invoke('get_directx'); st.dx = r.text || 'Unknown'; } catch { st.dx = 'Unavailable'; }
    if (alive) ddEls.forEach((d) => { d.textContent = st.dx; });
  }
  const refresh = () => { st.dx = null; loadSpecs(); loadDx(true); };
  const specCard = ui.card({ style: { flex: '999 1 560px', minWidth: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden' } },
    h('div', { class: 'card-head wrap' }, h('h2', {}, 'System specifications'),
      h('div', { class: 'row' }, updating, ui.button({ label: 'Copy report', icon: 'copy', onClick: copyReport }), ui.button({ label: 'Refresh', icon: 'refresh', onClick: refresh }))),
    specBody);

  /* ---------- drives + top apps ---------- */
  const drivesBox = h('div', { class: 'col', style: { gap: '14px' } });
  const driveRows = new Map();
  function paintDrives(list) {
    st.drives = list;
    const keep = new Set(list.map((d) => d.mount));
    for (const [k, r] of driveRows) if (!keep.has(k)) { r.el.remove(); driveRows.delete(k); }
    for (const d of list) {
      let r = driveRows.get(d.mount);
      if (!r) {
        const txt = h('span', { class: 'tnum', style: { color: 'var(--muted-2)' } });
        const bar = charts.bar(d.percent);
        r = { txt, bar, el: h('div', { class: 'col', style: { gap: '7px' } }, h('div', { class: 'row between', style: { gap: '12px' } }, h('span', { style: { fontWeight: 600 } }, d.mount), txt), bar) };
        driveRows.set(d.mount, r); drivesBox.append(r.el);
      }
      setText(r.txt, `${fmt.bytesFixed(d.free)} free of ${fmt.bytesFixed(d.total)}`);
      r.bar.set(d.percent, fmt.usageColor(d.percent, 'var(--amber)'));
    }
    paintHealth();
  }
  const appsBody = h('tbody');
  const appsTbl = h('table', { class: 'tbl' },
    h('thead', {}, h('tr', {}, h('th', { style: { height: '32px' } }, 'App'), h('th', { class: 'num', style: { height: '32px' } }, 'Memory ↓'), h('th', { class: 'num', style: { height: '32px' } }, 'CPU'))), appsBody);
  function paintApps(list) {
    appsBody.replaceChildren(...list.map((a) => h('tr', {}, h('td', {}, a.name, a.count > 1 ? h('span', { class: 'muted' }, ` × ${a.count}`) : null),
      h('td', { class: 'num' }, fmt.bytesFixed(a.mem)), h('td', { class: 'num' }, `${Math.round(a.cpu)}%`))));
  }
  const side = ui.card({ style: { flex: '1 1 340px', minWidth: 0, padding: '18px', display: 'flex', flexDirection: 'column', gap: '22px' } },
    h('div', { class: 'col', style: { gap: '14px' } }, h('h2', {}, 'Drives'), drivesBox),
    h('div', { class: 'col', style: { gap: '10px' } }, h('h2', {}, 'Top apps by memory'), appsTbl));

  /* ---------- polling ---------- */
  ctx.every(1000, async () => paintLive(await api.invoke('get_live')));
  ctx.every(10000, async () => paintDrives(await api.invoke('get_drives')));
  ctx.every(5000, async () => paintApps(await api.invoke('get_top_apps', { by: 'memory', limit: 6 })));
  loadSpecs(); loadDx(false);

  return [head, health, cards, h('section', { class: 'split' }, specCard, side)];
}
