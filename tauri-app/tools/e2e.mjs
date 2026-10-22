// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Contract test against the REAL backend: launches the built exe, calls every read-only command inside its web view
// (WebView2 remote-debugging port + Node's built-in WebSocket), and compares the JSON SHAPE of each response with the
// shape the UI's mock returns (the mock = what the UI was written against).
//
//   node tauri-app/tools/e2e.mjs path\to\ProcessKiller.exe [--keep]
//
// Build a prompt-free exe first:  set PK_NO_ADMIN_MANIFEST=1 && cargo build --release   (never ship that build)
import { spawn, execSync } from 'node:child_process';

const exe = process.argv[2];
const keep = process.argv.includes('--keep');
if (!exe) { console.error('usage: node e2e.mjs <exe> [--keep]'); process.exit(2); }
const PORT = 9333;

const child = spawn(exe, [], { env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` }, stdio: 'ignore', detached: false });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const kill = () => { try { execSync(`taskkill /PID ${child.pid} /T /F`, { stdio: 'ignore' }); } catch { /* already gone */ } };

async function target() {
  for (let i = 0; i < 120; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const t = list.find((x) => x.type === 'page' && /tauri|localhost/.test(x.url));
      if (t) return t;
    } catch { /* not up yet */ }
    await sleep(250);
  }
  throw new Error('web view debug target never appeared (app crashed? WebView2 missing?)');
}

// --- code that runs INSIDE the app page ---
const PAGE_SCRIPT = `(async () => {
  const mod = await import('/js/mock.js');
  const inv = (c, a) => window.__TAURI__.core.invoke(c, a || {});
  const shape = (v) => v === null || v === undefined ? 'null' : Array.isArray(v) ? [v.length ? shape(v[0]) : 'empty']
    : typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, shape(v[k])])) : typeof v;
  const out = [];
  const check = async (cmd, args, slow) => {
    const t0 = performance.now();
    let real, mock, err = null;
    try { real = await inv(cmd, args); } catch (e) { err = String(e && e.message || e); }
    try { mock = await mod.mockInvoke(cmd, args || {}); } catch (e) { mock = undefined; }
    out.push({ cmd, args, ms: Math.round(performance.now() - t0), err, real: err ? null : shape(real), mock: shape(mock), sample: err ? null : JSON.stringify(real).slice(0, 160) });
    return real;
  };
  await check('get_overview'); await check('get_live'); await check('get_drives');
  await check('get_top_processes', { by: 'memory', limit: 6 }); await check('get_top_apps', { by: 'cpu', limit: 6 });
  await check('get_specs_fast'); await check('get_cpu_detail'); await check('get_mem_detail'); await check('get_mem_hw');
  await check('get_net_detail'); await check('get_gpu_detail'); await check('list_services'); await check('list_processes');
  await check('list_startup'); await check('get_activity', { limit: 50 }); await check('get_settings'); await check('get_about');
  const cats = await check('get_junk_categories');
  const first = cats && (cats.categories || cats)[0];
  if (first) await check('scan_junk', { id: first.id });
  const apps = await check('list_apps');
  const list = apps && (apps.apps || apps);
  const app = list && list.find((a) => !a.is_edge && !a.uninstaller_missing);
  if (app) { await check('get_related', { app_id: app.id }); await check('find_leftovers', { app_id: app.id }); }
  await check('other_browsers'); await check('check_update');
  await check('get_specs_full');
  return JSON.stringify(out);
})()`;

// --- shape comparison ---
function diff(real, mock, path = '$') {
  const bad = [];
  if (real === 'null' || mock === 'null' || real === 'empty' || mock === 'empty') return bad;   // optional / empty list
  if (typeof real !== typeof mock || Array.isArray(real) !== Array.isArray(mock)) return [`${path}: real is ${JSON.stringify(real)} but the UI expects ${JSON.stringify(mock)}`];
  if (Array.isArray(real)) return diff(real[0], mock[0], path + '[]');
  if (typeof real === 'object') {
    for (const k of Object.keys(mock)) if (!(k in real)) bad.push(`${path}.${k}: MISSING in real response (UI expects ${JSON.stringify(mock[k])})`);
    for (const k of Object.keys(real)) if (k in mock) bad.push(...diff(real[k], mock[k], `${path}.${k}`));
    return bad;
  }
  return real === mock ? bad : [`${path}: real ${real} vs UI ${mock}`];
}

let failures = 0;
try {
  const t = await target();
  await sleep(1500);
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = () => rej(new Error('websocket failed')); });
  const result = await new Promise((resolve, reject) => {
    ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id === 1) d.result?.exceptionDetails ? reject(new Error(JSON.stringify(d.result.exceptionDetails).slice(0, 400))) : resolve(d.result.result.value); };
    ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression: PAGE_SCRIPT, awaitPromise: true, returnByValue: true, timeout: 120000 } }));
  });
  const rows = JSON.parse(result);
  console.log('command'.padEnd(22), 'ms'.padStart(6), ' result');
  for (const r of rows) {
    const problems = r.err ? [`ERROR: ${r.err}`] : diff(r.real, r.mock);
    if (problems.length) failures++;
    console.log(r.cmd.padEnd(22), String(r.ms).padStart(6), ' ' + (problems.length ? 'FAIL' : 'ok  '), problems.length ? '' : (r.sample || '').slice(0, 90));
    problems.slice(0, 6).forEach((p) => console.log('      -', p));
  }
  console.log(`\n${rows.length - failures}/${rows.length} commands match the UI contract`);
  ws.close();
} catch (e) {
  failures++;
  console.error('E2E FAILED:', e.message);
} finally {
  if (!keep) kill();
}
process.exit(failures ? 1 : 0);
