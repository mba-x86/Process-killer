// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock, emit, wait } from '../js/mock.js';

const MB = 1048576, GB = 1073741824;
const CATS = [
  { id: 'user_temp', title: 'User temp files', desc: 'Leftovers from apps and installers', default_on: true, warn: false },
  { id: 'win_temp', title: 'Windows temp files', desc: 'System temp folder (needs Administrator)', default_on: true, warn: false },
  { id: 'crash', title: 'Crash dumps & error reports', desc: 'Memory dumps and Windows error reports', default_on: true, warn: false },
  { id: 'recycle', title: 'Recycle Bin', desc: 'Permanently empties the bin', default_on: true, warn: true },
  { id: 'browser', title: 'Browser caches', desc: 'Chrome, Edge, Brave, Firefox (you stay logged in)', default_on: false, warn: false },
];
const SIZES = { user_temp: 842 * MB, win_temp: 1.3 * GB, crash: 214 * MB, recycle: 3.2 * GB, browser: 1.1 * GB };

mock('get_junk_categories', () => ({ categories: CATS }));
mock('scan_junk', async ({ id }) => { await wait(500 + Math.random() * 500); return { bytes: Math.round(SIZES[id] || 0), files: Math.round((SIZES[id] || 0) / (80 * 1024)) }; });
mock('boost_ram', async () => { await wait(1600); return { freed: 612 * MB, trimmed: 41, purged: false, percent: 54 }; });

let n = 0;
mock('clean_start', ({ ids }) => {
  const job = `clean${++n}`;
  (async () => {
    await wait(100);
    const results = [];
    for (const id of ids) {
      const total = Math.round(SIZES[id] || 0);
      for (let i = 1; i <= 8; i++) { await wait(90); emit('clean:progress', { job, id, done_bytes: (total * i) / 8, total_bytes: total }); }
      results.push({ id, freed: Math.round(total * 0.97), failed: id === 'user_temp' ? 3 : 0 });
    }
    emit('clean:done', { job, results, total_freed: results.reduce((s, r) => s + r.freed, 0) });
  })();
  return { job };
});
