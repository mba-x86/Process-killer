// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Sidebar structure from the design (Processor / Memory / Network / Graphics are tabs inside 'performance'). Pages are loaded lazily (first visit only) to keep startup instant.
export const GROUPS = ['OVERVIEW', 'MANAGE', 'CLEAN UP', 'APP'];

export const ROUTES = [
  { id: 'dashboard',    label: 'Dashboard',        group: 'OVERVIEW',    icon: 'dashboard',    load: () => import('../pages/dashboard.js') },
  { id: 'performance',  label: 'Performance',      group: 'OVERVIEW',    icon: 'performance',  load: () => import('../pages/performance.js') },
  { id: 'applications', label: 'Applications',     group: 'MANAGE',      icon: 'applications', load: () => import('../pages/applications.js') },
  { id: 'services',     label: 'Services',         group: 'MANAGE',      icon: 'services',     load: () => import('../pages/services.js') },
  { id: 'processes',    label: 'Processes',        group: 'MANAGE',      icon: 'processes',    load: () => import('../pages/processes.js') },
  { id: 'startup',      label: 'Startup & tasks',  group: 'MANAGE',      icon: 'startup',      load: () => import('../pages/startup.js') },
  { id: 'optimizer',    label: 'Optimizer',        group: 'CLEAN UP',    icon: 'optimizer',    load: () => import('../pages/optimizer.js') },
  { id: 'storage',      label: 'Storage analyzer', group: 'CLEAN UP',    icon: 'storage',      load: () => import('../pages/storage.js') },
  { id: 'activity',     label: 'Activity',         group: 'APP',         icon: 'activity',     load: () => import('../pages/activity.js') },
  { id: 'about',        label: 'About & updates',  group: 'APP',         icon: 'about',        load: () => import('../pages/about.js') },
];
