# Memory page API

Also uses SHARED: `get_overview` (model), `get_live` (1000 ms: `mem.used/available/total/percent` feed the chart), `get_top_apps({by:"memory",limit:6})` (3000 ms), `boost_ram` (owned by the dashboard/optimizer docs).
The page keeps the rolling 60-sample history itself. No admin needed for any command below.

## `get_mem_detail()` - poll every 1500 ms, cheap (no process enumeration)
```json
{ "total": 16966000000, "used": 10520000000, "available": 6446000000, "cached": 3900000000,
  "committed": 15139000000, "commit_limit": 24267000000,
  "paged_pool": 429000000, "nonpaged_pool": 335000000,
  "compressed": 1288000000, "page_file_total": 7300000000, "page_file_used": 2000000000 }
```
- Bytes. `compressed` = size of the MemCompression process working set, or `null` when not readable/not present.
- `page_file_total` is 0 when no page file exists. Sources: `GlobalMemoryStatusEx`, `GetPerformanceInfo` (PageSize * counts), compressed from process list (name `Memory Compression`).
- Errors: thrown `Error(message)`; the page ignores a failed tick and retries.

## `get_mem_hw()` - called once per page open (may take up to ~1 s; WMI `Win32_PhysicalMemory` / `Win32_PhysicalMemoryArray`)
```json
{ "type": "DDR4", "speed_mhz": 3200, "slots_used": 2, "slots_total": 4, "form_factor": "DIMM",
  "modules": [ { "slot": "DIMM_A1", "size": 8589934592, "manufacturer": "Kingston", "part_number": "KF432C16BB/8", "speed_mhz": 3200 } ] }
```
- `type`: `"DDR3" | "DDR4" | "DDR5" | "LPDDR4" | ...` from SMBIOSMemoryType; `null` when unknown.
- `speed_mhz`: configured speed (shown as MT/s); `null` if unknown. `slots_total` / `form_factor` may be `null`.
- Page shows module list as a tooltip on the "Slots used" value.
