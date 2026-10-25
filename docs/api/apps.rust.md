# apps - handler registration (Rust)

Add to `generate_handler![...]` in lib.rs:
```
apps::list_apps,
apps::get_related,
apps::other_browsers,
apps::uninstall_start,
apps::find_leftovers,
apps::clean_leftovers,
apps::open_folder,
```
No `app.manage(...)` needed (uses existing AppState.jobs). No Cargo changes.
Events: `uninstall:progress`, `uninstall:done`. Files: src/apps.rs, src/apps/{parse,model,system,tests}.rs
