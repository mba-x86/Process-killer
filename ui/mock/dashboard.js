// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock, wait } from '../js/mock.js';

const FAST = [
  { title: 'Device', rows: [['Device name', 'DESKTOP-JOGUA40'], ['Manufacturer', 'Gigabyte Technology Co., Ltd.'], ['Model', 'B760M D3HP DDR4'], ['Type', 'Desktop'], ['User', 'mohdb'], ['Domain / workgroup', 'WORKGROUP']] },
  { title: 'Operating system', rows: [['Edition', 'Microsoft Windows 11 Pro'], ['Version', '10.0.26300 (build 26300)'], ['Architecture', '64-bit'], ['Installed on', '2026-02-14']] },
  { title: 'Security & firmware', rows: [['Firmware mode', 'UEFI'], ['Secure Boot', 'Enabled', 'ok'], ['TPM', 'Version 2.0 · enabled', 'ok'], ['Virtualization', 'Enabled', 'ok']] },
  { title: 'Processor', rows: [['Model', 'Intel(R) Core(TM) i5-14400F'], ['Cores / threads', '10 cores / 16 threads'], ['Base speed', '2.50 GHz']] },
  { title: 'Memory', rows: [['Total', '15.8 GB']] },
];
mock('get_specs_fast', () => ({ sections: FAST }));

mock('get_specs_full', async () => {
  await wait(1800);
  const s = structuredClone(FAST);
  s[3].rows.push(['Cache', 'L2 10 MB · L3 20 MB'], ['Socket', 'U3E1']);
  s[4].rows = [['Total', '15.8 GB'], ['Type', 'DDR4 · 3200 MHz'], ['Slots used', '2 of 4'], ['Modules', '2 x 8 GB (Kingston)']];
  s.splice(5, 0,
    { title: 'Graphics', rows: [['Adapter', 'NVIDIA GeForce RTX 5060'], ['Video memory', '8 GB'], ['Driver', '32.0.15.8157']] },
    { title: 'Motherboard & BIOS', rows: [['Board', 'Gigabyte B760M D3HP DDR4'], ['BIOS', 'American Megatrends F8, 2024-05-02']] },
    { title: 'Storage devices', rows: [['Disk 0', 'Samsung SSD 980 PRO 500GB · 465.8 GB'], ['Disk 1', 'Kingston SA400 240GB · 223.6 GB']] },
    { title: 'Network', rows: [['Adapter', 'Realtek PCIe 2.5GbE · 192.168.1.23']] });
  return { sections: s };
});

mock('get_directx', async () => { await wait(3000); return { text: 'DirectX 12 (Feature level 12_2)' }; });

mock('boost_ram', async () => { await wait(600); return { freed: 812 * 1048576, trimmed: 64, purged: 0, percent: 58 }; });
