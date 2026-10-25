// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock, walk } from '../js/mock.js';

const MB = 1048576;
const BRAVE = 'C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe';
const seed = [
  [3984, 'MemCompression', 'MemCompression', 1228, true], [1336, 'brave.exe', BRAVE, 508], [5712, 'brave.exe', BRAVE, 450],
  [11952, 'brave.exe', BRAVE, 418], [9848, 'brave.exe', BRAVE, 373], [17484, 'Discord.exe', 'C:\\Users\\mohdb\\AppData\\Local\\Discord\\app-1.0.9264\\Discord.exe', 371],
  [5588, 'MsMpEng.exe', 'C:\\ProgramData\\Microsoft\\Windows Defender\\Platform\\4.18.26090.9-0\\MsMpEng.exe', 347],
  [10688, 'brave.exe', BRAVE, 338], [3364, 'brave.exe', BRAVE, 314],
  [15956, 'TextInputHost.exe', 'C:\\Windows\\SystemApps\\MicrosoftWindows.Client.CBS_cw5n1h2txyewy\\TextInputHost.exe', 296, true],
  [13820, 'brave.exe', BRAVE, 268], [12880, 'Code.exe', 'E:\\APPS\\Microsoft VS Code\\Code.exe', 252], [17764, 'brave.exe', BRAVE, 243],
  [23408, 'claude.exe', 'C:\\Users\\mohdb\\.local\\bin\\claude.exe', 170], [3876, 'svchost.exe', 'C:\\Windows\\System32\\svchost.exe', 166, true],
  [10908, 'explorer.exe', 'C:\\Windows\\explorer.exe', 142, true], [2544, 'brave.exe', BRAVE, 109], [14740, 'brave.exe', BRAVE, 104],
  [12504, 'brave.exe', BRAVE, 100], [12060, 'StartMenuExperienceHost.exe', 'C:\\Windows\\SystemApps\\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\\StartMenuExperienceHost.exe', 98, true],
  [10444, 'Code.exe', 'E:\\APPS\\Microsoft VS Code\\Code.exe', 92],
];
let procs = seed.map(([pid, name, exe, mb, protectedP]) => ({ pid, name, exe, mem: mb * MB, cpu: Math.random() * 0.4, protected: !!protectedP }));
for (let i = procs.length; i < 271; i++) {
  const sys = i % 3 === 0;
  procs.push({ pid: 30000 + i * 4, name: sys ? 'svchost.exe' : `helper${i}.exe`, exe: sys ? 'C:\\Windows\\System32\\svchost.exe' : `C:\\Program Files\\Vendor${i % 20}\\helper${i}.exe`,
    mem: (3 + (i * 7919 % 85)) * MB, cpu: Math.random() * 0.2, protected: sys });
}

mock('list_processes', () => {
  procs.forEach((p) => { p.cpu = walk(p.cpu, 0.3, 0, 25); p.mem = Math.max(MB, p.mem + (Math.random() - 0.5) * MB); });
  return { processes: procs };
});
mock('kill_process', ({ pids, tree }) => ({
  results: pids.map((pid) => {
    const p = procs.find((x) => x.pid === pid);
    if (!p) return { pid, ok: false, message: 'process not found' };
    if (p.protected) return { pid, ok: false, message: 'protected system process' };
    procs = procs.filter((x) => x.pid !== pid);
    return { pid, ok: true, message: tree ? 'process tree ended' : 'process ended' };
  }),
}));
mock('open_location', () => null);
