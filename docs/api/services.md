# services page API
Admin needed for start/stop/config of most services (Rust must return a readable error otherwise).

## `list_services()` -> `{services:[...]}`  (on open, Refresh, and after every action; no polling)
```json
{ "services": [ { "name": "AppXSvc", "display": "AppX Deployment Service (AppXSVC)", "status": "running",
  "start": "automatic", "path": "C:\WINDOWS\system32\svchost.exe -k wsappx -p", "system": true } ] }
```
`status`: running|stopped|paused|pending. `start`: automatic|manual|disabled|delayed. `system`: true when the binary is under C:\Windows
(same rule as the Python `_is_system`). Use the cheap SCM queries (config + status), not per-service property calls.

## `set_service_state({names:[..], action})` -> `{results:[{name, ok, message}]}`
action: start|stop|restart|enable (automatic)|manual (demand)|disable (stop + disabled). Never throws per item; failures are `ok:false` with the SCM message.
The UI confirms stop/restart/disable for `system:true` services (and multi-disable); Rust must not rely on it for hard-blocked services.
