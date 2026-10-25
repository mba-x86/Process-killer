// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Performance: Processor / Memory / Network / Graphics as tabs of ONE page. Switching a tab never reloads the shell or the sidebar:
// the previous tab's timers are stopped, the next page module is loaded (first time only) and fades in. The URL follows the tab
// (#/performance/memory) so reload and links open the same tab; the old routes (#/memory ...) still work and land here.
const TABS = [
  { id: 'processor', label: 'Processor', icon: 'processor', load: () => import('./processor.js') },
  { id: 'memory', label: 'Memory', icon: 'memory', load: () => import('./memory.js') },
  { id: 'network', label: 'Network', icon: 'network', load: () => import('./network.js') },
  { id: 'graphics', label: 'Graphics', icon: 'graphics', load: () => import('./graphics.js') },
];
const KEY = 'tr.performance.tab';
const store = {
  get() { try { return localStorage.getItem(KEY); } catch { return null; } },
  set(v) { try { localStorage.setItem(KEY, v); } catch { /* private window etc. */ } },
};

export default async function (ctx) {
  ctx.css('css/pages/performance.css');
  const { h, icon, setChildren } = ctx;

  const seg = location.hash.replace(/^#\//, '').split(/[/?]/);                    // ['performance','memory'] | ['memory'] | ['performance']
  const fromUrl = TABS.find((t) => t.id === seg[1]) || TABS.find((t) => t.id === seg[0]);
  let active = (fromUrl || TABS.find((t) => t.id === store.get()) || TABS[0]).id;

  const body = h('div', { class: 'ptab-body', role: 'tabpanel' });
  const marker = h('i', { class: 'ptab-mark', 'aria-hidden': 'true' });
  const btns = new Map();
  const bar = h('div', { class: 'ptabs', role: 'tablist', 'aria-label': 'Performance' }, marker, TABS.map((t) => {
    const b = h('button', { type: 'button', role: 'tab', id: `ptab-${t.id}`, class: 'ptab', 'aria-controls': 'ptab-panel',
      onclick: () => select(t.id), onkeydown: (e) => keys(e, t.id) }, icon(t.icon, 15), t.label);
    btns.set(t.id, b);
    return b;
  }));
  body.id = 'ptab-panel';

  let local = [], seq = 0, running = false;
  const stopLocal = () => { const fns = local; local = []; fns.forEach((fn) => { try { fn(); } catch { /* ignore */ } }); };
  ctx.onCleanup(() => { seq++; stopLocal(); });

  function moveMarker() {
    const b = btns.get(active);
    if (!b || !b.offsetWidth) return;
    marker.style.width = `${b.offsetWidth}px`;
    marker.style.transform = `translateX(${b.offsetLeft}px)`;
  }
  function keys(e, id) {
    const i = TABS.findIndex((t) => t.id === id), n = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: TABS.length - 1 }[e.key];
    if (n == null) return;
    e.preventDefault();
    const t = TABS[(n + TABS.length) % TABS.length];
    select(t.id); btns.get(t.id).focus();
  }

  /** Sub-context for the tab's page: timers and cleanups belong to the TAB, so switching tabs stops them at once. */
  function tabCtx() {
    return Object.assign(Object.create(ctx), {
      embedded: true,
      every: (ms, fn, o) => { const stop = ctx.every(ms, fn, o); local.push(stop); return stop; },
      onCleanup: (fn) => { local.push(fn); },
    });
  }

  async function select(id, first = false) {
    if (!first && id === active && running) return;
    active = id; store.set(id);
    btns.forEach((b, k) => { const on = k === id; b.classList.toggle('on', on); b.setAttribute('aria-selected', String(on)); b.tabIndex = on ? 0 : -1; });
    moveMarker();
    const tab = TABS.find((t) => t.id === id), mine = ++seq;
    try { history.replaceState(null, '', `#/performance/${id}`); } catch { /* ignore */ }
    document.title = `${tab.label} — Task Reaper`;
    body.setAttribute('aria-labelledby', `ptab-${id}`);
    running = true;
    stopLocal();                                                           // old tab stops updating; its content stays until the new one is ready
    try {
      const mod = await tab.load();
      if (mine !== seq) return;
      const res = await mod.default(tabCtx());
      if (mine !== seq) { res?.destroy?.(); return; }
      setChildren(body, res instanceof Node || Array.isArray(res) ? res : res?.el);
      body.classList.remove('swap'); void body.offsetWidth; body.classList.add('swap');
    } catch (e) {
      if (mine !== seq) return;
      console.error(e);
      setChildren(body, h('div', { class: 'card card-pad col' }, h('h2', {}, `${tab.label} failed to load`), h('p', { class: 'muted' }, e.message || String(e))));
    }
  }

  // the first tab is rendered before the page is handed back, so the window never shows an empty page
  await select(active, true);
  requestAnimationFrame(moveMarker);
  if (document.fonts?.ready) document.fonts.ready.then(moveMarker);
  const onResize = () => moveMarker();
  addEventListener('resize', onResize);
  ctx.onCleanup(() => removeEventListener('resize', onResize));
  return [bar, body];
}
