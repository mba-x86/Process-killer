// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// 16x16 line icons taken from the design. icon('bolt', 15) -> <svg>.
import { h } from './dom.js';

const I = {
  dashboard: '<rect x="2.5" y="2.5" width="4.5" height="4.5" rx="1"/><rect x="9" y="2.5" width="4.5" height="4.5" rx="1"/><rect x="2.5" y="9" width="4.5" height="4.5" rx="1"/><rect x="9" y="9" width="4.5" height="4.5" rx="1"/>',
  performance: '<path d="M2 11.5a6 6 0 1 1 12 0"/><path d="M8 11.5 10.8 7"/><circle cx="8" cy="11.5" r=".9" fill="currentColor"/>',
  processor:'<rect x="4" y="4" width="8" height="8" rx="1.5"/><path d="M6.5 1.5v2M9.5 1.5v2M6.5 12.5v2M9.5 12.5v2M1.5 6.5h2M1.5 9.5h2M12.5 6.5h2M12.5 9.5h2"/>',
  memory: '<rect x="1.5" y="4.5" width="13" height="6" rx="1"/><path d="M4.5 10.5v2M8 10.5v2M11.5 10.5v2"/>',
  network: '<path d="M5 13V3M2.5 5.5 5 3l2.5 2.5M11 3v10M8.5 10.5 11 13l2.5-2.5"/>',
  graphics: '<rect x="1.5" y="4" width="13" height="8" rx="1.5"/><circle cx="10" cy="8" r="2"/><path d="M4 7h2.5M4 9h2.5"/>',
  applications: '<path d="M8 1.8 13.5 4.8v6.4L8 14.2 2.5 11.2V4.8z"/><path d="M2.5 4.8 8 7.8l5.5-3M8 7.8v6.4"/>',
  services: '<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/>',
  processes: '<path d="M1.5 8h3l2-5 3 10 2-5h3"/>',
  startup: '<path d="M8 1.8v5.5"/><path d="M4.6 4a5 5 0 1 0 6.8 0"/>',
  optimizer: '<path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z"/>',
  bolt: '<path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z"/>',
  storage: '<rect x="1.8" y="9" width="12.4" height="4.5" rx="1"/><path d="M3 9 4.8 3h6.4L13 9"/>',
  activity: '<path d="M5.5 4h8M5.5 8h8M5.5 12h8M2.5 4h.01M2.5 8h.01M2.5 12h.01"/>',
  about: '<circle cx="8" cy="8" r="6.2"/><path d="M8 7.2v4M8 5h.01"/>',
  trash: '<path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4"/>',
  bars: '<path d="M2.5 13.5v-4M6.2 13.5v-8M9.8 13.5V8M13.5 13.5v-11"/>',
  copy: '<rect x="5" y="5" width="8.5" height="8.5" rx="1.2"/><path d="M3 10.5V3.5a1 1 0 0 1 1-1h6.5"/>',
  refresh: '<path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 2.5v3h-3"/>',
  search: '<circle cx="7" cy="7" r="4.5"/><path d="m10.5 10.5 3.5 3.5"/>',
  back: '<path d="M10 3 5 8l5 5"/>',
  down: '<path d="M8 2.5v11M3.5 9 8 13.5 12.5 9"/>',
  up: '<path d="M8 13.5v-11M3.5 7 8 2.5 12.5 7"/>',
  x: '<path d="m3.5 3.5 9 9M12.5 3.5l-9 9"/>',
  check: '<path d="m3 8.5 3.2 3.2L13 4.8"/>',
  warn: '<path d="M8 2 14.5 13.5h-13z"/><path d="M8 6.5v3.2M8 11.6h.01"/>',
  folder: '<path d="M1.8 4.2c0-.7.5-1.2 1.2-1.2h3l1.4 1.6H13c.7 0 1.2.5 1.2 1.2v6.2c0 .7-.5 1.2-1.2 1.2H3c-.7 0-1.2-.5-1.2-1.2z"/>',
  play: '<path d="M4.5 3v10l8-5z"/>',
  stop: '<rect x="3.5" y="3.5" width="9" height="9" rx="1.2"/>',
  shield: '<path d="M8 1.8 13 3.6v4.2c0 3-2.1 5.2-5 6.4-2.9-1.2-5-3.4-5-6.4V3.6z"/><path d="m5.8 8 1.6 1.6 2.9-3"/>',
  chevron: '<path d="m6 3.5 4.5 4.5L6 12.5"/>',
  sort: '<path d="M8 3v10M4.5 6.5 8 3l3.5 3.5"/>',
  external: '<path d="M9 2.5h4.5V7M13.5 2.5 7.5 8.5M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3"/>',
};

export function icon(name, size = 16, attrs = {}) {
  const svg = h('svg', { width: size, height: size, viewBox: '0 0 16 16', fill: 'none', stroke: 'currentColor', 'stroke-width': 1.6,
    'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true', ...attrs });
  svg.innerHTML = I[name] || '';
  return svg;
}

/** Task Reaper mark: dashed ring + scythe. This is the ONE source of the logo: tools/make_icons.py reads the markup below
 *  (the line starting `svg.innerHTML =`) to render icon.ico, the PNG icons and the installer images. */
export function logo(size = 30) {
  const svg = h('svg', { width: size, height: size, viewBox: '0 0 28 28', fill: 'none', 'aria-hidden': 'true' });
  svg.innerHTML = '<circle cx="14" cy="14" r="11" stroke="#FF6B3D" stroke-width="2.2" stroke-dasharray="14 3.3"/><path d="M8.6 21.4L17.4 8" stroke="#E8EBEF" stroke-width="2.2" stroke-linecap="round"/><path d="M17.8 7.3C12.6 3.4 6.2 6.2 5 14C8.2 10.6 12.9 9.9 17.8 7.3Z" fill="#E8EBEF" stroke="#E8EBEF" stroke-width=".6" stroke-linejoin="round"/>';
  return svg;
}
