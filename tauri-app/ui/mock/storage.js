// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock, emit } from '../js/mock.js';

const MB = 1048576, GB = 1073741824, DAY = 86400, NOW = Math.floor(Date.now() / 1000);
const BASE = [
  ['C:\\pagefile.sys', 11.5 * GB], ['C:\\hiberfil.sys', 6.3 * GB], ['C:\\swapfile.sys', 256 * MB], ['C:\\Windows\\392667600.dat', 256 * MB],
  ['C:\\Windows\\WinSxS\\amd64_microsoft-windows-fonts.dll', 310 * MB], ['C:\\Windows\\Installer\\1a2b3c.msi', 180 * MB],
  ['C:\\Users\\mba\\Videos\\trip-2025.mp4', 4.2 * GB], ['C:\\Users\\mba\\Videos\\stream-vod.mkv', 2.8 * GB], ['C:\\Users\\mba\\Downloads\\Win11_24H2.iso', 5.6 * GB],
  ['C:\\Users\\mba\\Downloads\\backup-photos.zip', 1.9 * GB], ['C:\\Program Files\\Adobe\\Premiere\\app.dll', 420 * MB], ['C:\\Users\\mba\\AppData\\Local\\Temp\\setup.exe', 220 * MB],
  ['E:\\Games\\Cyber\\data.pak', 38 * GB], ['E:\\Games\\Cyber\\video.bik', 3.4 * GB], ['E:\\Projects\\render.mov', 6.1 * GB], ['E:\\Projects\\assets.psd', 1.2 * GB],
  ['E:\\Music\\library.flac', 640 * MB], ['E:\\Docs\\report.pdf', 48 * MB], ['E:\\Backups\\disk.vhdx', 22 * GB], ['E:\\Backups\\notes.txt', 12 * MB],
];
const EXTS = ['mp4', 'zip', 'iso', 'dll', 'pdf', 'log', 'dat', 'png', 'exe', 'vhdx'];
const rnd = (a, b) => a + Math.random() * (b - a);

function build(roots) {
  const low = roots.map((r) => r.toLowerCase().replace(/\\+$/, ''));
  let files = BASE.filter(([p]) => low.some((r) => p.toLowerCase().startsWith(r)));
  const base = roots[0].replace(/\\+$/, '');
  const extra = files.length < 12 ? 60 : 160;
  const subs = ['Cache', 'Data', 'Media', 'Logs', 'Old', 'Packages', 'Temp'];
  for (let i = 0; i < extra; i++) {
    const root = roots[i % roots.length].replace(/\\+$/, '');
    files.push([`${root}\\${subs[i % subs.length]}${Math.floor(i / 7)}\\file_${1000 + i}.${EXTS[i % EXTS.length]}`, Math.round(Math.pow(Math.random(), 3) * 900 * MB + 1.2 * MB)]);
  }
  void base;
  return files.map(([path, size]) => ({ path, size: Math.round(size), mtime: NOW - Math.floor(rnd(0, 400)) * DAY })).sort(() => Math.random() - 0.5);
}

const jobs = new Map();
let n = 0;
mock('storage_scan_start', ({ roots, min_size = MB }) => {
  const job = `scan${++n}`, all = build(roots), custom = roots.length === 1 && !/^[A-Za-z]:\\?$/.test(roots[0]);
  const totalBytes = custom ? null : roots.reduce((s, r) => s + (r.startsWith('C') ? 362 * GB : 170 * GB), 0);
  const st = { cancel: false }; jobs.set(job, st);
  const steps = 7, sumAll = all.reduce((s, f) => s + f.size, 0);
  (async () => {
    let i = 0, errors = 0;
    const ms = (t) => new Promise((r) => setTimeout(r, t));
    await ms(500);
    while (i < steps && !st.cancel) {
      i++;
      const part = all.slice(0, Math.ceil((all.length * i) / steps));
      const top = part.filter((f) => f.size >= min_size).sort((a, b) => b.size - a.size).slice(0, 1500);
      const fold = new Map();
      for (const f of part) {
        const rel = roots.map((r) => r.replace(/\\+$/, '')).find((r) => f.path.toLowerCase().startsWith(r.toLowerCase())) || roots[0];
        const parts = f.path.slice(rel.length).replace(/^\\/, '').split('\\');
        const key = parts.length > 1 ? `${rel}\\${parts[0]}` : `${rel}  (files in this folder)`;
        fold.set(key, (fold.get(key) || 0) + f.size);
      }
      errors += 3;
      const bytes = totalBytes ? Math.round((totalBytes * i) / steps * 0.9) : part.reduce((s, f) => s + f.size, 0);
      emit('storage:progress', { job, files: part.length * 410, bytes, errors, current: part.length ? part[part.length - 1].path.replace(/\\[^\\]*$/, '') : roots[0],
        total_bytes: totalBytes, top_files: top, folders: [...fold].map(([path, size]) => ({ path, size })) });
      void sumAll;
      if (i < steps) await ms(1500);
    }
    emit('storage:done', { job, cancelled: st.cancel, errors });
    jobs.delete(job);
  })();
  return { job };
});
mock('storage_scan_cancel', ({ job }) => { const s = jobs.get(job) || dupJobs.get(job); if (s) s.cancel = true; return null; });
// ---- duplicates (contract: docs/api/storage.md) --------------------------------------------------------
// dupes:progress {job, phase: walk|head|hash, files, candidates, hash_total, hashed, errors, skipped_cloud, current}
// dupes:done     {job, cancelled, errors, skipped_cloud, files, total_groups, total_files, wasted, groups:[{size, count, files:[{path, mtime}]}]}
const dupGroups = (roots) => {
  const r = roots[0].replace(/\\+$/, '');
  const mk = (size, names) => ({ size: Math.round(size), count: names.length, files: names.map((p, i) => ({ path: `${r}\\${p}`, mtime: NOW - (i + 1) * 9 * DAY })) });
  return [
    mk(4.2 * GB, ['Videos\\trip-2025.mp4', 'Backup\\Videos\\trip-2025.mp4', 'Downloads\\trip-2025 (1).mp4']),
    mk(1.9 * GB, ['Downloads\\backup-photos.zip', 'Archive\\backup-photos.zip']),
    mk(640 * MB, ['Music\\library.flac', 'Music\\old\\library copy.flac']),
    mk(48 * MB, ['Docs\\report.pdf', 'Docs\\Old\\report.pdf', 'Desktop\\report.pdf', 'Mail\\attachments\\report.pdf']),
    ...Array.from({ length: 70 }, (_, i) => mk((3 + i) * MB, [`Pictures\\2024\\IMG_${2000 + i}.jpg`, `Pictures\\Export\\IMG_${2000 + i}.jpg`])),
  ];
};
let dupN = 0;
const dupJobs = new Map();
mock('dupes_scan_start', ({ roots }) => {
  const job = `dup${++dupN}`, st = { cancel: false }; dupJobs.set(job, st);
  const ms = (t) => new Promise((r) => setTimeout(r, t));
  (async () => {
    await ms(400);
    for (const [phase, n] of [['walk', 3], ['head', 2], ['hash', 4]]) {
      for (let i = 1; i <= n && !st.cancel; i++) {
        emit('dupes:progress', { job, phase, files: 41000 * i, candidates: 8200, hash_total: 12 * GB, hashed: (12 * GB * i) / n, errors: 4, skipped_cloud: 2, current: `${roots[0]}Users\\mba\\Pictures` });
        await ms(700);
      }
    }
    const groups = st.cancel ? [] : dupGroups(roots);
    emit('dupes:done', { job, cancelled: st.cancel, errors: 4, skipped_cloud: 2, files: 123000, total_groups: groups.length, total_files: groups.reduce((s, g) => s + g.count, 0),
      wasted: groups.reduce((s, g) => s + g.size * (g.count - 1), 0), groups });
    dupJobs.delete(job);
  })();
  return { job };
});
mock('dupes_recycle', ({ groups }) => ({ results: groups.flatMap((g) => {
  // same rule as the backend: never remove a file unless the kept copy is still listed and different from it
  if (!g.keep || g.remove.some((r) => r.path.toLowerCase() === g.keep.path.toLowerCase())) return g.remove.map((r) => ({ path: r.path, ok: false, message: 'the file to keep is also marked for removal' }));
  return g.remove.map((r) => ({ path: r.path, ok: true, message: 'moved to Recycle Bin' }));
}) }));
mock('choose_folder', () => ({ path: 'C:\\Users\\mba\\Videos' }));
mock('open_location', () => null);
mock('recycle_files', ({ paths }) => ({ results: paths.map((path) => {
  const base = path.split('\\').pop().toLowerCase();
  return /^c:\\windows\\/i.test(path) || ['pagefile.sys', 'hiberfil.sys', 'swapfile.sys'].includes(base)
    ? { path, ok: false, message: 'protected system file' } : { path, ok: true, message: 'moved to Recycle Bin' };
}) }));
