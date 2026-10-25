// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Live system snapshot (processes / services / startup entries / scheduled tasks) and the low-level actions on them.
use std::sync::Mutex;
use std::time::{Duration, Instant};

use winreg::enums::*;
use winreg::RegKey;

use super::parse::*;
use crate::util::{run_hidden, wide};

#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub start: u64,
    pub name: String,
    pub exe: String,
    pub mem: u64,
}
#[derive(Clone, Debug)]
pub struct Svc {
    pub name: String,
    pub display: String,
    pub state: u32,
    pub start: u32,
    pub path: String,
}
#[derive(Clone, Debug)]
pub struct Startup {
    pub name: String,
    pub cmd: String,
    pub hklm: bool,
    pub wow: bool,
}
impl Startup {
    pub fn key_path(&self) -> &'static str {
        if self.wow {
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run"
        } else {
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"
        }
    }
    pub fn tag(&self) -> &'static str {
        match (self.hklm, self.wow) {
            (true, false) => "HKLM",
            (true, true) => "HKLM32",
            _ => "HKCU",
        }
    }
    pub fn label(&self) -> &'static str {
        if self.hklm {
            "All users"
        } else {
            "Current user"
        }
    }
}
#[derive(Clone, Debug)]
pub struct Task {
    pub name: String,
    pub cmd: String,
    pub status: String,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub procs: Vec<Proc>,
    pub services: Vec<Svc>,
    pub startup: Vec<Startup>,
    pub tasks: Vec<Task>,
}

// ------------------------------------------------------------------ processes

pub fn snap_processes() -> Vec<Proc> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always).with_memory(),
    );
    let me = std::process::id();
    let mut out = Vec::new();
    for (pid, p) in sys.processes() {
        let pid = pid.as_u32();
        if pid == 0 || pid == me {
            continue;
        }
        out.push(Proc {
            pid,
            ppid: p.parent().map(|x| x.as_u32()).unwrap_or(0),
            start: p.start_time(),
            name: p.name().to_string_lossy().into_owned(),
            exe: p.exe().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default(),
            mem: p.memory(),
        });
    }
    out
}

/// pid plus every descendant (by parent links in `all`).
pub fn descendants(all: &[Proc], root: u32) -> Vec<u32> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        let cur = out[i];
        for p in all {
            // a child must have started after its parent: a recycled parent PID must not drag unrelated processes in
            let born_after = all.iter().find(|x| x.pid == cur).map_or(true, |par| p.start >= par.start);
            if p.ppid == cur && !out.contains(&p.pid) && p.pid != 0 && p.pid != p.ppid && born_after {
                out.push(p.pid);
            }
        }
        i += 1;
    }
    out
}

/// Terminate one process and wait (up to 5 s). Ok(true) = ended, Ok(false) = was already gone.
pub fn kill_pid(pid: u32) -> Result<bool, String> {
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER};
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE};
    unsafe {
        let h = match OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) {
            Ok(h) => h,
            Err(_) => {
                let e = GetLastError();
                return if e == ERROR_INVALID_PARAMETER {
                    Ok(false)
                } else if e == ERROR_ACCESS_DENIED {
                    Err("access denied (run as Administrator)".into())
                } else {
                    Err(format!("could not open the process (error {})", e.0))
                };
            }
        };
        let r = TerminateProcess(h, 1);
        let res = match r {
            Ok(()) => {
                WaitForSingleObject(h, 5000);
                Ok(true)
            }
            Err(_) => {
                if GetLastError() == ERROR_ACCESS_DENIED {
                    Err("access denied (run as Administrator)".into())
                } else {
                    Err("could not end the process".into())
                }
            }
        };
        let _ = CloseHandle(h);
        res
    }
}

// ------------------------------------------------------------------ services

pub fn start_type_label(t: u32) -> &'static str {
    match t {
        0 => "Boot",
        1 => "System",
        2 => "Automatic",
        3 => "Manual",
        4 => "Disabled",
        _ => "Unknown",
    }
}
pub fn state_label(s: u32) -> &'static str {
    match s {
        1 => "Stopped",
        2 => "Starting",
        3 => "Stopping",
        4 => "Running",
        5 => "Continuing",
        6 => "Pausing",
        7 => "Paused",
        _ => "Unknown",
    }
}

fn enum_service_states() -> Vec<(String, String, u32)> {
    use windows::Win32::Foundation::ERROR_MORE_DATA;
    use windows::Win32::System::Services::*;
    let mut out = Vec::new();
    unsafe {
        let Ok(scm) = OpenSCManagerW(None, None, SC_MANAGER_ENUMERATE_SERVICE) else { return out };
        let mut resume = 0u32;
        loop {
            let mut buf = vec![0u64; 32 * 1024];
            let bytes = std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len() * 8);
            let (mut needed, mut count) = (0u32, 0u32);
            let r = EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                Some(bytes),
                &mut needed,
                &mut count,
                Some(&mut resume),
                None,
            );
            let p = buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW;
            for i in 0..count as usize {
                let e = &*p.add(i);
                out.push((
                    e.lpServiceName.to_string().unwrap_or_default(),
                    e.lpDisplayName.to_string().unwrap_or_default(),
                    e.ServiceStatusProcess.dwCurrentState.0,
                ));
            }
            match r {
                Ok(()) => break,
                Err(e) if e.code() == ERROR_MORE_DATA.to_hresult() => continue,
                Err(_) => break,
            }
        }
        let _ = CloseServiceHandle(scm);
    }
    out
}

/// Current state of one service (SERVICE_* constant), None if it does not exist.
pub fn service_state(name: &str) -> Option<u32> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Services::*;
    unsafe {
        let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT).ok()?;
        let w = wide(name);
        let h = OpenServiceW(scm, PCWSTR(w.as_ptr()), SERVICE_QUERY_STATUS);
        let res = match h {
            Ok(h) => {
                let mut st = SERVICE_STATUS::default();
                let r = QueryServiceStatus(h, &mut st).ok().map(|_| st.dwCurrentState.0);
                let _ = CloseServiceHandle(h);
                r
            }
            Err(_) => None,
        };
        let _ = CloseServiceHandle(scm);
        res
    }
}

pub fn snap_services() -> Vec<Svc> {
    let root = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services", KEY_READ).ok();
    enum_service_states()
        .into_iter()
        .map(|(name, display, state)| {
            let (mut path, mut start) = (String::new(), 3u32);
            if let Some(k) = root.as_ref().and_then(|r| r.open_subkey_with_flags(&name, KEY_READ).ok()) {
                path = expand_env(&k.get_value::<String, _>("ImagePath").unwrap_or_default());
                start = k.get_value::<u32, _>("Start").unwrap_or(3);
            }
            Svc { name, display, state, start, path }
        })
        .collect()
}

pub fn svc_stop(name: &str, timeout: Duration) -> Result<String, String> {
    invalidate_caches();
    match service_state(name) {
        None | Some(1) => return Ok("already stopped".into()),
        _ => {}
    }
    let (_, out) = run_hidden("sc", &["stop", name]);
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if matches!(service_state(name), None | Some(1)) {
            return Ok("stopped".into());
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    Err(last_line(&out).unwrap_or_else(|| "timed out waiting for it to stop".into()))
}

pub fn svc_delete(name: &str) -> Result<String, String> {
    invalidate_caches();
    let _ = svc_stop(name, Duration::from_secs(8));
    let (ok, out) = run_hidden("sc", &["delete", name]);
    if ok {
        Ok("Service deleted".into())
    } else {
        Err(last_line(&out).unwrap_or_else(|| "failed".into()))
    }
}

pub fn last_line(out: &str) -> Option<String> {
    out.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string())
}

// ------------------------------------------------------------------ startup entries

pub fn list_startup() -> Vec<Startup> {
    let mut out = Vec::new();
    for (hklm, wow) in [(true, false), (true, true), (false, false)] {
        let probe = Startup { name: String::new(), cmd: String::new(), hklm, wow };
        let root = RegKey::predef(if hklm { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER });
        if let Ok(k) = root.open_subkey_with_flags(probe.key_path(), KEY_READ | KEY_WOW64_64KEY) {
            for (name, v) in k.enum_values().flatten() {
                let cmd = match v.vtype {
                    REG_SZ | REG_EXPAND_SZ => k.get_value::<String, _>(&name).unwrap_or_default(),
                    _ => String::new(),
                };
                out.push(Startup { name, cmd, hklm, wow });
            }
        }
    }
    out
}

pub fn startup_remove(e: &Startup) -> Result<String, String> {
    let root = RegKey::predef(if e.hklm { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER });
    match root.open_subkey_with_flags(e.key_path(), KEY_SET_VALUE | KEY_WOW64_64KEY) {
        Ok(k) => match k.delete_value(&e.name) {
            Ok(()) => Ok("removed".into()),
            Err(x) if x.kind() == std::io::ErrorKind::NotFound => Ok("already gone".into()),
            Err(x) if x.kind() == std::io::ErrorKind::PermissionDenied => Err("access denied (run as Administrator)".into()),
            Err(x) => Err(x.to_string()),
        },
        Err(x) if x.kind() == std::io::ErrorKind::NotFound => Ok("already gone".into()),
        Err(x) if x.kind() == std::io::ErrorKind::PermissionDenied => Err("access denied (run as Administrator)".into()),
        Err(x) => Err(x.to_string()),
    }
}

// ------------------------------------------------------------------ scheduled tasks

fn tasks_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("System32").join("Tasks")
}

/// Read the task definitions straight from System32\Tasks (a few ms). None when the folder is unreadable.
fn tasks_from_files() -> Option<Vec<Task>> {
    let base = tasks_dir();
    let top = std::fs::read_dir(&base).ok()?;
    let mut stack: Vec<std::path::PathBuf> = Vec::new();
    for e in top.flatten() {
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir && e.file_name().to_string_lossy().eq_ignore_ascii_case("microsoft") {
            continue;
        }
        stack.push(e.path());
    }
    let mut rows = Vec::new();
    while let Some(p) = stack.pop() {
        let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
        if md.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let (cmd, enabled) = parse_task_xml(&decode_text(&bytes));
        let rel = p.strip_prefix(&base).unwrap_or(&p).to_string_lossy().replace('/', "\\");
        rows.push(Task { name: format!("\\{rel}"), cmd, status: if enabled { "Ready".into() } else { "Disabled".into() } });
    }
    Some(rows)
}

fn tasks_from_schtasks() -> Vec<Task> {
    let (ok, text) = run_hidden("schtasks", &["/query", "/fo", "csv", "/v"]);
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if ok {
        for r in parse_csv(&text) {
            if r.len() < 9 || !r[1].starts_with('\\') || !seen.insert(r[1].clone()) || r[1].to_lowercase().starts_with("\\microsoft\\") {
                continue;
            }
            rows.push(Task { name: r[1].clone(), cmd: r[8].clone(), status: r[3].clone() });
        }
    }
    rows
}

pub fn task_delete(name: &str) -> Result<String, String> {
    invalidate_caches();
    if !name.starts_with('\\') {
        return Err("not a scheduled task".into());
    }
    let (ok, out) = run_hidden("schtasks", &["/delete", "/tn", name, "/f"]);
    if ok {
        Ok("Task deleted".into())
    } else {
        Err(last_line(&out).unwrap_or_else(|| "failed".into()))
    }
}

// ------------------------------------------------------------------ caches

static SVC_CACHE: Mutex<Option<(Instant, Vec<Svc>)>> = Mutex::new(None);
static TASK_CACHE: Mutex<Option<(Instant, Vec<Task>)>> = Mutex::new(None);

pub fn invalidate_caches() {
    *SVC_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *TASK_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn cached_services(fresh: bool) -> Vec<Svc> {
    if !fresh {
        if let Some((t, v)) = SVC_CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if t.elapsed() < Duration::from_secs(20) {
                return v.clone();
            }
        }
    }
    let v = snap_services();
    *SVC_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), v.clone()));
    v
}

fn cached_tasks(fresh: bool) -> Vec<Task> {
    if !fresh {
        if let Some((t, v)) = TASK_CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if t.elapsed() < Duration::from_secs(120) {
                return v.clone();
            }
        }
    }
    let v = tasks_from_files().unwrap_or_else(tasks_from_schtasks);
    *TASK_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), v.clone()));
    v
}

/// Processes are always live. Services (20 s) and tasks (2 min) are cached unless `fresh`.
pub fn snapshot(fresh: bool) -> Snapshot {
    Snapshot { procs: snap_processes(), services: cached_services(fresh), startup: list_startup(), tasks: cached_tasks(fresh) }
}

// ------------------------------------------------------------------ folders

/// Move a folder to the Recycle Bin (IFileOperation, see `crate::recycle`). The caller must have validated the path.
/// An item that cannot be recycled (too large, no bin on the drive) is NOT deleted and the row says so.
pub fn recycle(path: &str) -> Result<String, String> {
    if std::fs::symlink_metadata(path).is_err() {
        return Ok("Already gone".into());
    }
    crate::recycle::recycle(path).map(|_| "Moved to the Recycle Bin".to_string())
}

/// Folder size, capped by entries and time so a huge game folder cannot stall the scan.
pub fn dir_size(path: &str) -> (u64, bool) {
    let start = Instant::now();
    let mut total = 0u64;
    let mut n = 0u32;
    for e in walkdir::WalkDir::new(path).follow_links(false).into_iter().flatten() {
        if e.file_type().is_file() {
            total += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
        n += 1;
        if n % 256 == 0 && start.elapsed() > Duration::from_millis(700) {
            return (total, true);
        }
    }
    (total, false)
}

pub fn env_roots() -> Vec<String> {
    let e = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let mut cands: Vec<String> = Vec::new();
    for k in ["ProgramFiles", "ProgramFiles(x86)", "ProgramData", "LOCALAPPDATA", "APPDATA"] {
        if let Some(v) = e(k) {
            cands.push(v);
        }
    }
    if let Some(v) = e("LOCALAPPDATA") {
        cands.push(format!("{v}\\Programs"));
    }
    let mut seen = std::collections::HashSet::new();
    cands.into_iter().filter(|c| std::path::Path::new(c).is_dir() && seen.insert(c.to_lowercase())).collect()
}
