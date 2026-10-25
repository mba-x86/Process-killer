// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
#![allow(dead_code)]
//! Windows services page: list (one SCM enumeration + one config query each) and start/stop/restart/enable/manual/disable.
//! Talks to the Service Control Manager directly - no `sc.exe`, no PowerShell, no console flashes.
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA};
use windows::Win32::System::Services::*;

use crate::util::{err, wide, win_dir, CmdResult};

// ---------------------------------------------------------------- RAII handle

struct Sc(SC_HANDLE);
impl Drop for Sc {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseServiceHandle(self.0);
            }
        }
    }
}

fn open_scm(access: u32) -> CmdResult<Sc> {
    match unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), access) } {
        Ok(h) => Ok(Sc(h)),
        Err(e) => err(win_msg(&e)),
    }
}

/// Plain-English message for a Win32 error (access denied gets the "run as Administrator" hint).
fn win_msg(e: &windows::core::Error) -> String {
    let code = (e.code().0 as u32) & 0xFFFF;
    code_msg(code, &e.message())
}

fn code_msg(code: u32, fallback: &str) -> String {
    match code {
        5 => "Access denied - run Task Reaper as Administrator".into(),
        1051 => "Other running services depend on it - stop those first".into(),
        1053 => "The service did not respond in time".into(),
        1058 => "The service is disabled".into(),
        1060 => "Service not found".into(),
        1061 => "The service is busy - try again in a moment".into(),
        1072 => "The service is marked for deletion".into(),
        _ => {
            let f = fallback.trim();
            if f.is_empty() {
                format!("Windows error {code}")
            } else {
                f.to_string()
            }
        }
    }
}

/// u64-backed buffer so struct casts are aligned.
fn abuf(bytes: usize) -> Vec<u64> {
    vec![0u64; bytes.div_ceil(8).max(1)]
}
fn as_bytes(b: &mut [u64]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(b.as_mut_ptr() as *mut u8, b.len() * 8) }
}

unsafe fn pw(p: PWSTR) -> String {
    if p.is_null() {
        String::new()
    } else {
        p.to_string().unwrap_or_default()
    }
}

// ---------------------------------------------------------------- pure helpers

pub fn map_status(state: u32) -> &'static str {
    match state {
        4 => "running",
        1 => "stopped",
        6 | 7 => "paused",
        _ => "pending",
    }
}

pub fn map_start(start_type: u32, delayed: bool) -> &'static str {
    match start_type {
        3 => "manual",
        4 => "disabled",
        2 if delayed => "delayed",
        _ => "automatic",
    }
}

/// Same rule as `_is_system` in app.py: svchost-hosted or anything under System32 (inside the Windows folder).
pub fn is_system_path(path: &str, windir: &str) -> bool {
    let mut p = path.to_lowercase().replace('"', "");
    p = p.trim_start().to_string();
    if let Some(rest) = p.strip_prefix("\\systemroot") {
        p = format!("{windir}{rest}");
    } else if let Some(rest) = p.strip_prefix("%systemroot%") {
        p = format!("{windir}{rest}");
    } else if let Some(rest) = p.strip_prefix("%windir%") {
        p = format!("{windir}{rest}");
    }
    let p = p.replace('/', "\\");
    let w = format!("{windir}\\");
    (p.starts_with(&w) && p.contains("svchost")) || p.starts_with(&format!("{windir}\\system32\\"))
}

/// Services that must never be stopped/disabled: doing so crashes or bricks the session.
const HARD_BLOCKED: &[&str] = &[
    "rpcss", "dcomlaunch", "rpceptmapper", "lsm", "samss", "wininit", "brokerinfrastructure", "systemeventsbroker", "coremessagingregistrar", "profsvc", "gpsvc",
];

pub fn is_hard_blocked(name: &str) -> bool {
    HARD_BLOCKED.contains(&name.to_lowercase().as_str())
}

/// Does the state Windows reports AFTER an action match what the action should have produced?
/// Returns (verified, pending): `pending` = a slow service is still starting/stopping, so the UI should keep watching.
pub fn expected_state(action: Action, status: &str, start: &str) -> (bool, bool) {
    let pending = status == "pending";
    let ok = match action {
        Action::Start | Action::Restart => status == "running",
        Action::Stop => status == "stopped",
        Action::Enable => start == "automatic" || start == "delayed",
        Action::Manual => start == "manual",
        Action::Disable => start == "disabled",
    };
    (ok, pending && !ok)
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Enable,
    Manual,
    Disable,
}
pub fn parse_action(a: &str) -> Option<Action> {
    Some(match a {
        "start" => Action::Start,
        "stop" => Action::Stop,
        "restart" => Action::Restart,
        "enable" => Action::Enable,
        "manual" => Action::Manual,
        "disable" => Action::Disable,
        _ => return None,
    })
}

// ---------------------------------------------------------------- listing

#[derive(Serialize)]
pub struct ServiceRow {
    name: String,
    display: String,
    status: &'static str,
    start: &'static str,
    path: String,
    system: bool,
}

fn query_config(scm: &Sc, name: &[u16]) -> Option<(String, u32, bool)> {
    unsafe {
        let h = Sc(OpenServiceW(scm.0, PCWSTR(name.as_ptr()), SERVICE_QUERY_CONFIG).ok()?);
        let mut buf = abuf(4096);
        let mut need = 0u32;
        let mut res = QueryServiceConfigW(h.0, Some(buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW), 4096, &mut need);
        if let Err(e) = &res {
            if (e.code().0 as u32 & 0xFFFF) == ERROR_INSUFFICIENT_BUFFER.0 && need > 0 {
                buf = abuf(need as usize);
                res = QueryServiceConfigW(h.0, Some(buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW), need, &mut need);
            }
        }
        res.ok()?;
        let cfg = &*(buf.as_ptr() as *const QUERY_SERVICE_CONFIGW);
        let path = pw(cfg.lpBinaryPathName);
        let st = cfg.dwStartType.0;
        let mut delayed = false;
        if st == 2 {
            let mut b2 = abuf(16);
            let mut n2 = 0u32;
            if QueryServiceConfig2W(h.0, SERVICE_CONFIG_DELAYED_AUTO_START_INFO, Some(as_bytes(&mut b2)), &mut n2).is_ok() {
                let info = &*(b2.as_ptr() as *const SERVICE_DELAYED_AUTO_START_INFO);
                delayed = info.fDelayedAutostart.as_bool();
            }
        }
        Some((path, st, delayed))
    }
}

pub fn snapshot() -> CmdResult<Vec<ServiceRow>> {
    let scm = open_scm(SC_MANAGER_ENUMERATE_SERVICE)?;
    let windir = win_dir();
    let mut rows: Vec<ServiceRow> = Vec::with_capacity(320);
    let mut buf = abuf(128 * 1024);
    let mut resume = 0u32;
    loop {
        let mut need = 0u32;
        let mut returned = 0u32;
        let r = unsafe {
            EnumServicesStatusExW(
                scm.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                Some(as_bytes(&mut buf)),
                &mut need,
                &mut returned,
                Some(&mut resume),
                PCWSTR::null(),
            )
        };
        let more = match &r {
            Ok(()) => false,
            Err(e) if (e.code().0 as u32 & 0xFFFF) == ERROR_MORE_DATA.0 => true,
            Err(e) => return err(win_msg(e)),
        };
        let arr = buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW;
        for i in 0..returned as usize {
            let e = unsafe { &*arr.add(i) };
            let name = unsafe { pw(e.lpServiceName) };
            let display = unsafe { pw(e.lpDisplayName) };
            let wname = wide(&name);
            let (path, st, delayed) = query_config(&scm, &wname).unwrap_or((String::new(), 3, false));
            rows.push(ServiceRow {
                system: is_system_path(&path, &windir),
                status: map_status(e.ServiceStatusProcess.dwCurrentState.0),
                start: map_start(st, delayed),
                display: if display.is_empty() { name.clone() } else { display },
                name,
                path,
            });
        }
        if !more {
            break;
        }
        if buf.len() * 8 < need as usize {
            buf = abuf(need as usize);
        }
    }
    rows.sort_by(|a, b| a.display.to_lowercase().cmp(&b.display.to_lowercase()));
    Ok(rows)
}

#[tauri::command]
pub async fn list_services() -> CmdResult<Value> {
    let rows = tauri::async_runtime::spawn_blocking(snapshot).await.map_err(|e| crate::util::CmdError(e.to_string()))??;
    Ok(json!({ "services": rows }))
}

// ---------------------------------------------------------------- actions

fn current_state(h: &Sc) -> Option<u32> {
    let mut st = SERVICE_STATUS::default();
    unsafe { QueryServiceStatus(h.0, &mut st).ok()? };
    Some(st.dwCurrentState.0)
}

fn wait_for(h: &Sc, want: u32, secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        match current_state(h) {
            Some(s) if s == want => return true,
            None => return false,
            _ => {}
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn open_svc(scm: &Sc, name: &str, access: u32) -> Result<Sc, String> {
    let w = wide(name);
    match unsafe { OpenServiceW(scm.0, PCWSTR(w.as_ptr()), access) } {
        Ok(h) => Ok(Sc(h)),
        Err(e) => Err(win_msg(&e)),
    }
}

fn do_stop(h: &Sc, timeout: u64) -> Result<&'static str, String> {
    if current_state(h) == Some(1) {
        return Ok("already stopped");
    }
    let mut st = SERVICE_STATUS::default();
    if let Err(e) = unsafe { ControlService(h.0, SERVICE_CONTROL_STOP, &mut st) } {
        let code = e.code().0 as u32 & 0xFFFF;
        if code == 1062 {
            return Ok("already stopped"); // ERROR_SERVICE_NOT_ACTIVE
        }
        return Err(win_msg(&e));
    }
    if wait_for(h, 1, timeout) {
        Ok("stopped")
    } else {
        Err("Timed out waiting for the service to stop".into())
    }
}

fn do_start(h: &Sc) -> Result<&'static str, String> {
    if let Err(e) = unsafe { StartServiceW(h.0, None) } {
        let code = e.code().0 as u32 & 0xFFFF;
        if code == 1056 {
            return Ok("already running"); // ERROR_SERVICE_ALREADY_RUNNING
        }
        return Err(win_msg(&e));
    }
    if wait_for(h, 4, 10) {
        Ok("started")
    } else if current_state(h) == Some(1) {
        Err("The service stopped again right after starting".into())
    } else {
        Ok("start requested")
    }
}

fn do_config(h: &Sc, start: SERVICE_START_TYPE, clear_delayed: bool) -> Result<(), String> {
    unsafe {
        ChangeServiceConfigW(
            h.0,
            ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
            start,
            SERVICE_ERROR(SERVICE_NO_CHANGE),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
        .map_err(|e| win_msg(&e))?;
        if clear_delayed {
            let mut info = SERVICE_DELAYED_AUTO_START_INFO { fDelayedAutostart: false.into() };
            let _ = ChangeServiceConfig2W(h.0, SERVICE_CONFIG_DELAYED_AUTO_START_INFO, Some(&mut info as *mut _ as *mut _));
        }
    }
    Ok(())
}

const ACCESS_ALL: u32 = SERVICE_START | SERVICE_STOP | SERVICE_QUERY_STATUS | SERVICE_CHANGE_CONFIG | SERVICE_QUERY_CONFIG;

fn apply(scm: &Sc, name: &str, action: Action) -> Result<String, String> {
    if matches!(action, Action::Stop | Action::Restart | Action::Disable) && is_hard_blocked(name) {
        return Err("This service is critical for Windows and cannot be stopped or disabled".into());
    }
    let h = open_svc(scm, name, ACCESS_ALL)?;
    match action {
        Action::Start => do_start(&h).map(|s| format!("Service {s}")),
        Action::Stop => do_stop(&h, 20).map(|s| format!("Service {s}")),
        Action::Restart => {
            do_stop(&h, 20)?;
            do_start(&h).map(|s| format!("Service restarted ({s})"))
        }
        Action::Enable => {
            do_config(&h, SERVICE_AUTO_START, true)?;
            match do_start(&h) {
                Ok(_) => Ok("Enabled (automatic) and started".into()),
                Err(e) => Ok(format!("Enabled (automatic), but could not start it: {e}")),
            }
        }
        Action::Manual => do_config(&h, SERVICE_DEMAND_START, true).map(|_| "Startup type: manual".into()),
        Action::Disable => {
            let stopped = do_stop(&h, 20);
            do_config(&h, SERVICE_DISABLED, true)?;
            match stopped {
                Ok(_) => Ok("Stopped and disabled".into()),
                Err(e) => Ok(format!("Disabled ({e})")),
            }
        }
    }
}

/// Current (status, startup type) of one service, straight from the Service Control Manager.
fn read_back(scm: &Sc, name: &str) -> (&'static str, &'static str) {
    let w = wide(name);
    let start = query_config(scm, &w).map_or("unknown", |(_, st, delayed)| map_start(st, delayed));
    let status = open_svc(scm, name, SERVICE_QUERY_STATUS).ok().and_then(|h| current_state(&h)).map_or("unknown", map_status);
    (status, start)
}

/// Poll a service that was still starting/stopping (UI calls this until `pending` is false).
#[tauri::command(rename_all = "snake_case")]
pub async fn service_status(names: Vec<String>) -> CmdResult<Value> {
    let rows = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Vec<Value>> {
        let scm = open_scm(SC_MANAGER_CONNECT)?;
        Ok(names.iter().map(|n| { let (status, start) = read_back(&scm, n); json!({ "name": n, "status": status, "start": start }) }).collect())
    })
    .await
    .map_err(|e| crate::util::CmdError(e.to_string()))??;
    Ok(json!({ "services": rows }))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_service_state(app: tauri::AppHandle, names: Vec<String>, action: String) -> CmdResult<Value> {
    let act = match parse_action(&action) {
        Some(a) => a,
        None => return err(format!("Unknown action '{action}'")),
    };
    let app2 = app.clone();
    let results = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Vec<Value>> {
        let scm = open_scm(SC_MANAGER_CONNECT)?;
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let r = apply(&scm, &name, act);
            let (ok, message) = match r {
                Ok(m) => (true, m),
                Err(m) => (false, m),
            };
            // never trust the call alone: read the service back from the SCM and compare with what the action should give
            let (status, start) = read_back(&scm, &name);
            let (verified, pending) = if ok { expected_state(act, status, start) } else { (false, false) };
            let message = if ok && !verified && !pending { format!("{message} - but Windows still reports it as {status} / {start}") } else { message };
            crate::activity::log(
                &app2,
                if ok && (verified || pending) { "ok" } else { "fail" },
                format!("Service {name}: {action} - {message} [now {status}, startup {start}{}]", if verified { ", verified" } else { "" }),
            );
            out.push(json!({ "name": name, "ok": ok && (verified || pending), "message": message, "verified": verified, "pending": pending, "status": status, "start": start }));
        }
        Ok(out)
    })
    .await
    .map_err(|e| crate::util::CmdError(e.to_string()))??;
    Ok(json!({ "results": results }))
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_rule() {
        let w = "c:\\windows";
        assert!(is_system_path("C:\\WINDOWS\\system32\\svchost.exe -k netsvcs -p", w));
        assert!(is_system_path("\"C:\\Windows\\System32\\spoolsv.exe\"", w));
        assert!(is_system_path("\\SystemRoot\\System32\\smss.exe", w));
        assert!(is_system_path("C:\\Windows\\SysWOW64\\svchost.exe -k x", w));
        assert!(!is_system_path("\"C:\\Program Files\\Brave\\elevation_service.exe\"", w));
        assert!(!is_system_path("C:\\ProgramData\\Microsoft\\Windows Defender\\Platform\\1\\MsMpEng.exe", w));
        assert!(!is_system_path("", w));
    }

    #[test]
    fn status_and_start_maps() {
        assert_eq!(map_status(4), "running");
        assert_eq!(map_status(1), "stopped");
        assert_eq!(map_status(7), "paused");
        assert_eq!(map_status(2), "pending");
        assert_eq!(map_start(2, true), "delayed");
        assert_eq!(map_start(2, false), "automatic");
        assert_eq!(map_start(4, false), "disabled");
        assert_eq!(map_start(3, true), "manual");
    }

    #[test]
    fn actions_and_blocklist() {
        assert_eq!(parse_action("disable"), Some(Action::Disable));
        assert_eq!(parse_action("rm"), None);
        assert!(is_hard_blocked("RpcSs"));
        assert!(!is_hard_blocked("Spooler"));
        // refused before any SCM call is made (no handle needed)
        if let Ok(scm) = open_scm(SC_MANAGER_CONNECT) {
            assert!(apply(&scm, "RpcSs", Action::Stop).is_err());
            assert!(apply(&scm, "DcomLaunch", Action::Disable).is_err());
        }
    }

    #[test]
    fn expected_vs_actual() {
        assert_eq!(expected_state(Action::Stop, "stopped", "manual"), (true, false));
        assert_eq!(expected_state(Action::Stop, "running", "manual"), (false, false));
        assert_eq!(expected_state(Action::Stop, "pending", "manual"), (false, true));
        assert_eq!(expected_state(Action::Start, "running", "disabled"), (true, false));
        assert_eq!(expected_state(Action::Restart, "stopped", "manual"), (false, false));
        assert_eq!(expected_state(Action::Disable, "stopped", "disabled"), (true, false));
        assert_eq!(expected_state(Action::Disable, "running", "manual"), (false, false));
        assert_eq!(expected_state(Action::Enable, "running", "delayed"), (true, false));
        assert_eq!(expected_state(Action::Enable, "stopped", "manual"), (false, false));
        assert_eq!(expected_state(Action::Manual, "running", "manual"), (true, false));
    }

    #[test]
    fn error_messages() {
        assert!(code_msg(5, "x").contains("Administrator"));
        assert_eq!(code_msg(1060, ""), "Service not found");
    }

    /// Read-only: lists the real services.
    #[test]
    #[ignore]
    fn smoke_list_services() {
        let t = Instant::now();
        let rows = snapshot().unwrap();
        println!("{} services in {:?}", rows.len(), t.elapsed());
        for r in rows.iter().take(5) {
            println!("{} | {} | {} | {} | {} | {}", r.name, r.display, r.status, r.start, r.system, r.path);
        }
        assert!(rows.len() > 50);
    }
}
