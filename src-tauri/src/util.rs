// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Shared helpers for every command module.
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Error type for commands: serialises as a plain human-readable string, which the UI shows as `Error(message)`.
#[derive(Debug)]
pub struct CmdError(pub String);
impl serde::Serialize for CmdError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<E: std::fmt::Display> From<E> for CmdError {
    fn from(e: E) -> Self {
        CmdError(e.to_string())
    }
}
pub type CmdResult<T> = Result<T, CmdError>;
pub fn err<T>(msg: impl Into<String>) -> CmdResult<T> {
    Err(CmdError(msg.into()))
}

/// Run a program without flashing a console window. Returns (success, stdout+stderr trimmed).
pub fn run_hidden(program: &str, args: &[&str]) -> (bool, String) {
    match Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
    {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), s.trim().to_string())
        }
        Err(e) => (false, e.to_string()),
    }
}

/// PowerShell one-liner (UTF-8 output, no profile). Slow (~0.4 s start-up): prefer direct APIs; use for what has none.
pub fn powershell(script: &str) -> (bool, String) {
    let wrapped = format!("[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ErrorActionPreference='SilentlyContinue'; {script}");
    run_hidden("powershell", &["-NoProfile", "-NonInteractive", "-Command", &wrapped])
}

pub fn is_admin() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `%SystemRoot%` lower-cased without a trailing slash, e.g. `c:\windows`.
pub fn win_dir() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()).to_lowercase().trim_end_matches('\\').to_string()
}

/// Per-user data folder shared with the 1.x app: `%APPDATA%\TaskReaper` (settings.json, activity.log, backups\, specs cache).
pub fn data_dir() -> std::path::PathBuf {
    let p = std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("TaskReaper");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// One running copy at a time. Waits up to ~6 s so a relaunch (update / elevation) can start while the old copy exits.
/// Uses the same mutex name as the 1.x app, so 1.x and 2.x never run together.
pub fn acquire_single_instance() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    let name = wide("TaskReaperAppMutex");
    for _ in 0..12 {
        unsafe {
            if let Ok(h) = CreateMutexW(None, false, PCWSTR(name.as_ptr())) {
                if GetLastError() != ERROR_ALREADY_EXISTS {
                    return true; // keep the handle open for the life of the process
                }
                let _ = CloseHandle(h);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    false
}

/// Reveal a file in Explorer (select it). Safe: only passes the path as one argument.
pub fn reveal_in_explorer(path: &str) {
    let _ = Command::new("explorer").arg(format!("/select,{path}")).spawn();
}
