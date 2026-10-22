# cleanup / storage - handler lines for generate_handler![...]
```
cleanup::boost_ram,
cleanup::get_junk_categories,
cleanup::scan_junk,
cleanup::clean_start,
storage::storage_scan_start,
storage::storage_scan_cancel,
storage::choose_folder,
storage::recycle_files,
```
No extra managed state. `open_location` is NOT defined here (manage agent). Uses `AppState.jobs`; commands take `State<AppState>`.
`choose_folder` takes the `WebviewWindow` (injected automatically) as dialog owner. `ntdll.lib` is linked for NtSetSystemInformation. No Cargo changes.
