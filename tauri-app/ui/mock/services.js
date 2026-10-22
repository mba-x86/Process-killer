// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock } from '../js/mock.js';

const SV = 'C:\\WINDOWS\\system32\\svchost.exe -k ';
const seed = [
  ['ActiveX Installer (AxInstSV)', 'AxInstSV', 'stopped', 'manual', SV + 'AxInstSVGroup'],
  ['Agent Activation Runtime_17d4ee', 'AarSvc_17d4ee', 'stopped', 'manual', SV + 'AarSvcGroup -p'],
  ['Aggregated Data Platform Service', 'ADPSvc', 'stopped', 'manual', SV + 'LocalServiceAndNoImpersonation -p'],
  ['App Readiness', 'AppReadiness', 'stopped', 'manual', 'C:\\WINDOWS\\System32\\svchost.exe -k AppReadiness'],
  ['Application Identity', 'AppIDSvc', 'stopped', 'manual', SV + 'LocalServiceNetworkRestricted -p'],
  ['Application Information', 'Appinfo', 'running', 'manual', SV + 'netsvcs -p'],
  ['Application Layer Gateway Service', 'ALG', 'stopped', 'manual', 'C:\\WINDOWS\\System32\\alg.exe'],
  ['Application Management', 'AppMgmt', 'stopped', 'manual', SV + 'netsvcs -p'],
  ['AppX Deployment Service (AppXSVC)', 'AppXSvc', 'running', 'automatic', SV + 'wsappx -p'],
  ['AssignedAccessManager Service', 'AssignedAccessManager', 'stopped', 'manual', SV + 'AssignedAccessManagerSvc'],
  ['Auto Time Zone Updater', 'tzautoupdate', 'stopped', 'manual', SV + 'LocalService -p'],
  ['AVCTP service', 'BthAvctpSvc', 'stopped', 'manual', SV + 'LocalServiceRestricted -p'],
  ['Background Intelligent Transfer Service', 'BITS', 'stopped', 'manual', 'C:\\WINDOWS\\System32\\svchost.exe -k netsvcs -p'],
  ['Background Tasks Infrastructure Service', 'BrokerInfrastructure', 'running', 'automatic', SV + 'DcomLaunch -p'],
  ['Base Filtering Engine', 'BFE', 'running', 'automatic', SV + 'LocalServiceNoNetworkFirewall -p'],
  ['Battle.net Update Helper Svc', 'battlenet_helpersvc', 'stopped', 'manual', 'C:\\ProgramData\\Battle.net_components\\battlenet_helpersvc\\battlenet_helpersvc.exe', false],
  ['BitLocker Drive Encryption Service', 'BDESVC', 'stopped', 'manual', 'C:\\WINDOWS\\System32\\svchost.exe -k netsvcs -p'],
  ['Block Level Backup Engine Service', 'wbengine', 'stopped', 'manual', '"C:\\WINDOWS\\system32\\wbengine.exe"'],
  ['Bluetooth Audio Gateway Service', 'BTAGService', 'running', 'automatic', SV + 'LocalServiceNetworkRestricted -p'],
  ['Bluetooth Support Service', 'bthserv', 'running', 'automatic', SV + 'LocalService -p'],
  ['BranchCache', 'PeerDistSvc', 'stopped', 'manual', 'C:\\WINDOWS\\System32\\svchost.exe -k PeerDist'],
  ['Brave Elevation Service (BraveElevationService)', 'BraveElevationService', 'stopped', 'manual', '"C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\elevation_service.exe"', false],
  ['Brave Update Service (brave)', 'brave', 'stopped', 'automatic', '"C:\\Program Files (x86)\\BraveSoftware\\Update\\BraveUpdate.exe" /svc', false],
  ['NVIDIA Display Container LS', 'NVDisplay.ContainerLocalSystem', 'running', 'automatic', '"C:\\Windows\\System32\\DriverStore\\FileRepository\\nv_dispig.inf\\Display.NvContainer\\NVDisplay.Container.exe" -s NVDisplay.ContainerLocalSystem -f'],
  ['Razer Game Manager Service', 'RzActionSvc', 'running', 'automatic', '"C:\\Program Files (x86)\\Razer\\Razer Services\\Razer Central\\RzActionSvc.exe"', false],
  ['Windows Update', 'wuauserv', 'running', 'manual', SV + 'netsvcs -p'],
  ['Print Spooler', 'Spooler', 'running', 'automatic', 'C:\\WINDOWS\\System32\\spoolsv.exe'],
  ['Windows Defender Antivirus Service', 'WinDefend', 'running', 'automatic', '"C:\\ProgramData\\Microsoft\\Windows Defender\\Platform\\4.18.26090.9-0\\MsMpEng.exe"'],
];
const rows = seed.map(([display, name, status, start, path, system = true]) => ({ display, name, status, start, path, system }));
const kinds = ['LocalService', 'netsvcs', 'LocalSystemNetworkRestricted', 'UnistackSvcGroup', 'NetworkService'];
for (let i = 0; rows.length < 308; i++) {
  const n = `Svc${i.toString(36).toUpperCase()}${100 + i}`;
  rows.push({ display: `Windows Component Service ${i + 1}`, name: n, status: i % 5 === 0 ? 'running' : 'stopped',
    start: i % 5 === 0 ? 'automatic' : i % 9 === 0 ? 'disabled' : 'manual', path: SV + kinds[i % kinds.length] + ' -p', system: true });
}

mock('list_services', () => ({ services: rows }));
mock('set_service_state', ({ names, action }) => ({
  results: names.map((name) => {
    const s = rows.find((r) => r.name === name);
    if (!s) return { name, ok: false, message: 'service not found' };
    if (name === 'Spooler' && action === 'stop') return { name, ok: false, message: 'Access is denied (administrator needed)' };
    if (action === 'start') s.status = 'running';
    else if (action === 'stop') s.status = 'stopped';
    else if (action === 'restart') s.status = 'running';
    else if (action === 'enable') s.start = 'automatic';
    else if (action === 'manual') s.start = 'manual';
    else if (action === 'disable') { s.start = 'disabled'; s.status = 'stopped'; }
    return { name, ok: true, message: 'ok' };
  }),
}));
