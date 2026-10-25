# updates (Rust) - handler registration

Add to `generate_handler![...]` in lib.rs:
```
updates::get_activity,
updates::clear_activity,
updates::open_log_file,
updates::get_settings,
updates::set_settings,
updates::get_about,
updates::open_url,
updates::check_update,
updates::start_update,
```
No `app.manage(...)` needed (uses `AppState.jobs`). No Cargo.toml changes.

Notes
- `UPDATE_PUBLIC_KEY` (src/updates.rs) is empty: signature check skipped, SHA-256 still mandatory (a manifest without a valid `sha256` is refused).
- `clear_activity` truncates the log file (per task brief; the api doc says the file stays).
- Log timestamps: local time -> unix via GetLocalTime/GetSystemTime offset (<=1 h off for lines written across a DST change).
- `get_about` omits `release_notes_current` (optional).
