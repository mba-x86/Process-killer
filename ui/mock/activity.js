// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock } from '../js/mock.js';

const at = (h, m, s) => { const d = new Date(); d.setHours(h, m, s, 0); return Math.floor(d / 1000); };
let entries = [
  { ts: at(16, 34, 2), level: 'info', message: 'Started in 0.8s · Administrator · version 1.0.3' },
  { ts: at(16, 34, 5), level: 'info', message: 'Slow task: collect took 3.3s' },
];

mock('get_activity', ({ limit = 500 } = {}) => ({ entries: entries.slice(-limit) }));
mock('clear_activity', () => { entries = []; return null; });
mock('open_log_file', () => null);
