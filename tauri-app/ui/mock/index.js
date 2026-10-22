// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Loads every page's mock module. Missing files are fine (allSettled) so pages can be added one by one.
await Promise.allSettled([
  import('./shell.js'), import('./dashboard.js'), import('./processor.js'), import('./memory.js'), import('./network.js'),
  import('./graphics.js'), import('./applications.js'), import('./services.js'), import('./processes.js'),
  import('./startup.js'), import('./optimizer.js'), import('./storage.js'), import('./activity.js'), import('./about.js'),
]);
