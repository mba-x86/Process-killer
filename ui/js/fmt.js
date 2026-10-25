// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Formatting helpers. All sizes use 1024 steps (matches the old app and Windows Explorer).
export const DASH = '—';

export function bytes(n, dp = 1) {
  if (n == null || isNaN(n)) return DASH;
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0, v = Math.abs(n);
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  const s = i === 0 ? String(Math.round(v)) : v.toFixed(i >= 3 ? dp : (v >= 100 ? 0 : dp)).replace(/\.0$/, '');
  return `${n < 0 ? '-' : ''}${s} ${u[i]}`;
}
/** Same units, but ALWAYS `dp` decimals from KB up ("68.0 GB", "704.0 MB") - the dashboard/drives/top-apps style in the design. */
export function bytesFixed(n, dp = 1) {
  if (n == null || isNaN(n)) return DASH;
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0, v = Math.abs(n);
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${n < 0 ? '-' : ''}${i === 0 ? Math.round(v) : v.toFixed(dp)} ${u[i]}`;
}
export const rate = (bps) => (bps == null ? DASH : `${bytes(bps)}/s`);
export const pct = (v, dp = 0) => (v == null ? DASH : `${Number(v).toFixed(dp)}%`);
export const ghz = (mhz) => (mhz ? `${(mhz / 1000).toFixed(2)} GHz` : DASH);
export const num = (n) => (n == null ? DASH : Number(n).toLocaleString('en-US'));

export function uptime(secs) {
  if (secs == null) return DASH;
  const d = Math.floor(secs / 86400), h = Math.floor((secs % 86400) / 3600), m = Math.floor((secs % 3600) / 60);
  return (d ? `${d}d ` : '') + `${h}h ${m}m`;
}
/** ISO / unix-seconds -> 2026-04-07 */
export function date(v) {
  if (!v) return DASH;
  const d = typeof v === 'number' ? new Date(v * 1000) : new Date(v);
  return isNaN(d) ? String(v) : d.toISOString().slice(0, 10);
}
export const clamp = (v, lo = 0, hi = 100) => Math.max(lo, Math.min(hi, v));

/** Colour for a usage percentage (green -> amber -> red) using the design tokens. */
export function usageColor(p, base = 'var(--teal)') { return p >= 90 ? 'var(--bad)' : p >= 75 ? 'var(--amber)' : base; }
