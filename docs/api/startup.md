# startup page API
## `list_startup()` -> `{items:[...]}`  (on open + Refresh; tasks may take a moment, run off the UI thread; Microsoft built-in tasks already filtered out)
```json
{ "items": [
  { "id": "s3", "kind": "startup", "name": "Discord", "command": "\"C:\...\Update.exe\" --processStart Discord.exe", "location": "HKCU Run", "status": "" },
  { "id": "t7", "kind": "task", "name": "\GCC", "command": "\"C:\...\GCC.exe\" -b", "location": "", "status": "Ready" } ] }
```
`id` is opaque and stable only until the next list (Rust maps it back to hive/path/name or task name). Column "Location" shows `location || status`.
## `remove_startup({ids:[..]})` -> `{results:[{id, ok, message}]}`
Startup entry -> delete the registry value; task -> `schtasks /delete /tn <name> /f`. "already gone" counts as ok. UI confirms first.
