# Shared commands (owned by the orchestrator; every page may call these, none may redefine them)

All sizes bytes, rates bytes/s, percentages 0-100, times unix seconds, snake_case keys.

## `get_overview()` -> object  (called once at startup)
`{ hostname, os, model, uptime_secs, is_admin, version, copyright }`
## `app_ready()` -> null   (UI calls it after the first paint; Rust then shows the main window - no white flash, instant feel)
## `relaunch_as_admin()` -> null   (UAC prompt; the app then exits)
## `get_live()` -> object  (poll every 1000 ms; must be cheap: < 5 ms, no process enumeration)
```json
{ "ts": 1790000000,
  "cpu":  { "percent": 10.2, "per_core": [5.1, 3.0, ...], "freq_mhz": 2500, "threads": 16, "cores": 10 },
  "mem":  { "total": 16966000000, "used": 10520000000, "available": 6446000000, "percent": 62.0, "committed": 14100000000, "cached": 3900000000 },
  "net":  { "down_bps": 21400, "up_bps": 12000 },
  "disk_sys": { "mount": "C:\\", "total": 511000000000, "used": 389000000000, "free": 122000000000, "percent": 76.0 } }
```
`per_core` has one entry per logical processor, in the order of `get_cpu_detail().cores`.
## `get_drives()` -> array  (poll <= every 10 s; runs off the UI thread in Rust, may be slow)
`[{ "mount": "C:\\", "label": "Local disk", "fstype": "NTFS", "total": n, "used": n, "free": n, "percent": 76.1, "removable": false }]`
## `get_top_processes({ by: "memory"|"cpu", limit: 6 })` -> array of individual processes (poll every 2-5 s)
`[{ "pid": 11952, "name": "brave.exe", "mem": 123456789, "cpu": 0.4 }]`
## `get_top_apps({ by: "memory"|"cpu", limit: 6 })` -> array grouped by executable name
`[{ "name": "brave.exe", "count": 21, "mem": 3650000000, "cpu": 1.2 }]`
## `copy_text({ text })` -> null   (clipboard; the UI may also use navigator.clipboard)

## Events (backend -> UI)
`job:progress` / `job:done` are NOT used; each domain has its own: `storage:progress`, `storage:done`, `clean:progress`, `clean:done`,
`uninstall:progress`, `uninstall:done`, `update:progress`, `update:done`. Payload always includes `{ job }`.
