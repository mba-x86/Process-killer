// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock, walk } from '../js/mock.js';

const GB = 1024 ** 3;
let down = 21400, up = 12000, rx = 38.4 * GB, tx = 4.1 * GB;

mock('get_net_detail', () => {
  down = walk(down, 25000, 0, 3e6); up = walk(up, 9000, 0, 5e5);
  if (Math.random() < 0.06) down += 400000;
  rx += down; tx += up;
  return { adapters: [
    { id: '{9D1A0F52-1B6E-4C0B-8E55-2A1B7C1E0001}', name: 'Realtek PCIe 2.5GbE Family Controller', kind: 'Ethernet', up: true, ip: '192.168.1.23', gateway: '192.168.1.1',
      dns: ['192.168.1.1', '1.1.1.1'], mac: 'A8-5E-45-12-9C-3F', link_mbps: 2500, down_bps: down, up_bps: up, total_down: rx, total_up: tx, ssid: null },
    { id: '{9D1A0F52-1B6E-4C0B-8E55-2A1B7C1E0002}', name: 'Intel(R) Wi-Fi 6 AX201 160MHz', kind: 'Wi-Fi', up: false, ip: null, gateway: null, dns: [], mac: 'F4-6A-DD-01-22-B7',
      link_mbps: null, down_bps: 0, up_bps: 0, total_down: 0.9 * GB, total_up: 0.1 * GB, ssid: null },
  ] };
});
