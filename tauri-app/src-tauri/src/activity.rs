// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Shared activity log (the UI's Activity page). Every module calls `activity::log(&app, "ok"|"fail"|"info", "text")`
//! for notable actions. Entries go to `%APPDATA%\ProcessKiller\activity.log` (same line format as the 1.x app:
//! `YYYY-MM-DD HH:MM:SS [level] message`) and are pushed to the UI as the `activity:entry` event.
use std::io::Write;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::util;

#[derive(Serialize, Clone)]
pub struct Entry {
    pub ts: u64,
    pub level: String,
    pub message: String,
}

fn local_stamp() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

pub fn log_path() -> std::path::PathBuf {
    util::data_dir().join("activity.log")
}

/// Append to the log file and notify the UI. Never panics, never blocks for long.
pub fn log(app: &AppHandle, level: &str, message: impl Into<String>) {
    let message = message.into().replace(['\r', '\n'], " ");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{} [{}] {}", local_stamp(), level, message);
    }
    let _ = app.emit("activity:entry", Entry { ts: util::unix_now(), level: level.to_string(), message });
}
