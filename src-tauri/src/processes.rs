// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
#![allow(dead_code)]
//! Processes page: list (polled every 3 s), end process / process tree, reveal in Explorer.
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_TERMINATE};

use crate::protect;
use crate::util::{err, CmdError, CmdResult};

const PROCESS_SYNCHRONIZE: u32 = 0x0010_0000;

/// One System held for the life of the app: CPU % needs two samples, and re-creating it every poll would be slow.
fn sys() -> &'static Mutex<System> {
    static S: OnceLock<Mutex<System>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(System::new()))
}

fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing().with_memory().with_cpu().with_exe(UpdateKind::OnlyIfNotSet)
}

fn cpu_count() -> f32 {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32
}

/// Process info reduced to what the page and the kill logic need.
#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub start: u64,
    pub name: String,
    pub exe: String,
}

fn proc_name(p: &sysinfo::Process) -> String {
    p.name().to_string_lossy().into_owned()
}
fn proc_exe(p: &sysinfo::Process) -> String {
    p.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
}

#[tauri::command]
pub async fn list_processes() -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut s = sys().lock().map_err(|_| CmdError("process list is busy".into()))?;
        let first = s.processes().is_empty();
        s.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        if first {
            // CPU usage is a delta: take a second sample so the very first list is not all zeros.
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            s.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        }
        let n = cpu_count();
        let rows: Vec<Value> = s
            .processes()
            .values()
            .map(|p| {
                let name = proc_name(p);
                let exe = proc_exe(p);
                let cpu = ((p.cpu_usage() / n).clamp(0.0, 100.0) * 10.0).round() / 10.0;
                json!({
                    "pid": p.pid().as_u32(),
                    "protected": protect::is_protected_process(&name, &exe),
                    "name": name,
                    "exe": exe,
                    "mem": p.memory(),
                    "cpu": cpu,
                })
            })
            .collect();
        Ok(json!({ "processes": rows }))
    })
    .await
    .map_err(|e| CmdError(e.to_string()))?
}

// ---------------------------------------------------------------- kill logic

/// All descendants of `root` (children first / deepest first), using the parent map. A child only counts when it
/// started after its parent (guards against a recycled parent PID).
pub fn descendants(all: &[Proc], root: u32) -> Vec<u32> {
    let by_pid: HashMap<u32, &Proc> = all.iter().map(|p| (p.pid, p)).collect();
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in all {
        if p.pid == p.ppid {
            continue;
        }
        if let Some(par) = by_pid.get(&p.ppid) {
            if p.start >= par.start {
                kids.entry(p.ppid).or_default().push(p.pid);
            }
        }
    }
    let mut order = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    seen.insert(root);
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if let Some(ks) = kids.get(&cur) {
            for &k in ks {
                if seen.insert(k) {
                    order.push(k);
                    stack.push(k);
                }
            }
        }
    }
    order.reverse(); // discovered parents-before-children; reversed = deepest first
    order
}

/// Why this PID must not be ended (None = allowed).
pub fn refuse_reason(p: &Proc, own: u32, parent: u32) -> Option<&'static str> {
    if p.pid == 0 || p.pid == 4 || protect::is_protected_process(&p.name, &p.exe) {
        Some("protected system process")
    } else if p.pid == own {
        Some("that is Task Reaper itself")
    } else if p.pid == parent {
        Some("that is the program that launched Task Reaper")
    } else {
        None
    }
}

fn terminate(pid: u32) -> Result<(), String> {
    unsafe {
        let h = OpenProcess(
            windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS(PROCESS_TERMINATE.0 | PROCESS_SYNCHRONIZE),
            false,
            pid,
        )
        .map_err(|e| {
            let c = e.code().0 as u32 & 0xFFFF;
            match c {
                5 => "access denied (run as Administrator)".to_string(),
                87 => "already gone".to_string(),
                _ => e.message(),
            }
        })?;
        let r = TerminateProcess(h, 1);
        if r.is_ok() {
            WaitForSingleObject(h, 3000);
        }
        let _ = CloseHandle(h);
        r.map_err(|e| {
            if e.code().0 as u32 & 0xFFFF == 5 {
                "access denied (run as Administrator)".to_string()
            } else {
                e.message()
            }
        })
    }
}

fn fresh_snapshot() -> CmdResult<Vec<Proc>> {
    let mut s = sys().lock().map_err(|_| CmdError("process list is busy".into()))?;
    s.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet));
    Ok(s
        .processes()
        .values()
        .map(|p| Proc {
            pid: p.pid().as_u32(),
            ppid: p.parent().map(|x: Pid| x.as_u32()).unwrap_or(0),
            start: p.start_time(),
            name: proc_name(p),
            exe: proc_exe(p),
        })
        .collect())
}

fn parent_pid(all: &[Proc], own: u32) -> u32 {
    all.iter().find(|p| p.pid == own).map(|p| p.ppid).unwrap_or(0)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn kill_process(app: tauri::AppHandle, pids: Vec<u32>, tree: bool) -> CmdResult<Value> {
    let (results, ended) = tauri::async_runtime::spawn_blocking(move || -> CmdResult<(Vec<Value>, usize)> {
        let all = fresh_snapshot()?;
        let own = std::process::id();
        let parent = parent_pid(&all, own);
        let by_pid: HashMap<u32, &Proc> = all.iter().map(|p| (p.pid, p)).collect();
        // our own WebView2 helper processes belong to Task Reaper itself: ending them blanks the window
        let own_tree: HashSet<u32> = descendants(&all, own).into_iter().collect();
        let mut out = Vec::new();
        let mut ended = 0usize;
        for pid in pids {
            let Some(p) = by_pid.get(&pid) else {
                out.push(json!({ "pid": pid, "ok": true, "message": "already gone" }));
                continue;
            };
            let why = if own_tree.contains(&pid) { Some("that is part of Task Reaper itself") } else { refuse_reason(p, own, parent) };
            if let Some(why) = why {
                out.push(json!({ "pid": pid, "ok": false, "message": why }));
                crate::activity::log(&app, "fail", format!("Refused to end {} (PID {pid}): {why}", p.name));
                continue;
            }
            if tree {
                for c in descendants(&all, pid) {
                    if let Some(cp) = by_pid.get(&c) {
                        if refuse_reason(cp, own, parent).is_none() && !own_tree.contains(&c) {
                            let _ = terminate(c);
                        }
                    }
                }
            }
            match terminate(pid) {
                Ok(()) => {
                    ended += 1;
                    out.push(json!({ "pid": pid, "ok": true, "message": "process ended" }));
                    crate::activity::log(&app, "ok", format!("Ended {} (PID {pid}){}", p.name, if tree { " with its child processes" } else { "" }));
                }
                Err(m) if m == "already gone" => out.push(json!({ "pid": pid, "ok": true, "message": m })),
                Err(m) => {
                    crate::activity::log(&app, "fail", format!("Could not end {} (PID {pid}): {m}", p.name));
                    out.push(json!({ "pid": pid, "ok": false, "message": m }));
                }
            }
        }
        Ok((out, ended))
    })
    .await
    .map_err(|e| CmdError(e.to_string()))??;
    let _ = ended;
    Ok(json!({ "results": results }))
}

/// Shared with the storage page: reveal an EXISTING file/folder in Explorer.
#[tauri::command]
pub fn open_location(path: String) -> CmdResult<Value> {
    let p = path.trim().trim_matches('"');
    if p.is_empty() || p.starts_with("\\\\") || !std::path::Path::new(p).exists() {
        // network (UNC) paths are refused: merely probing one makes Windows authenticate to the remote machine
        return err("That file no longer exists");
    }
    crate::util::reveal_in_explorer(p);
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(pid: u32, ppid: u32, start: u64, name: &str, exe: &str) -> Proc {
        Proc { pid, ppid, start, name: name.into(), exe: exe.into() }
    }

    #[test]
    fn protection_rules() {
        let sys = pr(500, 4, 1, "svchost.exe", "C:\\Windows\\System32\\svchost.exe");
        assert_eq!(refuse_reason(&sys, 1, 2), Some("protected system process"));
        assert_eq!(refuse_reason(&pr(0, 0, 0, "System Idle Process", ""), 1, 2), Some("protected system process"));
        assert_eq!(refuse_reason(&pr(4, 0, 0, "System", ""), 1, 2), Some("protected system process"));
        let app = pr(900, 800, 1, "notepad.exe", "C:\\Program Files\\x\\notepad.exe");
        assert_eq!(refuse_reason(&app, 1, 2), None);
        assert!(refuse_reason(&pr(1, 3, 1, "TaskReaper.exe", "C:\\x\\TaskReaper.exe"), 1, 2).is_some());
        assert!(refuse_reason(&pr(2, 3, 1, "cmd2.exe", "C:\\x\\cmd2.exe"), 1, 2).is_some());
        // WindowsApps-style names are caught by exe under C:\Windows even when name is unusual
        assert!(refuse_reason(&pr(77, 4, 1, "foo.exe", "C:\\WINDOWS\\system32\\foo.exe"), 1, 2).is_some());
    }

    #[test]
    fn tree_order_and_pid_reuse() {
        let all = vec![
            pr(10, 1, 1, "root", ""),
            pr(11, 10, 2, "a", ""),
            pr(12, 11, 3, "a1", ""),
            pr(13, 10, 2, "b", ""),
            pr(99, 10, 0, "stale-child-of-reused-pid", ""), // started before parent: ignore
            pr(20, 20, 5, "selfparent", ""),
        ];
        let d = descendants(&all, 10);
        assert_eq!(d.len(), 3);
        assert!(!d.contains(&99));
        let pos = |x| d.iter().position(|&v| v == x).unwrap();
        assert!(pos(12) < pos(11)); // grandchild before child
        assert!(descendants(&all, 20).is_empty());
    }

    #[test]
    fn tree_cycle_safe() {
        let all = vec![pr(1, 2, 5, "a", ""), pr(2, 1, 5, "b", "")];
        let d = descendants(&all, 1);
        assert_eq!(d, vec![2]);
    }

    #[test]
    fn open_location_rejects_missing() {
        assert!(open_location("C:\\definitely\\not\\here\\x.bin".into()).is_err());
        assert!(open_location("".into()).is_err());
    }

    /// Read-only: lists real processes (twice, so CPU is populated).
    #[test]
    #[ignore]
    fn smoke_list_processes() {
        let all = fresh_snapshot().unwrap();
        println!("{} processes", all.len());
        let mut s = sys().lock().unwrap();
        s.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let t = std::time::Instant::now();
        s.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        println!("refresh took {:?}", t.elapsed());
        let n = s.processes().len();
        let prot = s.processes().values().filter(|p| protect::is_protected_process(&proc_name(p), &proc_exe(p))).count();
        println!("{n} processes, {prot} protected");
        assert!(n > 20);
    }
}
