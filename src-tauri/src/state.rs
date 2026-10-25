// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Process-wide state managed by Tauri (`State<AppState>` in commands).
use std::sync::Mutex;
use std::time::Instant;

use sysinfo::{Networks, System};

use crate::jobs::Jobs;

pub struct AppState {
    pub jobs: Jobs,
    /// Reused between polls: CPU usage needs two samples, so the instance must live across calls.
    pub sys: Mutex<System>,
    pub nets: Mutex<(Networks, Instant)>,
}

impl AppState {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage(); // first sample so the next one yields real percentages
        sys.refresh_memory();
        AppState {
            jobs: Jobs::default(),
            sys: Mutex::new(sys),
            nets: Mutex::new((Networks::new_with_refreshed_list(), Instant::now())),
        }
    }
}
