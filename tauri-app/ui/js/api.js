// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Single doorway to the Rust backend. Inside the app it calls Tauri; in a plain browser (design work, screenshots,
// automated UI tests) it falls back to ../mock/*.js. The mock code is only ever downloaded when Tauri is absent.
const tauri = globalThis.__TAURI__;
export const inTauri = !!tauri?.core?.invoke;

let mockMod;
const loadMock = () => (mockMod ||= import('./mock.js'));

/** invoke('get_live') -> Promise<json>. Errors arrive as Error(message). */
export async function invoke(cmd, args = {}) {
  if (inTauri) {
    try { return await tauri.core.invoke(cmd, args); }
    catch (e) { throw new Error(typeof e === 'string' ? e : e?.message || String(e)); }
  }
  return (await loadMock()).mockInvoke(cmd, args);
}

/** listen('scan-progress', (payload) => ...) -> Promise<unlisten()> */
export async function listen(event, cb) {
  if (inTauri) return tauri.event.listen(event, (e) => cb(e.payload));
  return (await loadMock()).mockListen(event, cb);
}

export const api = { invoke, listen, inTauri };
