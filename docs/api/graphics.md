# Graphics page API

No other commands. No admin needed. The page keeps the 60-sample utilization history (and per-engine histories) itself.

## `get_gpu_detail()` - poll every 2000 ms
```json
{ "live_supported": true,
  "adapters": [ {
    "id": "PCI\\VEN_10DE&DEV_2D05", "name": "NVIDIA GeForce RTX 5060", "vendor": "NVIDIA",
    "vram_total": 8589934592, "driver": "32.0.15.8157", "driver_date": "2026-03-18", "directx": "12 (feature level 12_2)",
    "util_percent": 6.4, "vram_used": 1717986918, "temperature_c": 44,
    "clock_mhz": 1950, "power_w": 31,
    "engines": [ { "name": "3D", "percent": 6.4 }, { "name": "Copy", "percent": 0.8 }, { "name": "Video decode", "percent": 2 }, { "name": "Video encode", "percent": 0 }, { "name": "Compute", "percent": 3 } ]
  } ] }
```
- Required: `id`, `name`, `vendor`, `vram_total` (bytes, `null` if unknown; note `Win32_VideoController.AdapterRAM` caps at 4 GB - prefer DXGI `DedicatedVideoMemory` or the registry `HardwareInformation.qwMemorySize`), `driver`, `directx` (string or `null`), `util_percent`, `vram_used`, `temperature_c`.
- **Live values are `null` whenever Windows cannot provide them** - never estimate. Sources: PDH counters `\GPU Engine(*)\Utilization Percentage` (sum per adapter LUID, max over engine types for `util_percent`), `\GPU Adapter Memory(*)\Dedicated Usage` (`vram_used`); temperature only via vendor APIs (NVML/NVAPI/ADL) else `null`. WDDM < 2.0 / older GPUs have no counters.
- `live_supported`: `false` when none of the adapters provide counters (page then shows the "Live usage is not available for this GPU" state and a "Static info" pill instead of "Live").
- Optional extras (all nullable, page shows "N/A"): `driver_date` (YYYY-MM-DD), `clock_mhz`, `power_w`, `engines` (array of `{name, percent}`; names `3D`, `Copy`, `Video decode`, `Video encode`, `Compute`; `null` = per-engine data unavailable - the Engines card then shows an explanatory state). Keep the set/order of engine names stable between ticks.
- Adapters: skip software adapters (Microsoft Basic Render Driver, remote display). The page lets the user switch when more than one is returned (default: first with live util, else first). `id` must be stable between calls.
- Static fields (name, driver, dx, vram_total) may be cached by the backend; only counters need refreshing per call. Keep the call < 50 ms (PDH query on a persistent handle).
- Errors: thrown `Error(message)`; the page ignores a failed tick.
