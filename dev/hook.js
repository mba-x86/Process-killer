// Smoke-test hook (module, injected before js/main.js). Wraps the api object every page uses so scenarios can count,
// override and transform backend calls, and exposes the mock emitter. NOT shipped.
import { api } from '/js/api.js';
import * as mk from '/js/mock.js';

const pk = window.__pk;
const pre = window.parent.__smokePre || {};
Object.assign(pk.override, pre.override || {});
Object.assign(pk.transform, pre.transform || {});
pk.transformAll = pre.transformAll || null;
pk.emit = mk.emit;
pk.mockInvoke = mk.mockInvoke;

const origInvoke = api.invoke, origListen = api.listen;
const clone = (v) => { try { return JSON.parse(JSON.stringify(v)); } catch { return null; } };

api.invoke = async (cmd, args = {}) => {
  pk.log.push({ cmd, args: clone(args) });
  const orig = () => origInvoke(cmd, args);
  let res = pk.override[cmd] ? await pk.override[cmd](args, orig) : await orig();
  if (pk.transform[cmd]) res = pk.transform[cmd](res, args) ?? res;
  if (pk.transformAll) res = pk.transformAll(cmd, res, args) ?? res;
  return res;
};
api.listen = async (event, cb) => {
  const un = await origListen(event, cb);
  pk.lis[event] = (pk.lis[event] || 0) + 1;
  let done = false;
  return () => { if (!done) { done = true; pk.lis[event]--; } un(); };
};
