// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock, walk } from '../js/mock.js';

const GB = 1024 ** 3, MB = 1024 ** 2;
let paged = 410 * MB, cached = 3.9 * GB;

mock('get_mem_detail', () => {
  paged = walk(paged, 2 * MB, 300 * MB, 600 * MB); cached = walk(cached, 20 * MB, 2 * GB, 6 * GB);
  return { total: 15.8 * GB, used: 9.8 * GB, available: 6.0 * GB, cached, committed: 14.1 * GB, commit_limit: 22.6 * GB,
    paged_pool: paged, nonpaged_pool: 320 * MB, compressed: 1.2 * GB, page_file_total: 6.8 * GB, page_file_used: 1.9 * GB };
});

mock('get_mem_hw', () => ({ type: 'DDR4', speed_mhz: 3200, slots_used: 2, slots_total: 4, form_factor: 'DIMM',
  modules: [
    { slot: 'DIMM_A1', size: 8 * GB, manufacturer: 'Kingston', part_number: 'KF432C16BB/8', speed_mhz: 3200 },
    { slot: 'DIMM_B1', size: 8 * GB, manufacturer: 'Kingston', part_number: 'KF432C16BB/8', speed_mhz: 3200 },
  ] }));
