# system.rs - handlers to add to `generate_handler![...]`
```
system::get_live,
system::get_drives,
system::get_top_processes,
system::get_top_apps,
system::copy_text,
system::get_specs_fast,
system::get_specs_full,
system::get_directx,
system::get_cpu_detail,
```
No `app.manage` needed (uses existing AppState). No Cargo changes. Caches: `%APPDATA%\ProcessKiller\specs.json` (1.x format), `directx.txt` (refreshed after 7 days).
Notes: get_live/get_specs_fast are sync (cheap); the rest are async. TPM needs admin (else "Unavailable (run as Administrator)").
