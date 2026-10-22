# Process Killer 2.0 - guide for Rust backend agents

The UI (vanilla JS in `ui/`) is finished against a **mock backend**. Your job: implement the real Rust commands so the
UI works unchanged. **The contract is `docs/api/*.md` (+ `docs/api/SHARED.md`) and the mock handlers in `ui/mock/*.js`** -
JSON keys, units and event payloads must match EXACTLY (snake_case keys, bytes, bytes/s, percent 0-100, unix seconds).
Behavioural source of truth for every feature = the 1.x Python code in the repo root (`core.py`, `optimizer.py`,
`sysinfo.py`, `securityops.py`, `pages_extra.py`, `app.py`, `updater.py`): port its logic AND its safety rules.

## Project (`tauri-app/src-tauri`, Tauri v2, Rust 2021, Windows only)
```
src/lib.rs        module list + command registration            ORCHESTRATOR-OWNED (do not edit)
src/main.rs       entry                                          do not edit
src/util.rs       CmdError/CmdResult, run_hidden, powershell, is_admin, wide, data_dir, reveal_in_explorer, win_dir
src/protect.rs    protected processes/dirs/files, safe_location  (call these in EVERY destructive command)
src/jobs.rs       Jobs::start() -> (id, cancel flag); cancel(); finish()
src/state.rs      AppState { jobs, sys: Mutex<sysinfo::System>, nets }   (tauri::State<AppState>)
src/shell.rs      app_ready, get_overview, relaunch_as_admin     orchestrator-owned
src/<domain>.rs   YOUR module(s): system, perf, apps, services, processes, startup, cleanup, storage, updates
```
Add private helper modules in your own files/sub-folders only (`src/apps/mod.rs` is fine: replace `apps.rs`). If you need a new
dependency, a `windows` feature, or a change to a shared file, write it in `docs/requests/<agent>.md` and use `cargo add`-free workarounds
until the orchestrator applies it (Cargo.toml is shared; the dependency set already includes: tauri, serde, serde_json, winreg, sysinfo,
sha2, ureq, minisign-verify, walkdir, windows with a broad Win32 feature list).

## Command rules
1. `#[tauri::command] pub fn name(args...) -> CmdResult<Json-serialisable>`; arg names = JSON arg names in the docs (Tauri maps `camelCase`
   JS keys to snake_case params: the UI sends snake_case keys, so declare params with `#[tauri::command(rename_all = "snake_case")]`).
2. **Never block the UI thread:** anything > ~20 ms (process/service enumeration, registry sweeps, WMI, disk walks, subprocesses) must be
   `async` (`#[tauri::command] pub async fn` + `tauri::async_runtime::spawn_blocking`) or run on a worker thread with events (jobs).
3. Long jobs: return `{"job": id}` at once; emit `<domain>:progress` / `<domain>:done` via `app.emit(...)` (`use tauri::Emitter`); honour the cancel flag.
4. Errors: `CmdResult` (message is shown to the user: plain English, no Rust debug text). Partial failures inside a batch go in the result rows (`ok:false,message`).
5. **Safety is enforced here, not only in the UI:** `protect::is_protected_process / is_protected_dir_name / is_protected_file / safe_location`;
   deletes go to the Recycle Bin (`SHFileOperationW` with `FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT`, or the 1.x PowerShell method as fallback);
   admin-only actions must return a clear error when `!util::is_admin()`; Edge removal rules (other browser required, WebView2 never touched).
6. Never build command lines by string interpolation of untrusted text; pass args as separate arguments. Validate ids/paths received from the UI.
7. Use direct Win32/registry APIs instead of PowerShell wherever one exists (faster, no console flash). PowerShell/WMI only for what has no simple API
   (TPM, DirectX via dxdiag, task XML via file reads is fine). Always `CREATE_NO_WINDOW` (`util::run_hidden`).
8. Keep it lean: no new heavy crates; release profile is size-optimised; avoid regex/serde features you don't need.
9. Data shared with 1.x: `util::data_dir()` = `%APPDATA%\ProcessKiller` (`settings.json` {auto_check, update_source}, `activity.log`, `backups\*.reg`, `specs.json`, `directx.txt`).
   Keep file formats compatible so users keep their settings after the upgrade.

## Private workspace (REQUIRED)
Agents run in parallel and share one crate, so NEVER compile inside tauri-app/src-tauri directly (another agent's unfinished code would
break your build). Work in your own copy:
```
py tauri-app/tools/rust_workspace.py <agent>        # creates tauri-app/.cache/rust-work/<agent>/src-tauri (own target dir, own Cargo.toml)
cd tauri-app/.cache/rust-work/<agent>/src-tauri
cargo check                                          # first build ~2 min, then seconds
cargo test
```
Edit only YOUR module files inside that copy. You may edit the copy's Cargo.toml to add a dependency/feature - report every change in
docs/requests/<agent>.md. When done (and green), publish just your files back:
`py tauri-app/tools/rust_workspace.py <agent> --publish src/apps.rs src/apps/` (it refuses orchestrator-owned files).
In your copy the commands are not registered (lib.rs is the skeleton); `cargo check` still type-checks them. Add `#![allow(dead_code)]`-style
attributes in YOUR module if the "unused" warnings get in the way.
Shared helpers: `crate::util` (CmdResult, run_hidden, powershell, is_admin, data_dir...), `crate::protect`, `crate::jobs`, `crate::state::AppState`,
`crate::activity::log(&app, "ok"|"fail"|"info", "text")` - call it for every notable user-visible action (ended a process, uninstalled an app, cleaned N MB...).

## Verify
`cargo check` must be warning-clean for your files; `cargo test` must pass. Add `#[cfg(test)]` unit tests for parsing + SAFETY rules (protected paths,
uninstall-string parsing, version compare, junk path expansion...). End by writing `docs/api/<domain>.rust.md`: every command as the exact line to add
to `generate_handler![...]` (e.g. `apps::list_apps,`) plus any `app.manage(...)` state you need.
Read-only/safe checks against the real system are encouraged (write a `#[test] #[ignore]` that prints real output). **Never run destructive actions
(uninstall, delete, kill, stop service, registry delete) against the real machine in tests** - test them on throw-away temp folders / HKCU test keys only.

## Report
Finish with <=150 words: commands implemented, deviations from the contract (and why), what you could not verify.
