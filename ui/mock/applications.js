// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Mock backend for the Applications page (contract: docs/api/applications.md).
import { mock, emit, wait } from '../js/mock.js';

const MB = 1024 ** 2, GB = 1024 ** 3;
const admin = new URLSearchParams(location.search).get('admin') !== '0';

// [name, publisher, version, size, date, location]
const RAW = [
  ['2XKO', 'Riot Games, Inc', '', 0, '', 'C:\\Riot Games\\2XKO'],
  ['Android SDK Platform-Tools', 'Google LLC', '37.0.1', 0, '2026-10-05', ''],
  ['Android Studio', 'Google LLC', '2026.1', 0, '', 'C:\\Program Files\\Android\\Android Studio'],
  ['AULA F75', 'AULA', '2.0', 0, '2026-04-07', 'C:\\Program Files (x86)\\AULA F75'],
  ['Battlefield\u2122 6', 'Battlefield Studios', '', 0, '', 'E:\\Games\\Battlefield 6'],
  ['Brave', 'Brave Software Inc', '155.1.97.59', 544 * MB, '2026-10-16', 'C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application'],
  ["CnCNet Yuri's Revenge 9.3.2", 'cncnet.org', '9.3.2', 621 * MB, '2026-08-31', 'E:\\Games\\CnCNet'],
  ['Corsair Device Control Service Bundle', 'Corsair', '1.9.9', 20 * MB, '', 'C:\\Program Files\\Corsair\\Corsair Device Control Service'],
  ['Counter-Strike 2', 'Valve', '', 0, '', 'E:\\SteamLibrary\\steamapps\\common\\Counter-Strike Global Offensive'],
  ['Discord', 'Discord Inc.', '1.0.9262', 117 * MB, 'Unknown', 'C:\\Users\\Amr\\AppData\\Local\\Discord'],
  ['Docker Desktop', 'Docker Inc.', '4.85.0', 3.3 * GB, '', 'C:\\Program Files\\Docker\\Docker'],
  ['EA app', 'Electronic Arts', '13.796.0.6309', 773 * MB, '', 'C:\\Program Files\\Electronic Arts\\EA Desktop'],
  ['EA SPORTS FC\u2122 26', 'EA Canada', '', 0, '', 'E:\\Games\\EA SPORTS FC 26'],
  ['emu8086 microprocessor emulator', 'emu8086', '', 10 * MB, '2026-06-16', 'C:\\emu8086'],
  ['GBT_Dynamic_Lighting_Lib_UC 25.07.21.01', 'GIGABYTE', '25.07.21.01', 0, '', ''],
  ['GBT_MB_Update', 'GIGABYTE', '26.07.22.01', 0, '', ''],
  ['GBT_RGB_Sync_Control 26.08.31.01', 'GIGABYTE', '26.08.31.01', 0, '', ''],
  ['GBT_rgbMotherboard_UC 26.08.27.01', 'GIGABYTE', '26.08.27.01', 0, '', ''],
  ['GBT_VGA 26.08.24.01', 'GIGABYTE', '26.08.24.01', 0, '', ''],
  ['GIGABYTE Control Center 26.08.28.01', 'GIGABYTE', '26.08.28.01', 0, '', 'C:\\Program Files\\GIGABYTE\\Control Center'],
  ['GIGABYTE Performance Library', 'GIGABYTE', '26.08.06.01', 0, '', ''],
  ['GIGABYTE SSD Firmware Update Tool', 'GIGABYTE', '24.06.19.01', 0, '', ''],
  ['GIGABYTE Storage Library', 'GIGABYTE', '26.09.18.01', 0, '', ''],
  ['Git', 'The Git Development Community', '2.53.0.2', 329 * MB, '2026-04-02', 'C:\\Program Files\\Git'],
  ['GitHub CLI', 'GitHub, Inc.', '2.102.0', 41 * MB, '2026-10-13', 'C:\\Program Files\\GitHub CLI'],
  ['Men of War: Assault Squad 2 Airborne', '1C Entertainment', '3.262.0', 4.2 * GB, '2026-03-11', 'E:\\Games\\Men of War Assault Squad 2'],
  ['Microsoft Edge', 'Microsoft Corporation', '141.0.3537.71', 0, '', 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application'],
  ['Microsoft Edge WebView2 Runtime', 'Microsoft Corporation', '141.0.3537.71', 0, '', 'C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\Application'],
  ['Microsoft Visual C++ 2015-2022 Redistributable (x64)', 'Microsoft Corporation', '14.44.35211', 18 * MB, '2026-05-02', ''],
  ['Microsoft Visual C++ 2015-2022 Redistributable (x86)', 'Microsoft Corporation', '14.44.35211', 17 * MB, '2026-05-02', ''],
  ['Microsoft Visual Studio Code', 'Microsoft Corporation', '1.105.1', 412 * MB, '2026-10-14', 'C:\\Users\\Amr\\AppData\\Local\\Programs\\Microsoft VS Code'],
  ['Microsoft .NET Runtime 8.0.20 (x64)', 'Microsoft Corporation', '8.0.20.34912', 74 * MB, '2026-09-10', ''],
  ['Microsoft .NET Desktop Runtime 9.0.9 (x64)', 'Microsoft Corporation', '9.0.9.35012', 213 * MB, '2026-09-10', ''],
  ['Node.js', 'Node.js Foundation', '22.20.0', 90 * MB, '2026-09-24', 'C:\\Program Files\\nodejs'],
  ['Notepad++', 'Notepad++ Team', '8.8.5', 12 * MB, '2026-08-02', 'C:\\Program Files\\Notepad++'],
  ['NVIDIA Graphics Driver 581.57', 'NVIDIA Corporation', '581.57', 0, '2026-10-02', ''],
  ['NVIDIA App', 'NVIDIA Corporation', '11.0.5.420', 327 * MB, '2026-10-02', 'C:\\Program Files\\NVIDIA Corporation\\NVIDIA App'],
  ['OBS Studio', 'OBS Project', '31.1.2', 345 * MB, '2026-07-19', 'C:\\Program Files\\obs-studio'],
  ['Python 3.13.7 (64-bit)', 'Python Software Foundation', '3.13.7150.0', 101 * MB, '2026-09-02', 'C:\\Users\\Amr\\AppData\\Local\\Programs\\Python\\Python313'],
  ['Realtek Audio Driver', 'Realtek Semiconductor Corp.', '6.0.9800.1', 0, '2026-03-30', ''],
  ['Steam', 'Valve Corporation', '2.10.91.91', 0, '', 'C:\\Program Files (x86)\\Steam'],
  ['Telegram Desktop', 'Telegram FZ-LLC', '6.1.4', 168 * MB, '2026-09-28', 'C:\\Users\\Amr\\AppData\\Roaming\\Telegram Desktop'],
  ['7-Zip 25.01 (x64)', 'Igor Pavlov', '25.01', 6 * MB, '2026-08-11', 'C:\\Program Files\\7-Zip'],
  ['VLC media player', 'VideoLAN', '3.0.21', 163 * MB, '2026-02-17', 'C:\\Program Files\\VideoLAN\\VLC'],
  ['WinRAR 7.13 (64-bit)', 'win.rar GmbH', '7.13.0', 9 * MB, '2026-05-22', 'C:\\Program Files\\WinRAR'],
  ['Zoom Workplace (64-bit)', 'Zoom Communications, Inc.', '6.6.2', 405 * MB, '2026-10-09', 'C:\\Program Files\\Zoom\\bin'],
];
const FILL = ['Visual Studio Build Tools', 'Windows SDK AddOn', 'Intel Chipset Device Software', 'Intel Management Engine Components',
  'Logitech G HUB', 'Wireshark', 'Blender', 'Unity Hub', 'Epic Games Launcher', 'qBittorrent', 'Paint.NET', 'ShareX', 'HWiNFO64',
  'CPUID CPU-Z', 'MSI Afterburner', 'Rufus', 'Spotify', 'WhatsApp', 'LibreOffice 25.8', 'Java 8 Update 461', 'Cisco Packet Tracer',
  'XAMPP', 'MySQL Workbench 8.4 CE', 'Postman', 'FileZilla Client', 'AnyDesk', 'TeamViewer', 'GIMP 3.0.4', 'Audacity', 'Inno Setup 6'];
const mk = ([name, publisher, version, size, date, location], i) => {
  const broken = name.startsWith('Men of War');
  const edge = name === 'Microsoft Edge';
  return { id: `app${i + 1}`, name, publisher, version, size, date, location,
    drive: /^[A-Za-z]:/.test(location) ? location.slice(0, 2).toUpperCase() + '\\' : '',
    uninstaller_missing: broken, is_edge: edge, can_silent: !broken && !edge && !/Driver|GBT_|GIGABYTE|Runtime|Redistributable/.test(name) };
};
let apps = RAW.map(mk);
for (let i = 0; apps.length < 94; i++) {
  const n = FILL[i % FILL.length] + (i >= FILL.length ? ` ${i}` : '');
  apps.push(mk([n, ['Vendor Inc.', 'Open Source Team', 'Contoso Ltd'][i % 3], `${1 + (i % 9)}.${i % 7}.${i * 3 % 40}`, (i % 4 ? (20 + i * 11) * MB : 0),
    i % 3 ? `2026-0${1 + (i % 9)}-1${i % 9}` : '', i % 4 === 0 ? '' : `${i % 5 === 0 ? 'E' : 'C'}:\\Program Files\\${n}`], apps.length));
}
const byId = (id) => apps.find((a) => a.id === id);
const first = (a) => a.name.split(/[ .:]/)[0];

/* ---- related items --------------------------------------------------------------------------------------------- */
const it = (id, kind, name, detail, state, reason, action_label, extra = {}) => ({ id, kind, name, detail, state, reason, protected: false, action_label, ...extra });
function related(a) {
  const k = a.id;
  if (a.is_edge) return [
    it(`${k}:p1`, 'process', 'msedge.exe', '', 'Running', 'path', 'End process', { count: 18, mem: 1.1 * GB }),
    it(`${k}:p2`, 'process', 'msedgewebview2.exe', '', 'Running', 'name', 'End process', { count: 9, mem: 0.4 * GB, protected: true }),
    it(`${k}:s1`, 'service', 'Microsoft Edge Update Service (edgeupdate)', 'Service \u00b7 Automatic', 'Stopped', 'path', 'Stop service'),
    it(`${k}:s2`, 'service', 'Microsoft Edge Elevation Service', 'Service \u00b7 Manual', 'Stopped', 'path', 'Stop service'),
    it(`${k}:t1`, 'task', 'MicrosoftEdgeUpdateTaskMachineCore', '2 scheduled tasks', 'Ready', 'path', 'Delete task'),
    it(`${k}:u1`, 'startup', 'MicrosoftEdgeAutoLaunch_8A2F', 'Startup entry \u00b7 Current user', 'Enabled', 'name', 'Remove entry')];
  if (a.name === 'Brave') return [
    it(`${k}:p1`, 'process', 'brave.exe', '', 'Running', 'path', 'End process', { count: 21, mem: 3.4 * GB }),
    it(`${k}:s1`, 'service', 'Brave Elevation Service', 'Service \u00b7 Manual', 'Stopped', 'path', 'Stop service'),
    it(`${k}:s2`, 'service', 'Brave Update Service', 'Service \u00b7 Automatic', 'Stopped', 'path', 'Stop service'),
    it(`${k}:t1`, 'task', 'Brave update tasks', '2 scheduled tasks', 'Ready', 'path', 'Delete task'),
    it(`${k}:p9`, 'process', 'BraveCrashHandler.exe', '', 'Running', 'name', 'End process', { count: 1, mem: 6 * MB })];
  if (a.name === 'Discord') return [
    it(`${k}:p1`, 'process', 'Discord.exe', '', 'Running', 'path', 'End process', { count: 7, mem: 0.52 * GB }),
    it(`${k}:u1`, 'startup', 'Discord', 'Startup entry \u00b7 Current user', 'Enabled', 'path', 'Remove entry')];
  if (a.name === 'Docker Desktop') return [
    it(`${k}:p1`, 'process', 'Docker Desktop.exe', '', 'Running', 'path', 'End process', { count: 5, mem: 0.9 * GB }),
    it(`${k}:s1`, 'service', 'Docker Desktop Service', 'Service \u00b7 Automatic', 'Running', 'path', 'Stop service')];
  if (a.uninstaller_missing || a.drive === '' || a.size === 0) return [];
  if (/Code|Zoom|Steam|Telegram|OBS|Notepad/.test(a.name)) return [
    it(`${k}:p1`, 'process', `${first(a)}.exe`, '', 'Running', 'path', 'End process', { count: 3, mem: 0.21 * GB })];
  return [];
}
mock('list_apps', async () => { await wait(350); return { apps }; });
mock('get_related', async ({ app_id }) => { await wait(300 + Math.random() * 400); return { items: related(byId(app_id)) }; });
mock('other_browsers', () => ({ names: ['Brave', 'Google Chrome'] }));
mock('open_folder', ({ path }) => { console.info('open_folder', path); return null; });

/* ---- uninstall job --------------------------------------------------------------------------------------------- */
let jobN = 0;
mock('uninstall_start', ({ app_id, stop_ids = [], silent, block_edge_reinstall }) => {
  const a = byId(app_id), job = `job${++jobN}`, items = related(a);
  if (a.is_edge && !admin) throw new Error('Removing Microsoft Edge needs Administrator rights.');
  (async () => {
    const say = async (level, text, ms = 500) => { await wait(ms); emit('uninstall:progress', { job, level, text }); };
    for (const id of stop_ids) {
      const r = items.find((x) => x.id === id);
      if (!r || r.protected) continue;
      const label = { process: 'Process', service: 'Service', startup: 'Startup entry', task: 'Scheduled task' }[r.kind];
      await say('ok', `${label} \u201c${r.name}\u201d \u2014 ${r.kind === 'process' ? `ended ${r.count || 1} process(es)` : r.kind === 'service' ? 'stopped' : 'removed'}`, 450);
    }
    if (a.uninstaller_missing) {
      await say('info', 'Removing the dead entry from Installed apps\u2026');
      await say('ok', 'Backup saved to %APPDATA%\\ProcessKiller\\backups\\Men-of-War-Assault-Squad-2.reg', 700);
      await say('ok', 'Removed the dead entry.', 400);
      apps = apps.filter((x) => x.id !== a.id);
      emit('uninstall:done', { job, ok: true, message: 'Removed the dead entry.', needs_restart: false }); return;
    }
    await say('info', silent ? 'Starting the silent uninstaller\u2026' : 'Starting the uninstaller\u2026');
    await say('info', 'Waiting for the uninstaller to finish\u2026', 900);
    if (/Docker/.test(a.name)) {
      await say('fail', 'The uninstaller exited with code 1603 (fatal error during installation).', 1500);
      emit('uninstall:done', { job, ok: false, message: 'The uninstaller exited with code 1603 (fatal error during installation).', needs_restart: false }); return;
    }
    await say('ok', `${a.name} was uninstalled.`, 1500);
    if (a.is_edge && block_edge_reinstall) await say('ok', 'Edge updater told not to install Edge again (DoNotUpdateToEdgeWithChromium = 1).', 300);
    apps = apps.filter((x) => x.id !== a.id);
    emit('uninstall:done', { job, ok: true, message: `${a.name} was uninstalled.`, needs_restart: /Corsair|NVIDIA|Realtek/.test(a.name) });
  })();
  return { job };
});

/* ---- leftovers ------------------------------------------------------------------------------------------------- */
const LEFT = (a) => {
  const k = a.id, n = first(a);
  if (a.is_edge) return [
    it(`${k}:f1`, 'folder', 'Edge', 'C:\\Program Files (x86)\\Microsoft\\Edge', 'Folder \u00b7 412 MB', 'path', 'Recycle Bin', { default_on: true }),
    it(`${k}:f2`, 'folder', 'Edge User Data', 'C:\\Users\\Amr\\AppData\\Local\\Microsoft\\Edge\\User Data', 'Folder \u00b7 1.8 GB \u00b7 your profile, history and passwords', 'path', 'Recycle Bin', { default_on: false }),
    it(`${k}:t1`, 'task', 'MicrosoftEdgeUpdateTaskMachineUA', 'Scheduled task', 'Ready', 'path', 'Delete task', { default_on: true })];
  if (a.uninstaller_missing) return [
    it(`${k}:f1`, 'folder', 'Men of War Assault Squad 2', a.location, 'Folder \u00b7 4.2 GB', 'path', 'Recycle Bin', { default_on: true }),
    it(`${k}:f2`, 'folder', 'Men of War Assault Squad 2', 'C:\\Users\\Amr\\Documents\\My Games\\Men of War Assault Squad 2', 'Folder \u00b7 38 MB \u00b7 savegames', 'name', 'Recycle Bin', { default_on: false })];
  if (!a.drive) return [];
  return [
    it(`${k}:f1`, 'folder', n, a.location, 'Folder \u00b7 120 MB', 'path', 'Recycle Bin', { default_on: true }),
    it(`${k}:f2`, 'folder', n, `C:\\Users\\Amr\\AppData\\Roaming\\${n}`, 'Folder \u00b7 14 MB', 'name', 'Recycle Bin', { default_on: false }),
    it(`${k}:u1`, 'startup', n, 'Startup entry \u00b7 Current user', 'Enabled', 'path', 'Remove entry', { default_on: true })];
};
const gone = new Map();   // id -> app, so leftovers still resolve after the app left the list
mock('find_leftovers', async ({ app_id }) => { await wait(900); return { items: LEFT(byId(app_id) || gone.get(app_id)) }; });
mock('clean_leftovers', async ({ app_id, ids = [] }) => {
  await wait(1100);
  const bad = (id) => gone.get(app_id)?.is_edge && id.endsWith(':t1');
  return { results: ids.map((id) => (bad(id) ? { id, ok: false, message: 'Access is denied (the task is protected)' }
    : { id, ok: true, message: id.includes(':f') ? 'Moved to the Recycle Bin' : id.includes(':t') ? 'Task deleted' : 'Entry removed' })) };
});
const keep = apps.slice(); keep.forEach((a) => gone.set(a.id, a));
