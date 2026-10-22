// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Browser-only fake backend. Each page owns ui/mock/<page>.js and registers handlers:
//     import { mock } from '../js/mock.js';   mock('get_live', () => ({ ... }));
// The SHAPE of these objects is the contract the Rust commands must implement (see docs/API.md).
const handlers = new Map();
const listeners = new Map();

export function mock(cmd, fn) { handlers.set(cmd, fn); }
export function emit(event, payload) { (listeners.get(event) || []).forEach((cb) => cb(payload)); }
export const wait = (ms) => new Promise((r) => setTimeout(r, ms));
/** Gentle random walk, handy for live numbers. */
export const walk = (v, step = 3, lo = 0, hi = 100) => Math.max(lo, Math.min(hi, v + (Math.random() - 0.5) * step * 2));

let loaded;
async function loadAll() {
  loaded ||= import('../mock/index.js');   // each page module registers its commands via mock()
  await loaded;
}

export async function mockInvoke(cmd, args) {
  await loadAll();
  const fn = handlers.get(cmd);
  if (!fn) throw new Error(`mock: no handler for "${cmd}"`);
  await wait(30 + Math.random() * 60);      // feel like IPC, catches loading-state bugs
  return structuredClone(await fn(args || {}));
}

export async function mockListen(event, cb) {
  const arr = listeners.get(event) || [];
  arr.push(cb); listeners.set(event, arr);
  return () => listeners.set(event, (listeners.get(event) || []).filter((x) => x !== cb));
}
