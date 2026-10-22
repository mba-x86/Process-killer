# review agent - change log and requests

## REQUESTS to the orchestrator (the published files do NOT build in the shared crate until the first is applied)
1. `src/lib.rs`: add `mod recycle;` (new file `src/recycle.rs`; `apps` already calls `crate::recycle::recycle`).
2. cleanup / storage agents: replace their own `SHFileOperationW(FO_DELETE, FOF_ALLOWUNDO)` with `crate::recycle::recycle(path) -> Result<(), String>`
   (SHFileOperationW silently PERMANENTLY deletes items too large for the bin / on drives without one). `storage.rs:400-425` is affected today.
3. `src/protect.rs` (orchestrator-owned): `safe_location()` has the weaknesses fixed in `apps/parse.rs::safe_location` (trailing dots/spaces, 8.3 names,
   no profile/known-folder rules); `is_protected_file()` compares raw lower-case text (no trailing-dot / 8.3 / junction handling). Prefer `apps::folder_allowed`
   style checks (normalise + GetLongPathName + canonicalize) for any delete. Consider adding to PROTECTED_PROCS: audiodg.exe, lsaiso.exe, sihost.exe,
   taskhostw.exe, msmpeng.exe, nissrv.exe, securityhealthservice.exe, spoolsv.exe (protects when the exe path cannot be read).
4. `src/jobs.rs` / `src/activity.rs`: replace `.lock().unwrap()` with `.lock().unwrap_or_else(|e| e.into_inner())` (poisoning cannot occur with panic=abort, but tests/dev builds can).
5. Release: set `UPDATE_PUBLIC_KEY` in `updates.rs` and sign installers (see RELEASING.md). Until then a compromised release/manifest = code execution as admin.
6. Installer is `PrivilegesRequired=lowest` (+ dialog) while the exe manifest is `requireAdministrator`: a per-user install lives in a user-writable folder, so any
   same-user process can replace the exe and inherit the UAC approval. Prefer `PrivilegesRequired=admin` (Program Files) or drop the manifest and elevate on demand.

## CHANGES (file | issue | fix)
- src/recycle.rs (new) | SHFileOperationW+FOF_ALLOWUNDO permanently deletes too-large items silently | IFileOperation + FOFX_RECYCLEONDELETE, pre-flight (fixed drive, bin enabled, size <= bin MaxCapacity), post-check of bin item count, COM init balanced; item that cannot be recycled FAILS "too large for the Recycle Bin ... not deleted".
- src/apps/system.rs | `recycle()` reported "Moved to the Recycle Bin" whenever the path had vanished (also after a permanent delete) | now uses crate::recycle, honest result
- src/apps/system.rs | `descendants` had no start-time guard: stale/recycled ppid could end unrelated processes | children must start after the parent; added `Proc.start`
- src/apps.rs `folder_allowed` | bypass via trailing dots/spaces, 8.3 names, junction/symlink (also through a parent link), `C:\Users\<any>`, Public/Default, Documents/Desktop/Downloads..., Common Files, WindowsApps ancestors, `SystemRoot` itself | normalised multi-spelling check + reparse-point refusal + canonical-path comparison
- src/apps/parse.rs `safe_location`/`normalise_path` | same trailing-dot / 8.3 bypass (broad-folder comparison was textual) | strip trailing dots/spaces, compare GetLongPathName spelling
- src/apps.rs `run_uninstall` | uninstall string run through `cmd /S /C "<registry text>"` (`&`, `|`, `%VAR%` injection; HKCU is user-writable); 1 h timeout did not end the child | new src/apps/launch.rs: program split from args, started directly (absolute path or msiexec/rundll32 in System32), args passed verbatim, 740 -> ShellExecuteEx, timeout ends the process tree
- src/apps.rs `uninstall_start` | "Microsoft Edge Update" (shared with WebView2) could be uninstalled | refused
- src/apps.rs / apps/*.rs | `.lock().unwrap()`, `unwrap()` on `as_object_mut`, `chars().next()` | non-panicking; model.rs `size_kb * 1024` saturating
- src/startup.rs `tag_span` | `to_lowercase()` changes byte length (U+0130, U+212A) so offsets sliced the original at wrong places: a task XML with such a character PANICS (=abort) the Startup page | ASCII-only lowercase + regression test
- src/updates.rs | signature silently skipped when the manifest had none (even with a key configured) | missing signature is a hard failure once a key is built in
- src/updates.rs | `update_source` in user-writable settings.json let any same-user process point the elevated updater at its own manifest+installer (hash is in the same manifest); http://127.0.0.1 allowed in release | custom source honoured only when UPDATE_PUBLIC_KEY is set; loopback http only under cfg(test)
- src/updates.rs | no size limit on the installer; URL-derived file name (CON.exe, odd chars); predictable temp dir; hash-then-launch TOCTOU; double start launched two installers | cap = manifest size (<=512 MB) incl. Content-Length, fixed file name, create_dir + create_new, file re-hashed and locked (no writers/deleters) until the installer is spawned, single-flight guard, stale pk_update_* cleanup
- src/updates.rs settings | truncated/non-object settings.json was overwritten (all 1.x keys lost); non-atomic write | damaged file kept as settings.json.bak, write via tmp + rename; BOM tolerated
- src/updates.rs `is_repo_url` | control chars/whitespace accepted | rejected
- src/services.rs | critical-service blocklist too small | + BrokerInfrastructure, SystemEventsBroker, CoreMessagingRegistrar, ProfSvc, gpsvc
- src/processes.rs | "End process" could end Process Killer's own WebView2 helpers (blank window); open_location probed UNC paths (NTLM leak) | own process tree refused; UNC refused
- src/system.rs | dxdiag wrote an elevated file to a predictable %TEMP% name (link redirection); topology buffer was Vec<u8> cast to an aligned struct (UB / debug panic), no bounds check on record Size | fresh private folder via create_dir; u64 buffer + bounds check
- src/perf.rs | wlanapi.dll loaded with default search order; SMBIOS type-16 sum could overflow | LOAD_LIBRARY_SEARCH_SYSTEM32; saturating_add

## Not changed (see final report): get_live is a sync command (main thread, <5 ms), PID-reuse window between snapshot and OpenProcess in processes.rs,
## dxdiag/%TEMP% residual race (same-user attacker watching the temp dir), uninstall job has no cancel.
