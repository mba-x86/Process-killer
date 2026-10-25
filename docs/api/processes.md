# processes page API
## `list_processes()` -> `{processes:[...]}`  (UI polls every 3000 ms while "Auto-refresh" is on and the window is visible)
```json
{ "processes": [ { "pid": 11952, "name": "brave.exe", "exe": "C:\Program Files\...\brave.exe", "mem": 438304768, "cpu": 0.4, "protected": false } ] }
```
`mem` = working set bytes, `cpu` = % of total CPU since previous call (0-100), `exe` may be "" (shown as dash), `protected` = name in PROTECTED_PROCS
(system, smss, csrss, wininit, winlogon, services, lsass, svchost, dwm, explorer, fontdrvhost, memory compression, ...) or exe under C:\Windows.
The UI shows at most the top 300 rows after filter/sort.
## `kill_process({pids:[..], tree:bool})` -> `{results:[{pid, ok, message}]}`
Rust MUST re-check `protected` and refuse (`ok:false, message:"protected system process"`). tree=true kills children recursively first.
## `open_location({path})` -> null
Reveal in Explorer (`explorer /select,<path>`). Error if the file does not exist.
