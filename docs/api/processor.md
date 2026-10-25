# Processor API

Uses shared: `get_live` every 1500 ms (`cpu.percent`, `cpu.per_core`, `cpu.freq_mhz`), `get_overview` (uptime, ticks locally), `get_top_processes {by:"cpu",limit:6}` every 3 s.

## `get_cpu_detail()` -> object   (called once per page open; may take ~1 s, not polled)
```json
{ "model": "Intel(R) Core(TM) i5-14400F", "socket": "U3E1", "architecture": "64-bit",
  "cores_p": 6, "cores_e": 4, "threads": 16, "base_mhz": 2500, "max_mhz": null,
  "l2_bytes": 10485760, "l3_bytes": 20971520, "virtualization": true,
  "cores": [ { "id": 0, "kind": "P" }, { "id": 12, "kind": "E" } ], "process_count": 271 }
```
- Any field may be `null`/missing: the UI omits that row (e.g. `max_mhz` is not readable on most PCs; never fake it).
- `cores_p`/`cores_e`: physical cores per kind; on non-hybrid CPUs set `cores_p` = physical cores and `cores_e` = 0 (no "(x P + y E)" suffix).
- `cores`: one entry per logical processor, same order as `get_live.cpu.per_core`; `kind` "P" or "E" (non-hybrid: all "P"; the P/E legend is hidden when there is no E core).
- `freq_mhz` from `get_live` is shown as "Speed now"; on many CPUs it is the base clock, so the UI does not claim boost.
- Errors: page still renders without the specs if the call fails. No admin needed.
