// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock, emit, wait } from '../js/mock.js';

const AVAILABLE = ['1', 'go'].includes(new URLSearchParams(location.search).get('update'));
const OFFLINE = new URLSearchParams(location.search).get('update') === 'offline';
let settings = { auto_check: true };

const NOTES_CURRENT = [
  'New: Remove apps whose uninstaller is missing (stale entries Windows cannot uninstall), with a registry backup',
  'New: Guarded removal of Microsoft Edge (keeps WebView2, requires another browser, optional reinstall block)',
  'Fixed: Clearer errors from background tasks, thread-safe logging, app names with hidden characters',
].join('\n');

mock('get_about', () => ({ version: '1.0.3', copyright: '© 2026 mba-x86', license: 'MIT License',
  repo_url: 'https://github.com/mba-x86/task-reaper', release_notes_current: NOTES_CURRENT }));

mock('check_update', async () => {
  await wait(600);
  if (OFFLINE) throw new Error('Could not reach the update server. Check your internet connection.');
  if (!AVAILABLE) return { available: false, current: '1.0.3', version: '1.0.3', notes: NOTES_CURRENT, size: 0 };
  return { available: true, current: '1.0.3', version: '2.0.1', size: 15400000, published: Math.floor(Date.now() / 1000) - 86400,
    notes: ['New: Tauri rebuild - starts instantly and uses a fraction of the memory',
      'New: Redesigned dashboard, storage analyzer and activity log',
      'Improved: Faster application details and hardware report',
      'Fixed: Services list could miss entries on non-English Windows'].join('\n') };
});

mock('start_update', () => {
  const job = 'upd-1', total = 15400000;
  (async () => {
    for (let done = 0; done < total; done += 1100000) { await wait(450); emit('update:progress', { job, done: Math.min(done, total), total }); }
    emit('update:progress', { job, done: total, total });
    await wait(500);
    emit('update:done', { job, ok: true, message: 'ready' });
  })();
  return { job };
});

mock('get_settings', () => ({ ...settings }));
mock('set_settings', (a) => { settings = { ...settings, ...a }; return null; });
mock('open_url', () => null);

// Screenshot helper: ?update=go clicks "Update now" + the confirm button by itself (download banner demo).
if (new URLSearchParams(location.search).get('update') === 'go') {
  const click = (t) => [...document.querySelectorAll('button')].filter((b) => b.textContent.trim() === t).pop()?.click();
  setTimeout(() => click('Update now'), 4600);
  setTimeout(() => click('Update now'), 4900);
}
