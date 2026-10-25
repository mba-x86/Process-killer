# Network page API

Also uses SHARED: `get_overview` (uptime for the "since start-up" caption). The page keeps 60-sample rolling histories (and session peaks) per adapter itself; peaks are "since the page was opened", not persisted.
No admin needed.

## `get_net_detail()` - poll every 1000 ms, cheap (no WMI per tick; cache static fields like ip/mac/ssid/gateway/dns and refresh them every ~10 s)
```json
{ "adapters": [
  { "id": "{9D1A0F52-...}", "name": "Realtek PCIe 2.5GbE Family Controller", "kind": "Ethernet", "up": true,
    "ip": "192.168.1.23", "gateway": "192.168.1.1", "dns": ["192.168.1.1", "1.1.1.1"], "mac": "A8-5E-45-12-9C-3F",
    "link_mbps": 2500, "down_bps": 21400, "up_bps": 12000, "total_down": 41234567890, "total_up": 4400000000, "ssid": null }
] }
```
- `id`: stable adapter identifier (interface GUID/LUID as string). `kind`: `"Ethernet" | "Wi-Fi" | "Other"`.
- `up`: operational status is Up. `ip`: first IPv4 address or `null`. `gateway`: IPv4 default gateway or `null`. `dns`: array (maybe empty).
- `link_mbps`: link speed in Mbit/s or `null`. `down_bps` / `up_bps`: bytes per second computed from the interface octet counters delta (`GetIfTable2`/`GetIfEntry2`), 0 when down.
- `total_down` / `total_up`: octet counters (bytes) since the interface came up / Windows started.
- `ssid`: connected Wi-Fi network name or `null`.
- Skip loopback/tunnel/virtual pseudo-adapters (Teredo, ISATAP, Loopback). Virtual adapters (Hyper-V, VPN) may appear as `Other`; list order: Ethernet, Wi-Fi, Other.
- `gateway` and `dns` are extras beyond the minimal contract: if the backend cannot provide them it may return `null` / `[]`, the page shows an em dash.
- Errors: thrown `Error(message)`; the page ignores a failed tick.
