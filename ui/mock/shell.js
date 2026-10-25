// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Mock implementations of the SHARED commands (docs/api/SHARED.md). Orchestrator-owned.
import { mock, walk } from '../js/mock.js';

const admin = new URLSearchParams(location.search).get('admin') !== '0';   // ?admin=0 simulates a limited session
mock('get_overview', () => ({ hostname: 'DESKTOP-JOGUA40', os: 'Microsoft Windows 11 Pro', model: 'B760M D3HP DDR4',
  uptime_secs: 2 * 3600 + 7 * 60, is_admin: admin, version: '2.0.0-dev', copyright: '© 2026 mba-x86' }));
mock('relaunch_as_admin', () => null);

const GB = 1024 ** 3;
let cpu = 10, mem = 62, down = 21400, up = 12000;
const cores = Array.from({ length: 16 }, () => 5 + Math.random() * 10);
mock('get_live', () => {
  cpu = walk(cpu, 4, 2, 60); mem = walk(mem, 0.4, 40, 90); down = walk(down, 20000, 0, 3e6); up = walk(up, 8000, 0, 5e5);
  cores.forEach((c, i) => { cores[i] = walk(c, 6, 0, 90); });
  return { ts: Math.floor(Date.now() / 1000),
    cpu: { percent: cpu, per_core: [...cores], freq_mhz: 2500, threads: 16, cores: 10 },
    mem: { total: 15.8 * GB, used: (15.8 * GB * mem) / 100, available: (15.8 * GB * (100 - mem)) / 100, percent: mem, committed: 14.1 * GB, cached: 3.9 * GB },
    net: { down_bps: down, up_bps: up },
    disk_sys: { mount: 'C:\\', total: 475.9 * GB, used: 362.3 * GB, free: 113.7 * GB, percent: 76.1 } };
});
mock('get_drives', () => [
  { mount: 'C:\\', label: 'Local disk', fstype: 'NTFS', total: 475.9 * GB, used: 362.3 * GB, free: 113.7 * GB, percent: 76.1, removable: false },
  { mount: 'E:\\', label: 'New Volume', fstype: 'NTFS', total: 238.5 * GB, used: 170.5 * GB, free: 68.0 * GB, percent: 71.5, removable: false },
]);
const APPS = [['brave.exe', 21, 3.4 * GB, 1], ['MemCompression', 1, 1.2 * GB, 0], ['svchost.exe', 86, 0.69 * GB, 1], ['Code.exe', 14, 0.6 * GB, 0],
  ['Discord.exe', 6, 0.5 * GB, 0], ['MsMpEng.exe', 1, 0.33 * GB, 0]];
mock('get_top_apps', ({ by = 'memory', limit = 6 }) => APPS.map(([name, count, m, c]) => ({ name, count, mem: m, cpu: c }))
  .sort((a, b) => (by === 'cpu' ? b.cpu - a.cpu : b.mem - a.mem)).slice(0, limit));
const PROCS = [['brave.exe', 11952, 0.4], ['MsMpEng.exe', 5588, 0.3], ['explorer.exe', 10908, 0.3], ['ProcessKiller.exe', 6300, 0.3], ['brave.exe', 1336, 0.2], ['Code.exe', 12880, 0.2]];
mock('get_top_processes', ({ by = 'memory', limit = 6 }) => PROCS.map(([name, pid, c], i) => ({ pid, name, cpu: c * (0.8 + Math.random() * 0.4), mem: (900 - i * 120) * 1048576 }))
  .sort((a, b) => (by === 'cpu' ? b.cpu - a.cpu : b.mem - a.mem)).slice(0, limit));
mock('copy_text', () => null);
