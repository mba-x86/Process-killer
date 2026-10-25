// Interaction smoke-test runner (dev only, see tools/smoke.py). Loads the app in a same-origin iframe, runs scenario
// files from dev/scenarios/<name>.js sequentially and publishes JSON into <pre id="result">.
//
// A scenario file default-exports  [{ name, run: async (T) => {...} }]  - see the helpers in makeT() below.
// All waits are counted in poll iterations of setTimeout(), so they work with Chrome's virtual time.
const q = new URLSearchParams(location.search);
const files = (q.get('run') || '').split(',').filter(Boolean);
const frame = document.getElementById('app');
const out = document.getElementById('result');
const state = { finished: false, fatal: null, files: {} };
const publish = () => { out.textContent = JSON.stringify(state); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const norm = (s) => String(s ?? '').replace(/\s+/g, ' ').trim();
let seq = 0;

function load(url) {
  return new Promise((resolve, reject) => {
    const to = setTimeout(() => reject(new Error(`iframe load timeout: ${url}`)), 30000);
    frame.addEventListener('load', () => { clearTimeout(to); resolve(); }, { once: true });
    frame.src = url;
  });
}

function makeT(rec) {
  const doc = () => frame.contentDocument, win = () => frame.contentWindow;
  const pk = () => win().__pk;
  const visible = (el) => !!el && !el.hidden && !el.closest('[hidden]') && (el.getClientRects().length > 0);

  function drain(prefix = '') {
    let p; try { p = pk(); } catch { return; }
    if (!p) return;
    for (const e of p.errors.splice(0)) rec.failures.push(`${prefix}JS error: ${e}`);
    for (const e of p.resErrors.splice(0)) rec.failures.push(`${prefix}${e}`);
    for (const a of p.alerts.splice(0)) rec.failures.push(`${prefix}alert() was called: ${a}`);
  }

  const T = {
    sleep, norm, drain,
    get doc() { return doc(); }, get win() { return win(); }, get pk() { return pk(); },
    note(msg) { rec.notes.push(String(msg)); },
    /** Records a short text snapshot of the page (debug aid, shown with --verbose). */
    shotNote(label = 'page') {
      const t = norm(doc().getElementById('page')?.innerText || '').slice(0, 300);
      rec.notes.push(`[${label}] ${t}`);
    },

    /* ---- navigation ---- */
    /** Loads the app fresh at #/route. opts: {w,h,override,transform,transformAll}. */
    async open(route, query = '', opts = {}) {
      drain();
      window.__smokePre = { override: opts.override, transform: opts.transform, transformAll: opts.transformAll };
      frame.style.width = `${opts.w || 1440}px`; frame.style.height = `${opts.h || 900}px`;
      await load('about:blank');
      await load(`/index.html?__smoke=1${query ? `&${query}` : ''}&_=${++seq}#/${route}`);
      await T.pageReady(route);
    },
    async pageReady(route) {
      await T.waitFor(() => {
        const page = doc().getElementById('page');
        if (page?.querySelector('.card-pad h2')?.textContent === 'This page failed to load') throw new Error(`page failed to load: ${norm(page.innerText).slice(0, 200)}`);
        return page?.querySelector('h1') && win().__pkReady;
      }, { timeout: 20000, msg: `page ${route || ''} to render` });
    },
    /** Hash navigation inside the running app (no reload). */
    async nav(route, { wait = true } = {}) {
      win().location.hash = `#/${route}`;
      if (wait) {
        await T.waitFor(() => doc().querySelector(`.nav[data-route="${route}"]`)?.classList.contains('on'), { msg: `nav to ${route}` });
        await T.waitFor(() => doc().querySelector('#page h1'), { timeout: 15000, msg: `h1 on ${route}` });
        await sleep(30);
      }
    },
    hash: () => win().location.hash,
    title: () => norm(doc().querySelector('#page h1')?.textContent),

    /* ---- waiting ---- */
    /** waitFor(selector | fn) -> truthy value. fn may throw to abort early. */
    async waitFor(cond, { timeout = 8000, step = 25, msg } = {}) {
      const test = typeof cond === 'string' ? () => doc().querySelector(cond) : cond;
      const n = Math.ceil(timeout / step);
      for (let i = 0; i <= n; i++) {
        const v = await test();
        if (v) return v;
        await sleep(step);
      }
      throw new Error(`timeout (${timeout} ms) waiting for ${msg || (typeof cond === 'string' ? cond : cond.toString().replace(/\s+/g, ' ').slice(0, 120))}`);
    },
    /** Wait until the text of `sel` (or fn()) matches string/RegExp. */
    async waitText(sel, re, opts = {}) {
      const rx = re instanceof RegExp ? re : new RegExp(String(re).replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
      return T.waitFor(() => { const t = typeof sel === 'function' ? sel() : T.text(sel); return rx.test(t) ? t : false; },
        { msg: `text of ${typeof sel === 'string' ? sel : 'fn'} to match ${rx}`, ...opts });
    },
    async waitToast(re, opts = {}) {
      const rx = re instanceof RegExp ? re : new RegExp(String(re).replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
      return T.waitFor(() => T.$$('.toast').find((t) => rx.test(t.textContent)), { msg: `toast ${rx}`, timeout: 6000, ...opts });
    },
    toasts: () => T.$$('.toast').map((t) => norm(t.textContent)),
    async waitGone(sel, opts = {}) { return T.waitFor(() => !doc().querySelector(sel), { msg: `${sel} to disappear`, ...opts }); },

    /* ---- queries ---- */
    $: (sel, root) => (root || doc()).querySelector(sel),
    $$: (sel, root) => [...(root || doc()).querySelectorAll(sel)],
    count: (sel, root) => (root || doc()).querySelectorAll(sel).length,
    text(x, root) { const el = typeof x === 'string' ? (root || doc()).querySelector(x) : x; return norm(el?.textContent); },
    visible,
    /** Last open modal element or null. */
    modal: () => [...doc().querySelectorAll('.overlay .modal')].pop() || null,
    modalText: () => norm(T.modal()?.textContent),
    /** Row (tr) whose text contains `text`. */
    row: (text, root) => [...(root || doc()).querySelectorAll('tbody tr')].find((tr) => norm(tr.textContent).includes(text)) || null,
    /** Find a clickable control by its text: exact match first, then substring. Search the topmost modal first. */
    byText(text, { root, tags = 'button, a, [role=button], label, summary, .btn, th, tr, td, li' } = {}) {
      const rx = text instanceof RegExp ? text : null;
      const ok = (el, exact) => { const t = norm(el.textContent); return rx ? rx.test(t) : exact ? t === text : t.includes(text); };
      const scopes = root ? [root] : [T.modal(), doc()].filter(Boolean);
      for (const scope of scopes) {
        const all = [...scope.querySelectorAll(tags)].filter(visible);
        const part = all.filter((el) => ok(el, false)).sort((a, b) => norm(a.textContent).length - norm(b.textContent).length);
        const hit = all.find((el) => ok(el, true)) || part[0];   // exact text first, else the tightest element containing it
        if (hit) return hit;
      }
      return null;
    },

    /* ---- actions ---- */
    /** click(Element | cssSelector | {text}) - waits for the target, refuses disabled controls (dead-button detector). */
    async click(x, opts = {}) {
      const find = () => ((x && x.nodeType === 1) ? x : typeof x === 'string' ? (opts.root || doc()).querySelector(x) : T.byText(x.text, opts));
      const el = await T.waitFor(() => { const e = find(); return e && (opts.hidden || visible(e)) ? e : false; }, { timeout: opts.timeout || 5000, msg: `clickable ${x?.text ? `"${x.text}"` : typeof x === 'string' ? x : 'element'}` });
      if (el.disabled && !opts.allowDisabled) throw new Error(`cannot click disabled control ${x?.text ? `"${x.text}"` : norm(el.textContent) || el.outerHTML.slice(0, 80)}`);
      if (opts.ctrl || opts.shift) {
        el.dispatchEvent(new (win().MouseEvent)('click', { bubbles: true, cancelable: true, ctrlKey: !!opts.ctrl, shiftKey: !!opts.shift, view: win() }));
      } else if (opts.dbl) {
        el.dispatchEvent(new (win().MouseEvent)('click', { bubbles: true, cancelable: true, view: win() }));
        el.dispatchEvent(new (win().MouseEvent)('dblclick', { bubbles: true, cancelable: true, view: win() }));
      } else el.click();
      await sleep(opts.settle ?? 30);
      return el;
    },
    clickText: (text, opts) => T.click({ text }, opts),
    async type(x, value) {
      const el = typeof x === 'string' ? await T.waitFor(x) : x;
      el.focus(); el.value = value;
      el.dispatchEvent(new (win().Event)('input', { bubbles: true }));
      el.dispatchEvent(new (win().Event)('change', { bubbles: true }));
      await sleep(30);
    },
    async select(x, value) {
      const el = typeof x === 'string' ? await T.waitFor(x) : x;
      const has = [...el.options].some((o) => o.value === value);
      if (!has) throw new Error(`<select> has no option "${value}" (has: ${[...el.options].map((o) => o.value).join(', ')})`);
      el.value = value;
      el.dispatchEvent(new (win().Event)('change', { bubbles: true }));
      await sleep(30);
    },
    async key(k, target) {
      (target || doc()).dispatchEvent(new (win().KeyboardEvent)('keydown', { key: k, bubbles: true, cancelable: true }));
      await sleep(30);
    },

    /* ---- backend instrumentation ---- */
    mark: () => pk().log.length,
    /** Calls of `cmd` since mark (or in total). */
    calls: (cmd, since = 0) => pk().log.slice(since).filter((c) => c.cmd === cmd),
    emit: (event, payload) => pk().emit(event, payload),
    listeners: (event) => pk().lis[event] || 0,
    intervals: () => pk().intervals.size,

    /* ---- assertions ---- */
    assert(cond, msg) { if (!cond) throw new Error(msg || 'assertion failed'); },
    eq(a, b, msg) { if (a !== b) throw new Error(`${msg || 'not equal'}: expected ${JSON.stringify(b)}, got ${JSON.stringify(a)}`); },
    has(hay, needle, msg) {
      const ok = needle instanceof RegExp ? needle.test(hay) : String(hay).includes(needle);
      if (!ok) throw new Error(`${msg || 'text mismatch'}: expected to contain ${needle}, got "${String(hay).slice(0, 200)}"`);
    },
    lacks(hay, needle, msg) {
      const bad = needle instanceof RegExp ? needle.test(hay) : String(hay).includes(needle);
      if (bad) throw new Error(`${msg || 'unexpected text'}: must not contain ${needle}, got "${String(hay).slice(0, 200)}"`);
    },
    /** Run a named sub-check; a failure is recorded and the scenario continues. */
    async step(name, fn) {
      try { await fn(); } catch (e) { rec.failures.push(`[${name}] ${e.message}`); }
      drain(`[${name}] `);
    },
  };
  return T;
}

async function main() {
  publish();
  for (const file of files) {
    const fr = (state.files[file] = { scenarios: [], error: null });
    let list;
    try { list = (await import(`./scenarios/${file}.js`)).default; }
    catch (e) { fr.error = `cannot load scenarios/${file}.js: ${e.message}`; publish(); continue; }
    for (const sc of list) {
      const rec = { name: `${file}: ${sc.name}`, ok: true, ms: 0, failures: [], notes: [] };
      const t0 = performance.now();
      const T = makeT(rec);
      try {
        await Promise.race([sc.run(T), sleep(sc.timeout || 150000).then(() => { throw new Error('scenario timed out'); })]);
      } catch (e) { rec.failures.push(`${e.message}`); }
      T.drain();
      rec.ok = rec.failures.length === 0;
      rec.ms = Math.round(performance.now() - t0);
      fr.scenarios.push(rec);
      publish();
    }
  }
  await load('about:blank').catch(() => {});   // stops every app timer so the remaining virtual-time budget elapses instantly
  state.finished = true;
  publish();
}
main().catch((e) => { state.fatal = String(e && e.stack || e); state.finished = true; publish(); });
