// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// ?gpu=legacy in the page URL simulates an old GPU that reports no live values.
import { mock, walk } from '../js/mock.js';

const GB = 1024 ** 3;
const legacy = new URLSearchParams(location.search).get('gpu') === 'legacy';
let util = 6, vram = 1.6 * GB, temp = 44;
const eng = { '3D': 6, Copy: 1, 'Video decode': 2, 'Video encode': 0, Compute: 3 };

mock('get_gpu_detail', () => {
  if (legacy) return { live_supported: false, adapters: [
    { id: 'PCI\\VEN_10DE&DEV_0DE1', name: 'NVIDIA GeForce GT 430', vendor: 'NVIDIA', vram_total: 1 * GB, driver: '21.21.13.4201', driver_date: '2015-03-04', directx: '11',
      util_percent: null, vram_used: null, temperature_c: null, clock_mhz: null, power_w: null, engines: null },
  ] };
  util = walk(util, 3, 0, 40); vram = walk(vram, 30e6, 1.2 * GB, 2.4 * GB); temp = walk(temp, 0.5, 38, 62);
  for (const k in eng) eng[k] = walk(eng[k], 2, 0, k === '3D' ? util + 4 : 8);
  return { live_supported: true, adapters: [
    { id: 'PCI\\VEN_10DE&DEV_2D05', name: 'NVIDIA GeForce RTX 5060', vendor: 'NVIDIA', vram_total: 8 * GB, driver: '32.0.15.8157', driver_date: '2026-03-18', directx: '12 (feature level 12_2)',
      util_percent: util, vram_used: vram, temperature_c: temp, clock_mhz: 1800 + util * 8, power_w: 22 + util * 1.4,
      engines: Object.entries(eng).map(([name, percent]) => ({ name, percent })) },
  ] };
});
