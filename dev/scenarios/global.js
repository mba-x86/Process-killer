const ROUTES = ['dashboard', 'performance/processor', 'performance/memory', 'performance/network', 'performance/graphics', 'applications', 'services', 'processes', 'startup', 'optimizer', 'storage', 'activity', 'about'];
const XSS = '<img src=x onerror=alert(1)>';
const KEYS = /name|display|publisher|title|path|command|message|desc|detail|label|current|manufacturer|model|ssid|vendor|driver|exe|location|text|notes|part_number|slot|hostname|os$/i;

function taint(v, key = '') {
  if (typeof v === 'string') return KEYS.test(key) && v ? v + XSS : v;
  if (Array.isArray(v)) return v.map((x) => taint(x, key));
  if (v && typeof v === 'object') { const o = {}; for (const k of Object.keys(v)) o[k] = taint(v[k], k); return o; }
  return v;
}

export default [
  { name: 'every route renders without errors', async run(T) {
    for (const r of ROUTES) await T.step(r, async () => { await T.open(r); await T.sleep(2500); T.assert(T.count('#page h1') >= 1, 'no h1'); T.lacks(T.text('#page'), 'failed to load'); });
  } },

  { name: 'XSS: hostile names render as text on every page', async run(T) {
    for (const r of ROUTES) {
      await T.step(r, async () => {
        await T.open(r, 'autoscan', { transformAll: (cmd, res) => taint(res) });
        await T.sleep(2200);
        if (r === 'applications') { await T.waitFor(() => T.count('.apps-tbl tr[data-id]') > 5); await T.click(T.$('.apps-tbl tr[data-id]')); await T.sleep(800); await T.clickText('Uninstall safely').catch(() => {}); await T.sleep(800); }
        if (r === 'storage') await T.sleep(3000);
        if (r === 'services' || r === 'processes') await T.click(T.$('#page tbody tr.click'));
        if (r === 'optimizer') { await T.clickText('Scan'); await T.sleep(500); }
        T.eq(T.count('img'), 0, 'an <img> element was injected');
        T.eq(T.count('[onerror]'), 0, 'an onerror attribute was injected');
        T.eq(T.pk.alerts.length, 0, 'alert fired');
      });
    }
  } },

  { name: 'timers stop when leaving a page (intervals + backend polling)', async run(T) {
    await T.open('activity');
    await T.sleep(300);
    const base = T.intervals();
    for (const r of ROUTES.filter((x) => x !== 'activity' && x !== 'about')) {
      await T.step(`${r} -> activity`, async () => {
        await T.nav(r); await T.sleep(1500);
        await T.nav('activity'); await T.sleep(300);
        T.eq(T.intervals(), base, 'intervals still running after leaving');
        const m = T.mark(); await T.sleep(6500);
        const polled = T.pk.log.slice(m).filter((c) => /^(get_live|get_top_|get_drives|list_processes|get_net_detail|get_gpu_detail|get_mem_detail|get_cpu_detail)/.test(c.cmd));
        T.eq(polled.length, 0, `polling continued after leaving: ${polled.map((c) => c.cmd).join(',')}`);
      });
    }
  } },

  { name: 'fast navigation does not leak timers from the abandoned page', async run(T) {
    await T.open('activity');
    const base = T.intervals();
    for (const r of ['dashboard', 'memory', 'network', 'processes', 'graphics', 'processor']) {
      await T.step(`${r} then immediately activity`, async () => {
        T.win.location.hash = `#/${r}`; await T.sleep(2);
        T.win.location.hash = '#/activity';
        await T.waitFor(() => T.title() === 'Activity', { msg: 'activity page' });
        await T.sleep(1500);
        T.eq(T.intervals(), base, 'abandoned page left intervals running');
      });
    }
  } },

  { name: 'no horizontal overflow at 1100x700 and 1440x900', async run(T) {
    for (const [w, h] of [[1100, 700], [1440, 900]]) {
      for (const r of ROUTES) {
        await T.step(`${r} ${w}x${h}`, async () => {
          await T.open(r, r === 'storage' ? 'autoscan' : '', { w, h });
          await T.sleep(r === 'storage' ? 5000 : 2200);
          const bad = [];
          for (const el of [T.doc.documentElement, T.doc.body, T.$('.main'), T.$('#page')]) if (el && el.scrollWidth > el.clientWidth + 1) bad.push(`${el.id || el.className || el.tagName} ${el.scrollWidth}>${el.clientWidth}`);
          T.eq(bad.join('; '), '', 'horizontal overflow');
        });
      }
    }
  } },

  { name: 'keyboard: nav + primary buttons focusable with visible focus; Escape closes modals', async run(T) {
    await T.open('dashboard');
    await T.step('nav links', async () => {
      const links = T.$$('a.nav'); T.eq(links.length, 10);
      T.assert(links.every((a) => a.tabIndex >= 0 && a.getAttribute('href')), 'nav links must be tabbable anchors');
      links[3].focus(); T.eq(T.doc.activeElement, links[3], 'focus() works');
      T.assert(T.win.getComputedStyle(links[3]).outlineStyle !== 'none', 'focus outline visible on nav');
    });
    await T.step('primary buttons', async () => {
      for (const r of ['dashboard', 'optimizer', 'storage', 'about']) {
        await T.nav(r);
        await T.waitFor(() => T.$$('#page .btn-p').some((x) => !x.disabled), { msg: `${r}: enabled primary button` });
        const b = T.$$('#page .btn-p').filter((x) => !x.disabled);
        T.assert(b.length > 0, `${r}: has a primary button`);
        b[0].focus(); T.eq(T.doc.activeElement, b[0], `${r}: primary focusable`);
        T.assert(T.win.getComputedStyle(b[0]).outlineStyle !== 'none', `${r}: focus ring on primary button`);
      }
    });
    await T.step('Escape closes confirm dialogs', async () => {
      await T.open('startup'); await T.waitFor(() => T.count('#page tbody tr.click') > 3);
      await T.click(T.$('#page tbody tr.click')); await T.clickText('Remove selected');
      await T.waitFor(() => T.modal()); await T.key('Escape'); T.assert(!T.modal(), 'dialog still open after Escape');
    });
    await T.step('modal moves focus inside', async () => {
      await T.click(T.$('#page tbody tr.click')); await T.clickText('Remove selected'); await T.waitFor(() => T.modal()); await T.sleep(100);
      T.assert(T.modal().contains(T.doc.activeElement), 'focus should move into the dialog (keyboard users otherwise tab behind it)');
      await T.key('Escape');
    });
  } },
];
