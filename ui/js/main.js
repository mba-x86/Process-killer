// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// App shell: sidebar, hash router, banners, page context. Pages are plain modules:
//     export default async function (ctx) { return <Node | Node[] | { el, destroy() }>; }
import { h, setChildren } from './dom.js';
import { icon, logo } from './icons.js';
import { api } from './api.js';
import * as fmt from './fmt.js';
import * as charts from './charts.js';
import * as ui from './ui.js';
import { GROUPS, ROUTES } from './routes.js';

const navEls = new Map();
const bannerEls = new Map();
const bannerHost = h('div', { class: 'banners' });
const pageEl = h('div', { class: 'page', id: 'page', tabindex: '-1' });
let cleanups = [], current = null, token = 0, overviewP;

/** Register a cleanup for page #owner. If that page was already left (a late await), run it right away instead of
 *  leaking it into the next page's lifetime. */
function reg(owner, fn) { if (owner !== token) { try { fn(); } catch { /* ignore */ } } else cleanups.push(fn); }

/* ---------------- context handed to every page ---------------- */
const ctx = {
  api, ui, fmt, charts, h, icon, setChildren, toast: ui.toast,
  navigate: (id) => { location.hash = `#/${id}`; },
  /** Run fn now and then every `ms`; paused while the window is hidden/minimised; stopped on navigation. */
  every(ms, fn, { immediate = true } = {}, owner = token) {
    let busy = false;
    const run = async () => {
      if (document.hidden || busy) return;
      busy = true;
      try { await fn(); } catch (e) { console.error(e); } finally { busy = false; }
    };
    if (immediate) run();
    const id = setInterval(run, ms);
    const stop = () => clearInterval(id);
    reg(owner, stop);
    return stop;
  },
  onCleanup: (fn) => reg(token, fn),
  /** Load a page-specific stylesheet once: ctx.css('css/pages/applications.css') (path relative to index.html). */
  css(href) { if (!document.querySelector(`link[data-pk="${href}"]`)) document.head.append(h('link', { rel: 'stylesheet', href, 'data-pk': href })); },
  /** Cached overview: { hostname, os, model, uptime_secs, is_admin, version, copyright } */
  overview: () => (overviewP ||= api.invoke('get_overview')),
  banners: {
    show({ id, kind = 'info', text, actions = [] }) {
      bannerEls.get(id)?.remove();
      const el = h('div', { class: ['banner', kind], role: 'status' }, h('span', {}, text),
        h('div', { class: 'acts' }, actions.map((a) => ui.button({ label: a.label, kind: a.kind, small: true, onClick: a.onClick }))));
      bannerEls.set(id, el); bannerHost.append(el);
    },
    hide(id) { bannerEls.get(id)?.remove(); bannerEls.delete(id); },
  },
};

/* ---------------- sidebar ---------------- */
function sidebar() {
  const groups = GROUPS.map((g) => h('div', { class: 'nav-group' }, h('div', { class: 'cat' }, g),
    ROUTES.filter((r) => r.group === g).map((r) => {
      const a = h('a', { class: 'nav', href: `#/${r.id}`, 'data-route': r.id }, icon(r.icon, 16), r.label);
      navEls.set(r.id, a);
      return a;
    })));
  const who = h('span', { class: 'who' }), ver = h('span', {}, 'Version …');
  ctx.overview().then((o) => {
    setChildren(who, h('span', { class: ['dot', !o.is_admin && 'warn'] }), o.is_admin ? 'Administrator' : 'Limited access');
    who.style.color = o.is_admin ? 'var(--ok-text)' : 'var(--amber)';
    ver.textContent = `Version ${o.version}`;
    if (!o.is_admin) {
      ctx.banners.show({ id: 'admin', kind: 'warn', text: 'Not running as Administrator — stopping services, ending protected processes and some uninstalls will fail.',
        actions: [{ label: 'Restart as Administrator', kind: 'primary', onClick: () => api.invoke('relaunch_as_admin').catch((e) => ui.toast(e.message, 'bad')) }] });
    }
  }).catch(() => {});
  return h('aside', { class: 'sidebar', 'aria-label': 'Main' }, h('div', { class: 'brand' }, logo(30), 'Task Reaper'),
    h('nav', { class: 'col', style: { '--g': '0' } }, groups), h('div', { class: 'side-foot' }, who, ver, h('span', {}, '© 2026 mba-x86')));
}

/* ---------------- router ---------------- */
function routeFromHash() {
  const id = location.hash.replace(/^#\//, '').split(/[/?]/)[0] || 'dashboard';
  return ROUTES.find((r) => r.id === id) || ROUTES[0];
}

async function show() {
  const route = routeFromHash(), my = ++token;
  cleanups.forEach((fn) => { try { fn(); } catch { /* ignore */ } });
  cleanups = [];
  current?.destroy?.(); current = null;
  navEls.forEach((el, id) => { el.classList.toggle('on', id === route.id); el.toggleAttribute('aria-current', id === route.id); });
  document.title = `${route.label} — Task Reaper`;
  try {
    const mod = await route.load();
    if (my !== token) return;                           // navigated away while loading
    const pageCtx = Object.assign(Object.create(ctx), {   // every()/onCleanup() stay bound to THIS navigation even after awaits
      every: (ms, fn, o) => (my !== token ? () => {} : ctx.every(ms, fn, o, my)),
      onCleanup: (fn) => reg(my, fn),
    });
    const res = await mod.default(pageCtx);
    if (my !== token) { res?.destroy?.(); return; }
    current = res && !(res instanceof Node) && !Array.isArray(res) ? res : null;
    setChildren(pageEl, current ? current.el : res);
    pageEl.scrollTop = 0;
    if (!window.__pkReady) { window.__pkReady = true; requestAnimationFrame(() => api.invoke("app_ready").catch(() => {})); }  // show the native window only after the first paint
  } catch (e) {
    console.error(e);
    if (my === token) setChildren(pageEl, h('div', { class: 'card card-pad col' }, h('h2', {}, 'This page failed to load'), h('p', { class: 'muted' }, e.message || String(e)),
      ui.button({ label: 'Try again', icon: 'refresh', onClick: show })));
  }
}

document.getElementById('app').append(sidebar(), h('div', { class: 'main' }, bannerHost, pageEl));
addEventListener('hashchange', show);
show();
// Startup update check lives in js/updates.js (owned by the About page work); loaded late so launch stays instant.
setTimeout(() => import('./updates.js').then((m) => m.initUpdates?.(ctx)).catch(() => {}), 4000);
