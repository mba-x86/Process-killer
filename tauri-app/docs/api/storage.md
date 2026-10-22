# Storage analyzer API

Uses SHARED `get_drives()` (loaded once on page open). Sizes bytes, times unix seconds. Errors throw `Error(message)`.

## `storage_scan_start({ roots, min_size })` -> `{ "job": "s1" }`
`roots`: array of drive mounts (`"C:\\"`) or one folder path. `min_size`: bytes (UI sends 1048576; the size/type/text filters are applied client-side).
Walk with an explicit stack; skip symlinks/junctions; count unreadable dirs in `errors`. Keep the N (>=1500) largest files with size >= min_size and
per-first-level-folder byte totals (files directly in the root go to key `"<root>  (files in this folder)"`).
Only one scan at a time. No admin required (protected folders are counted in `errors`).

## Event `storage:progress` (every ~1.5 s, never faster)
```json
{ "job": "s1", "files": 31980, "bytes": 220000000000, "errors": 12, "current": "C:\\Windows\\WinSxS",
  "total_bytes": 511000000000,
  "top_files": [ { "path": "C:\\pagefile.sys", "size": 12345678901, "mtime": 1790000000 } ],
  "folders": [ { "path": "C:\\Users", "size": 98000000000 } ] }
```
`top_files` <= 1500 items (largest first, full replacement each time). `folders` <= 500. `total_bytes` = used bytes of the scanned
partitions, or `null` for a custom folder (UI shows an indeterminate bar).

## Event `storage:done`  `{ "job": "s1", "cancelled": false, "errors": 12 }`
Always sent once, also after cancel (a final `storage:progress` with the last data should precede it).

## `storage_scan_cancel({ job })` -> null

## `choose_folder()` -> `{ "path": "D:\\Games" | null }`   (native folder dialog; null if dismissed)

## `recycle_files({ paths })` -> `{ "results": [ { "path": "...", "ok": true, "message": "moved to Recycle Bin" } ] }`
Per path. MUST refuse (ok:false, message "protected system file"): `pagefile.sys`, `hiberfil.sys`, `swapfile.sys` and anything under `%WinDir%\`.
Missing file -> `ok:true, message:"already gone"`. Uses SendToRecycleBin (UI confirmed first).

## `open_location({ path })` -> null
Opens Explorer with the file selected (`explorer /select,"path"`); if `path` is a directory, opens it.

Copy path uses `navigator.clipboard` (falls back to SHARED `copy_text`).
