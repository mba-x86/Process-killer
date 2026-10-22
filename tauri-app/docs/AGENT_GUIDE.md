# Process Killer 2.0 (Tauri) - guide for UI page agents

Goal: rebuild the Windows app **Process Killer** with Tauri (Rust backend + HTML/CSS/JS frontend) using the new design.
The old app is Python/Tkinter in the repo root (`app.py`, `core.py`, `optimizer.py`, `sysinfo.py`, `pages_extra.py`, `dashboard.py`,
`about.py`, `updater.py`) - **read it for behaviour, texts and safety rules; do not modify it.**

Hard priorities: **small, instant-launching, light on CPU/RAM**. Vanilla ES modules, no npm packages, no frameworks, no bundler.

## Layout (`tauri-app/`)
```
ui/index.html            entry (do not edit)
ui/css/app.css           design tokens + shell + shared components (SHARED - do not edit)
ui/js/*.js               shared core (SHARED - do not edit): dom, icons, fmt, charts, ui, api, mock, routes, main
ui/pages/<id>.js         ONE FILE PER PAGE  <- you own these
ui/css/pages/<id>.css    optional page-only styles (load with ctx.css('css/pages/<id>.css'))
ui/mock/<id>.js          mock backend for your page  <- you own these
docs/api/<id>.md         the Rust API your page needs (commands + JSON shapes)  <- you own these
design-ref/<Page>.html   the design markup (inline styles = exact sizes/colours)      READ ONLY
design-ref/png/<Page>.png  rendered design 1440x1500                                  READ ONLY (use the Read tool to look at it)
tools/shot.py            render a route -> PNG + JS error report
```
Never edit another agent's files. If you need a change in a SHARED file, write it to `docs/requests/<your-agent>.md` and work around it locally.

## Page contract
```js
export default async function (ctx) {          // called each time the route opens
  const live = ...build DOM with ctx.h / ctx.ui / ctx.charts...
  ctx.every(1000, async () => { const d = await ctx.api.invoke('get_live'); /* update IN PLACE */ });  // auto-stopped on navigation
  return [ctx.ui.pageHead({ title, sub }), card1, card2];   // Node | Node[] | { el, destroy() }
}
```
`ctx` = `{ api:{invoke,listen,inTauri}, ui, fmt, charts, h, icon, setChildren, toast, navigate(id), every(ms,fn), onCleanup(fn), css(href), overview(), banners:{show,hide} }`.
Helpers: `ui.button/card/pageHead/searchBox/select/checkbox/segmented/dot/pill/badge/spinner/specList/dataTable/toast/modal/confirmDialog`,
`charts.ring/sparkline/history/lineChart/arcGauge/bar`, `fmt.bytes/rate/pct/ghz/num/uptime/date/usageColor`, `icon(name)` (see `js/icons.js`).
Read the shared files once - they are short.

## Performance rules (these matter more than pixel perfection)
1. **Update in place.** Build the DOM once; on each tick change text/attributes of existing nodes (`setText`, `ring.set`, `sparkline.update`). Never re-render a whole page every second.
2. Use `ctx.every` (paused while the window is hidden, stopped on navigation). No raw `setInterval`, no `requestAnimationFrame` loops while idle.
3. Tables: render at most ~300 rows at once (filter/limit before rendering); keep row DOM simple.
4. No images/fonts/libraries beyond what is in `ui/`. Prefer CSS/SVG. No `innerHTML` with user/system data (XSS) - use `h()`; static icon markup only.
5. Everything the backend returns is **untrusted text** (app names, paths): never put it in HTML strings.
6. Keep each page file small and readable (< ~500 lines).

## Backend contract (what you must document)
- Command names are `snake_case`; reads start with `get_`/`list_`; actions are verbs (`kill_process`, `set_service_state`, `uninstall_app`...).
- All commands return JSON (`camelCase`-free: use **snake_case keys**); errors are thrown as `Error(message)` with a human-readable message.
- Sizes in **bytes**, rates in **bytes/second**, percentages 0-100 (numbers), timestamps in **unix seconds**.
- Long jobs (scan, clean, uninstall, download): `invoke('<x>_start', args)` returns `{ job: "<id>" }`; the backend emits events
  `<domain>:progress` (payload `{ job, ... }`) and `<domain>:done` (payload `{ job, ok, message, ... }`); `invoke('<x>_cancel', { job })` stops it.
  Use `ctx.api.listen(event, cb)` and un-listen in `ctx.onCleanup`.
- Anything destructive is confirmed in the UI first (`ui.confirmDialog`) and the backend must re-validate (protected processes/folders, admin).
- Write `docs/api/<id>.md`: for each command: name, args, returns (JSON example), errors, polling interval, admin needed?.
- Your mock (`ui/mock/<id>.js`) implements the same commands with realistic data (use the real values from the design PNGs: DESKTOP-JOGUA40, i5-14400F, RTX 5060, etc.) and fills the contract. Use `mock(cmd, fn)`, `emit(event, payload)`, `walk()`.

## Testing your page
```
py tauri-app/tools/shot.py <route>                       # route ids: dashboard processor memory network graphics applications services processes startup optimizer storage activity about
py tauri-app/tools/shot.py applications --wait 6000 --query "admin=0"
```
It prints JS errors (must be 0) and the PNG path; then **Read the PNG and compare it with `design-ref/png/<Page>.png`**. Iterate until layout, spacing, type sizes and colours match.
Check interactions by temporarily scripting them in your mock/page and taking screenshots (e.g. open the wizard modal, select a row).
Also look at 1100x700 (`--size 1100x700`): nothing may overflow horizontally.

## Behaviour parity & safety (from the old app) - do not lose these
- Protected processes (`system, smss, csrss, wininit, winlogon, services, lsass, svchost, dwm, explorer, fontdrvhost, ...`, anything under `C:\Windows`) can't be ended.
- Folders named `Microsoft, Windows, WindowsApps, Common Files, ...` are never offered for deletion; deletes go to the Recycle Bin.
- Uninstall wizard: review related processes/services/startup/tasks (path matches ticked, name-only unticked) -> stop -> uninstall -> leftovers -> done.
  Broken entries (uninstaller missing) -> remove the entry with a .reg backup. Microsoft Edge -> guarded flow (risk banner, another browser required, "I understand" tick, admin, WebView2 untouched, optional reinstall block).
- Junk cleaner categories: user temp, Windows temp (admin), crash dumps/WER, Recycle Bin (permanent), browser caches (unticked by default).
- Admin banner + "Restart as Administrator" is handled by the shell. Update banner/flow is owned by the `about` page agent (it must also work from any page via `ctx.banners`).

## Deliverable & report
Files: `ui/pages/<id>.js` (+ optional `ui/css/pages/<id>.css`), `ui/mock/<id>.js`, `docs/api/<id>.md`. Finish with a <=150-word report: pages done, commands list, anything that deviates from the design and why, open questions.
