// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// App-wide update flow. One state machine, one banner; the About page subscribes to the same state.
//   phase: idle | checking | uptodate | available | error | downloading | installing | failed
import { api } from './api.js';
import { h } from './dom.js';
import { confirmDialog, toast } from './ui.js';

let ctx = null, listening = false, job = null, banner = null;
const subs = new Set();
export const state = { phase: 'idle', info: null, error: '', done: 0, total: 0, checked: false };

const mb = (n) => `${(n / 1048576).toFixed(1)} MB`;
const set = (patch) => { Object.assign(state, patch); subs.forEach((fn) => fn(state)); };
/** subscribe(fn) -> unsubscribe. fn(state) is called on every change. */
export function subscribe(fn) { subs.add(fn); return () => subs.delete(fn); }
/** Give this module the shell context (idempotent). */
export function bind(c) { ctx ||= c; }

/* ---------- banner (single, id 'update') ---------- */
function showBanner(text, actions = [], { progress = false } = {}) {
  ctx.banners.show({ id: 'update', kind: 'info', text, actions });
  const host = document.querySelector('.banners');
  const el = host?.lastElementChild;
  const span = el?.firstElementChild;
  banner = null;
  if (!el || !span) return;
  const label = h('span', {}, text);
  if (progress) {
    const fill = h('i', { style: { width: '0%' } });
    const track = h('div', { class: 'bar', style: { height: '4px', width: 'min(360px, 100%)', background: 'rgba(255,255,255,.12)' } }, fill);
    fill.style.background = 'var(--blue)';
    span.replaceChildren(label, track);
    Object.assign(span.style, { display: 'flex', flexDirection: 'column', gap: '6px', flex: '1 1 260px' });
    banner = { label, fill };
  } else span.replaceChildren(label);
}
const hideBanner = () => { banner = null; ctx?.banners.hide('update'); };

function paintProgress() {
  if (!banner) return;
  const { done, total } = state;
  banner.label.textContent = total ? `Downloading… ${mb(done)} of ${mb(total)}` : `Downloading… ${mb(done)}`;
  banner.fill.style.width = total ? `${Math.min(100, (done / total) * 100).toFixed(1)}%` : '100%';
}

function availableBanner() {
  const v = state.info.version;
  showBanner(`Version ${v} is available`, [
    { label: 'Update now', kind: 'primary', onClick: () => startUpdate() },
    { label: "What's new", onClick: () => ctx.navigate('about') },
    { label: 'Later', onClick: hideBanner },
  ]);
}

/* ---------- events ---------- */
async function listen() {
  if (listening) return;
  listening = true;
  await api.listen('update:progress', (p) => {
    if (state.phase !== 'downloading' || (job && p.job && p.job !== job)) return;
    state.done = p.done || 0; state.total = p.total || 0;
    paintProgress();
    subs.forEach((fn) => fn(state));
  });
  await api.listen('update:done', (p) => {
    if (state.phase !== 'downloading' || (job && p.job && p.job !== job)) return;
    if (p.ok) {
      set({ phase: 'installing' });
      showBanner('Installing… the app will restart in a moment');
    } else {
      set({ phase: 'failed', error: p.message || 'unknown error' });
      showBanner(`Update failed: ${state.error}`, [
        { label: 'Try again', kind: 'primary', onClick: () => startUpdate() },
        { label: 'Later', onClick: hideBanner },
      ]);
    }
  });
}

/* ---------- public API ---------- */
/** Ask the backend for a newer version. Quiet unless {manual}. Never throws. */
export async function checkNow({ manual = false } = {}) {
  if (['checking', 'downloading', 'installing'].includes(state.phase)) return state;
  set({ phase: 'checking', error: '' });
  try {
    const info = await api.invoke('check_update');
    state.checked = true;
    if (info.available) { set({ phase: 'available', info }); availableBanner(); }
    else { set({ phase: 'uptodate', info }); hideBanner(); }
  } catch (e) {
    state.checked = true;
    set({ phase: 'error', error: e.message || String(e) });
    if (manual) toast(state.error, 'bad');
  }
  return state;
}

/** Confirm, then download + install. Shared by the banner and the About page. */
export async function startUpdate() {
  if (!state.info?.available || ['downloading', 'installing'].includes(state.phase)) return;
  const v = state.info.version;
  const ok = await confirmDialog({ title: 'Install update', text: `Download and install version ${v}? The app will close and reopen automatically.`, confirmLabel: 'Update now' });
  if (!ok) return;
  await listen();
  job = null;
  set({ phase: 'downloading', done: 0, total: state.info.size || 0, error: '' });
  showBanner(`Downloading version ${v}…`, [], { progress: true });
  paintProgress();
  try { job = (await api.invoke('start_update')).job; }
  catch (e) {
    set({ phase: 'failed', error: e.message || String(e) });
    showBanner(`Update failed: ${state.error}`, [{ label: 'Try again', kind: 'primary', onClick: () => startUpdate() }, { label: 'Later', onClick: hideBanner }]);
  }
}

/** Called by js/main.js 4 s after launch. */
export async function initUpdates(c) {
  bind(c);
  try {
    const s = await api.invoke('get_settings');
    if (s.auto_check) await checkNow({ manual: false });
  } catch { /* ignore: never bother the user at startup */ }
}
