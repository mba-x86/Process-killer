// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
import { mock } from '../js/mock.js';

mock('get_cpu_detail', () => ({
  model: 'Intel(R) Core(TM) i5-14400F', socket: 'U3E1', architecture: '64-bit', cores_p: 6, cores_e: 4, threads: 16,
  base_mhz: 2500, max_mhz: null, l2_bytes: 10 * 1048576, l3_bytes: 20 * 1048576, virtualization: true, process_count: 271,
  cores: Array.from({ length: 16 }, (_, i) => ({ id: i, kind: i < 12 ? 'P' : 'E' })),
}));
