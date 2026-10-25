// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Long-running work (scan, clean, uninstall, download) runs on a worker thread and reports through events:
//!   start command  -> returns {"job": "<id>"} immediately
//!   events         -> "<domain>:progress" {job, ...}  and finally "<domain>:done" {job, ...}
//!   cancel command -> sets the job's flag; the worker polls `flag.load(Ordering::Relaxed)`.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Jobs {
    /// Register a new job: returns (id, cancel flag).
    pub fn start(&self) -> (String, Arc<AtomicBool>) {
        let id = format!("j{}", self.next.fetch_add(1, Ordering::Relaxed) + 1);
        let flag = Arc::new(AtomicBool::new(false));
        self.flags.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), flag.clone());
        (id, flag)
    }
    pub fn cancel(&self, id: &str) {
        if let Some(f) = self.flags.lock().unwrap_or_else(|e| e.into_inner()).get(id) {
            f.store(true, Ordering::Relaxed);
        }
    }
    pub fn finish(&self, id: &str) {
        self.flags.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
    }
}
