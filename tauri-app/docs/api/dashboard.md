# Dashboard API

Uses shared: `get_overview` (once), `get_live` (1000 ms), `get_drives` (10 s), `get_top_apps {by:"memory",limit:6}` (5 s), `copy_text`.
Uses `boost_ram()` -> `{freed, trimmed, purged, percent}` (owned by the optimizer page; freed in bytes). Errors are shown as a toast.

## `get_specs_fast()` -> `{ sections }`
Instant (< 20 ms), registry/env only, no WMI/PowerShell. Called on every page open and Refresh.
```json
{ "sections": [ { "title": "Device", "rows": [["Device name","DESKTOP-JOGUA40"],["Manufacturer","Gigabyte Technology Co., Ltd."]] },
                { "title": "Security & firmware", "rows": [["Secure Boot","Enabled","ok"],["TPM","Version 2.0 · enabled","ok"]] } ] }
```
Row = `[label, value, tone?]`, tone `"ok" | "warn" | "bad"` (green/amber/red dot + coloured text). Value is a string (never null; use "-" or omit the row).
Section titles used by the UI: Device, Operating system, Security & firmware, Processor, Memory (fast set). The title "Graphics" is special: the UI appends a `DirectX` row to it.

## `get_specs_full()` -> `{ sections }`
Same shape, complete (Device, Operating system, Security & firmware, Processor, Memory, Graphics, Motherboard & BIOS, Storage devices, Network, Battery (only if present)).
Slow (~7 s, WMI). Run off the UI thread. The UI replaces the whole content when it arrives and keeps the scroll position. Admin not needed.

## `get_directx()` -> `{ text }`
e.g. `{ "text": "DirectX 12 (Feature level 12_2)" }`. Very slow (~30 s, dxdiag). Called once per page open (cached by the page until Refresh); the row shows "Detecting..." meanwhile. Backend should cache the result for the session.

Health rule (UI side, from live data): memory >= 85 %, CPU >= 90 %, any drive >= 90 %, uptime >= 14 days -> warning banner text, otherwise "Your PC is running well".
