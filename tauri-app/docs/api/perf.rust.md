# perf - handler registration
Add to `generate_handler![...]`:
```
perf::get_mem_detail,
perf::get_mem_hw,
perf::get_net_detail,
perf::get_gpu_detail,
```
No `app.manage` state needed (statics inside perf.rs).
Notes: vram_used comes from PDH `GPU Adapter Memory\Dedicated Usage` (DXGI QueryVideoMemoryInfo is per-process, so not used); temperature/clock/power are null; Wi-Fi SSID via dynamically loaded wlanapi.dll.
