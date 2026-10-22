# About & update API
Poll: none (event driven). Admin: no (the installer may elevate itself).
- `get_about()` -> `{ version, copyright:"(c) 2026 mba-x86", license:"MIT License", repo_url, release_notes_current? }`. Notes text: one item per line, optional prefix `New:` / `Improved:` / `Fixed:` (rendered as tags); a leading `- ` is ignored.
- `check_update()` -> `{ available, current, version, notes, size, published? }` (size bytes, published unix secs). Throws Error("Could not reach the update server ...") when offline.
- `start_update()` -> `{ job }`. Downloads the installer, verifies it, launches it and exits the app when `update:done` ok is emitted.
- Events: `update:progress { job, done, total }` (bytes; total 0 if unknown), `update:done { job, ok, message }` (ok=true -> UI shows "Installing... the app will restart"; ok=false -> message shown as "Update failed: <message>").
- `get_settings()` -> `{ auto_check: bool }`; `set_settings({ auto_check })` -> null (persisted).
- `open_url({ url })` -> null; backend only opens the project repo URL (reject others).
UI: js/updates.js is the single update state machine + banner (`initUpdates(ctx)` 4 s after launch, `checkNow({manual})`, `startUpdate()`, `subscribe`); the About page renders the same state.
Mock queries: `?update=1` (available 2.0.1), `?update=offline` (error), `?update=go` (auto-clicks Update now for screenshots).
