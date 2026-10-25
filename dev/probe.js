// Smoke-test probe. Injected by tools/smoke.py into index.html (classic script, runs before the app). NOT shipped.
(() => {
  const pk = (window.__pk = { errors: [], resErrors: [], alerts: [], intervals: new Map(), log: [], lis: {}, override: {}, transform: {}, transformAll: null });
  const show = (a) => { try { return a instanceof Error ? (a.stack || a.message) : typeof a === 'string' ? a : JSON.stringify(a); } catch { return String(a); } };
  const ce = console.error.bind(console);
  console.error = (...a) => { pk.errors.push('console.error: ' + a.map(show).join(' ').slice(0, 400)); ce(...a); };
  window.addEventListener('error', (e) => {
    if (e.target && e.target !== window && e.target.tagName) pk.resErrors.push(`resource failed: <${e.target.tagName.toLowerCase()}> ${e.target.src || e.target.href || ''}`);
    else pk.errors.push(`uncaught: ${e.message} (${(e.filename || '').split('/').pop()}:${e.lineno})`);
  }, true);
  window.addEventListener('unhandledrejection', (e) => pk.errors.push('unhandled rejection: ' + show(e.reason).slice(0, 400)));
  window.alert = (m) => { pk.alerts.push(String(m)); };
  window.prompt = (m) => { pk.alerts.push('prompt:' + m); return null; };
  const si = window.setInterval.bind(window), ci = window.clearInterval.bind(window);
  window.setInterval = (fn, ms, ...a) => { const id = si(fn, ms, ...a); pk.intervals.set(id, ms); return id; };
  window.clearInterval = (id) => { pk.intervals.delete(id); ci(id); };
})();
