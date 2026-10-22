# Optimizer API

Uses SHARED `get_live` (1 s poll, `mem.percent/used/available/total`) and `get_overview().is_admin`.
Sizes in bytes. All commands throw `Error(message)` on failure.

## `boost_ram()` -> object   (admin: optional; deeper with admin)
Trims working sets of all non-protected processes; with admin also purges the standby list. Runs ~1-3 s.
```json
{ "freed": 641728512, "trimmed": 41, "purged": false, "percent": 54.2 }
```
`percent` = RAM in use after the boost. UI shows "Run as Administrator for a deeper boost" when `!purged && !is_admin`.

## `get_junk_categories()` -> object
```json
{ "categories": [ { "id": "user_temp", "title": "User temp files", "desc": "Leftovers from apps and installers", "default_on": true, "warn": false } ] }
```
Ids/order: `user_temp`, `win_temp` (desc "System temp folder (needs Administrator)"), `crash` (dumps + WER),
`recycle` (`warn: true`, "Permanently empties the bin"), `browser` (`default_on: false`; Chrome/Edge/Brave/Firefox caches only, never cookies/logins).

## `scan_junk({ id })` -> `{ "bytes": 883367936, "files": 4120 }`
Read-only size scan of one category. Called sequentially per category. Unknown id -> error.

## `clean_start({ ids: ["user_temp", "recycle"] })` -> `{ "job": "c1" }`
Permanently deletes the contents of the listed categories (the UI confirms first; the backend must re-validate ids and only touch the
known folders). Files in use are skipped, not an error. `win_temp` without admin: skip and report `failed`.
No cancel command.

Events:
- `clean:progress` `{ job, id, done_bytes, total_bytes }` (<= ~10/s per category)
- `clean:done` `{ job, results: [{ id, freed, failed }], total_freed }`  (`failed` = number of items skipped)
