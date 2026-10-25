# Applications page - Rust API

Sizes bytes, snake_case keys, ids are **opaque strings** chosen by the backend (the UI never parses them). All destructive
operations must be re-validated in Rust (protected processes/folders, admin) - the UI is not trusted. Errors are thrown as `Error(message)`.
Reference behaviour: `core.py` (`list_apps`, `find_related`, `find_leftovers`, `run_uninstall`, `uninstaller_missing`, `is_edge`,
`other_browsers`, `remove_uninstall_entry`, `block_edge_reinstall`, `PRE_ACTIONS`, `CLEAN_ACTIONS`) and `app.py` (`AppsPage`, `Wizard`).

## `list_apps()` -> object   (called on open and on Refresh; may take seconds - run off the UI thread, no polling)
```json
{ "apps": [ { "id": "app17", "name": "Brave", "publisher": "Brave Software Inc", "version": "155.1.97.59", "size": 570425344,
  "date": "2026-10-16", "location": "C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application", "drive": "C:\\",
  "uninstaller_missing": false, "is_edge": false, "can_silent": true } ] }
```
- Missing values are `""` (strings) or `0` (size); the UI shows an em dash. `date` is `YYYY-MM-DD`, `""` if unknown, or the literal `"Unknown"` when the registry holds an invalid date.
- `drive` is `"C:\\"`-style (upper-case letter + `:\`) derived from `location`, `""` if unknown; the UI builds the drive filter from the distinct values.
- `uninstaller_missing` = `core.uninstaller_missing(app)`; `is_edge` = `core.is_edge(app)` (Microsoft Edge only, never WebView2); `can_silent` = a quiet-uninstall string exists.
- The UI renders at most 300 rows after filter+sort, so returning all apps (hundreds) is fine.

## `get_related({ app_id })` -> object   (on every row selection and when the wizard opens; admin: no)
```json
{ "items": [
  { "id": "p:brave.exe", "kind": "process", "name": "brave.exe", "detail": "", "state": "Running", "reason": "path", "protected": false,
    "action_label": "End process", "count": 21, "mem": 3650000000 },
  { "id": "s:BraveElevationService", "kind": "service", "name": "Brave Elevation Service", "detail": "Service \u00b7 Manual", "state": "Stopped",
    "reason": "path", "protected": false, "action_label": "Stop service" },
  { "id": "t:brave-update", "kind": "task", "name": "Brave update tasks", "detail": "2 scheduled tasks", "state": "Ready", "reason": "path", "protected": false, "action_label": "Delete task" } ] }
```
- `kind`: `process | service | startup | task | folder`. `reason`: `path` (found in the install folder - certain, ticked by default) or `name` (guess, unticked).
- Processes are **grouped by executable**: one item with optional `count` (instances) and `mem` (bytes). `detail` is the muted second line (empty for processes).
- `protected: true` rows (system/`C:\Windows` processes etc.) are shown locked in the wizard and must be refused by `uninstall_start` even if their id is sent.
- `action_label` = what the wizard will do (`PRE_LABELS`: End process / Stop service / Remove entry / Delete task).
- Empty `items` -> the panel says nothing runs in the background.

## `other_browsers()` -> `{ "names": ["Brave", "Google Chrome"] }`   (Edge flow only; `core.other_browsers()`)
Empty list blocks the Edge removal (UI refuses to continue).

## `uninstall_start({ app_id, stop_ids, silent, block_edge_reinstall })` -> `{ "job": "job1" }`   (admin: Edge needs it; others as the uninstaller requires)
Runs in a background thread and returns immediately. Steps: stop each id in `stop_ids` (`PRE_ACTIONS[kind]`, skipping unknown/protected ids), then
- `uninstaller_missing` -> back up the key to a `.reg` file and `remove_uninstall_entry` (no uninstaller is run);
- otherwise `run_uninstall(app, silent)`; if it succeeded and the app is Edge and `block_edge_reinstall` -> `block_edge_reinstall()`.
Edge: reject with an Error when not admin or when `other_browsers()` is empty (the UI also checks, but never trust it). WebView2 is never touched.
Events (the UI subscribes *before* invoking and buffers events that arrive before the job id is known):
- `uninstall:progress` `{ "job", "level": "info"|"ok"|"fail", "text" }` - one line of the live log (e.g. `Process \u201cbrave.exe\u201d \u2014 ended 21 process(es)`, `Starting the uninstaller\u2026`).
- `uninstall:done` `{ "job", "ok": true, "message": "...", "needs_restart": false }` - exactly once. `ok:false` shows the failure screen with *Look for leftovers anyway*.

## `find_leftovers({ app_id })` -> `{ "items": [ <item as above> + "default_on": true ] }`   (after a successful uninstall; may take ~1 s)
Same item shape (`kind` normally `folder|startup|task|service|process`, `detail` = path or description, `state` = free text such as `Folder · 412 MB`,
`action_label` from `CLEAN_LABELS`) plus `default_on` (ticked initially: path matches true, name guesses false, Edge profile data (`...\\Edge\\User Data`) **false**).
The id must still resolve after the app has left the registry. Empty list -> the wizard goes straight to Finish ("Nothing was left behind.").

## `clean_leftovers({ app_id, ids })` -> `{ "results": [ { "id", "ok": true, "message": "Moved to the Recycle Bin" } ] }`
`CLEAN_ACTIONS` per id. Folders go to the **Recycle Bin**; never delete `Microsoft, Windows, WindowsApps, Common Files, ...` folders (return `ok:false` with a message instead).
Failures are listed on the Finish screen as `name \u2014 message`.

## `open_folder({ path })` -> null
Opens the folder in Explorer (the "Open install folder" icon button in the detail panel; only shown when `location` is non-empty). Validate that `path` is an existing directory.

## Notes
- After the wizard closes the page calls `list_apps()` again.
- Shared commands used: `get_overview` (`is_admin`), `relaunch_as_admin` (Edge flow offers a restart when not admin).
- Browser-only demo switch (ignored inside Tauri): `?demo=panel-edge|panel-broken|wizard|wizard-edge|wizard-broken|wizard-run` on the page URL.
