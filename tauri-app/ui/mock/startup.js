// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock } from '../js/mock.js';

const BR = '"C:\\Program Files (x86)\\BraveSoftware\\Update\\BraveUpdate.exe"';
let items = [
  ['task', '\\ZoomUpdateTaskUser-S-1-5-21-3499974983-7565', '"C:\\Users\\mohdb\\AppData\\Roaming\\Zoom\\bin\\Zoom.exe" --action=UpdateSchedule', 'Ready'],
  ...[1, 2, 3, 4, 5].map((i) => ['task', `\\SoftLanding\\S-1-5-21-3499974983-7565-${i}`, '', 'Ready']),
  ['task', '\\Seelen\\Seelen UI Service', 'C:\\Users\\mohdb\\AppData\\Local\\Microsoft\\WindowsApps\\slu-service.exe --startup', 'Ready'],
  ['task', '\\NVIDIA App SelfUpdate_{B2FE1952-0186-46C3}', '"C:\\Program Files\\NVIDIA Corporation\\NVIDIA App\\CEF\\NVIDIA App.exe"', 'Ready'],
  ['task', '\\NvDriverUpdateCheckDaily_{B2FE1952-0186}', '"C:\\Program Files\\NVIDIA Corporation\\NvContainer\\nvcontainer.exe" -d "C:\\Program Files\\NVIDIA Corporation\\NvContainer\\NvContainer.dll,OpenLibrary"', 'Ready'],
  ['task', '\\MicrosoftEdgeUpdateTaskMachineUA{1}', 'C:\\Program Files (x86)\\Microsoft\\EdgeUpdate\\MicrosoftEdgeUpdate.exe /ua /installsource scheduler', 'Ready'],
  ['task', '\\MicrosoftEdgeUpdateTaskMachineCore{1}', 'C:\\Program Files (x86)\\Microsoft\\EdgeUpdate\\MicrosoftEdgeUpdate.exe /c', 'Ready'],
  ['task', '\\GCC', '"C:\\Program Files\\GIGABYTE\\Control Center\\GCC.exe" -b', 'Ready'],
  ['task', '\\BraveSoftwareUpdateTaskMachineUA{1}', BR + ' /ua /installsource scheduler', 'Ready'],
  ['task', '\\BraveSoftwareUpdateTaskMachineCore{1}', BR + ' /c', 'Ready'],
  ['startup', 'SecurityHealth', '%windir%\\system32\\SecurityHealthSystray.exe', 'HKLM Run'],
  ['startup', 'Riot Vanguard', '"C:\\Program Files\\Riot Vanguard\\vgtray.exe"', 'HKLM Run'],
  ['startup', 'RtkAudUService', '"C:\\WINDOWS\\System32\\DriverStore\\FileRepository\\realtekservice.inf_amd64_9873da36e3c839c5\\RtkAudUService64.exe" -background', 'HKLM Run'],
  ['startup', 'Discord', 'C:\\ProgramData\\SquirrelMachineInstalls\\Discord.exe --checkInstall', 'HKLM Run'],
  ['startup', 'Discord', '"C:\\Users\\mohdb\\AppData\\Local\\Discord\\Update.exe" --processStart Discord.exe', 'HKCU Run'],
  ['startup', 'WallpaperEngine', '"C:\\Program Files (x86)\\Steam\\steamapps\\common\\wallpaper_engine\\wallpaper64.exe" -silent', 'HKCU Run'],
  ['startup', 'RiotClient', 'C:\\Riot Games\\Riot Client\\RiotClientServices.exe --launch-background-mode', 'HKCU Run'],
  ['startup', 'LGHUB', '"C:\\Program Files\\LGHUB\\system_tray\\lghub_system_tray.exe" --minimized', 'HKCU Run'],
].map(([kind, name, command, loc], i) => ({ id: `${kind[0]}${i}`, kind, name, command, location: kind === 'task' ? '' : loc, status: kind === 'task' ? loc : '' }));

mock('list_startup', () => ({ items }));
mock('remove_startup', ({ ids }) => ({
  results: ids.map((id) => {
    const it = items.find((x) => x.id === id);
    if (!it) return { id, ok: true, message: 'already gone' };
    if (it.name === 'RtkAudUService') return { id, ok: false, message: 'Access is denied' };
    items = items.filter((x) => x.id !== id);
    return { id, ok: true, message: it.kind === 'task' ? 'task deleted' : 'startup entry removed' };
  }),
}));
