// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
#![allow(dead_code)] // commands are registered by the orchestrator in lib.rs
//! Optimizer page: RAM boost, junk categories, junk scan and junk clean (ported from optimizer.py).
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;
use crate::util::{self, err, CmdResult};

// ------------------------------------------------------------------ RAM boost

fn avail_and_percent() -> (u64, f64) {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe {
        if GlobalMemoryStatusEx(&mut m).is_err() || m.ullTotalPhys == 0 {
            return (0, 0.0);
        }
    }
    let used = m.ullTotalPhys.saturating_sub(m.ullAvailPhys);
    (m.ullAvailPhys, used as f64 * 100.0 / m.ullTotalPhys as f64)
}

fn trim_working_sets() -> u32 {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{EmptyWorkingSet, EnumProcesses};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
    };
    let mut pids = vec![0u32; 4096];
    let mut needed = 0u32;
    unsafe {
        let ok = EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut needed).is_ok();
        if !ok {
            return 0;
        }
    }
    let n = (needed as usize / 4).min(pids.len());
    let mut trimmed = 0;
    for &pid in &pids[..n] {
        if pid == 0 || pid == 4 {
            continue;
        }
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, false, pid) {
                if EmptyWorkingSet(h).is_ok() {
                    trimmed += 1;
                }
                let _ = CloseHandle(h);
            }
        }
    }
    trimmed
}

#[link(name = "ntdll")]
extern "system" {
    fn NtSetSystemInformation(class: u32, info: *mut core::ffi::c_void, len: u32) -> i32;
}

fn enable_privilege(name: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_SUCCESS, HANDLE};
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
        TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let w = util::wide(name);
        let mut luid = Default::default();
        let mut ok = LookupPrivilegeValueW(PCWSTR::null(), PCWSTR(w.as_ptr()), &mut luid).is_ok();
        if ok {
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
            };
            ok = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None).is_ok() && GetLastError() == ERROR_SUCCESS;
        }
        let _ = CloseHandle(token);
        ok
    }
}

/// Flush the standby list (admin only).
fn purge_standby() -> bool {
    if !util::is_admin() || !enable_privilege("SeProfileSingleProcessPrivilege") {
        return false;
    }
    let mut cmd: i32 = 4; // MemoryPurgeStandbyList
    unsafe { NtSetSystemInformation(80, &mut cmd as *mut i32 as *mut _, 4) == 0 }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn boost_ram(app: AppHandle) -> CmdResult<Value> {
    let r = tauri::async_runtime::spawn_blocking(|| {
        let (before, _) = avail_and_percent();
        let trimmed = trim_working_sets();
        let purged = purge_standby();
        std::thread::sleep(Duration::from_millis(800));
        let (after, percent) = avail_and_percent();
        (after.saturating_sub(before), trimmed, purged, percent)
    })
    .await?;
    crate::activity::log(
        &app,
        "ok",
        format!("RAM boost: freed {}, trimmed {} processes{}", fmt_bytes(r.0), r.1, if r.2 { ", standby list purged" } else { "" }),
    );
    Ok(json!({ "freed": r.0, "trimmed": r.1, "purged": r.2, "percent": (r.3 * 10.0).round() / 10.0 }))
}

// ------------------------------------------------------------------ junk categories

pub fn fmt_bytes(b: u64) -> String {
    let (mut v, mut i) = (b as f64, 0);
    let u = ["B", "KB", "MB", "GB", "TB"];
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", u[i]) }
}

const CATS: &[(&str, &str, &str, bool, bool)] = &[
    ("user_temp", "User temp files", "Leftovers from apps and installers", true, false),
    ("win_temp", "Windows temp files", "System temp folder (needs Administrator)", true, false),
    ("crash", "Crash dumps & error reports", "Memory dumps and Windows error reports", true, false),
    ("recycle", "Recycle Bin", "Permanently empties the bin", true, true),
    ("browser", "Browser caches", "Chrome, Edge, Brave, Firefox (you stay logged in)", false, false),
];

#[tauri::command(rename_all = "snake_case")]
pub fn get_junk_categories() -> CmdResult<Value> {
    let cats: Vec<Value> = CATS
        .iter()
        .map(|(id, title, desc, on, warn)| json!({ "id": id, "title": title, "desc": desc, "default_on": on, "warn": warn }))
        .collect();
    Ok(json!({ "categories": cats }))
}

/// Folders of a category (None = the Recycle Bin). Only these roots are ever touched.
fn category_roots(id: &str) -> Option<Option<Vec<PathBuf>>> {
    let env = |n: &str| std::env::var(n).ok().filter(|v| !v.trim().is_empty()).map(PathBuf::from);
    let la = env("LOCALAPPDATA");
    let win = env("SystemRoot").unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let mut v: Vec<PathBuf> = Vec::new();
    match id {
        "user_temp" => {
            v.extend(env("TEMP"));
            v.extend(la.as_ref().map(|p| p.join("Temp")));
        }
        "win_temp" => v.push(win.join("Temp")),
        "crash" => {
            v.extend(la.as_ref().map(|p| p.join("CrashDumps")));
            if let Some(pd) = env("ProgramData") {
                v.push(pd.join("Microsoft").join("Windows").join("WER").join("ReportArchive"));
                v.push(pd.join("Microsoft").join("Windows").join("WER").join("ReportQueue"));
            }
            v.push(win.join("Minidump"));
        }
        "recycle" => return Some(None),
        "browser" => v = browser_caches(la.as_deref()),
        _ => return None,
    }
    // dedupe case-insensitively (TEMP is usually LOCALAPPDATA\Temp), keep only safe roots that exist
    let mut seen = std::collections::HashSet::new();
    v.retain(|p| seen.insert(p.to_string_lossy().to_lowercase().trim_end_matches('\\').to_string()));
    v.retain(|p| safe_root(p) && p.is_dir());
    Some(Some(v))
}

fn browser_caches(la: Option<&Path>) -> Vec<PathBuf> {
    let Some(la) = la else { return vec![] };
    let mut out = vec![];
    for base in ["Google\\Chrome", "Microsoft\\Edge", "BraveSoftware\\Brave-Browser"] {
        let ud = la.join(base).join("User Data");
        let mut profiles = vec![ud.join("Default")];
        if let Ok(rd) = std::fs::read_dir(&ud) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("Profile ") {
                    profiles.push(e.path());
                }
            }
        }
        for p in profiles {
            for sub in ["Cache", "Code Cache", "GPUCache"] {
                out.push(p.join(sub));
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(la.join("Mozilla").join("Firefox").join("Profiles")) {
        for e in rd.flatten() {
            out.push(e.path().join("cache2"));
        }
    }
    out
}

/// A cleanable root must be absolute, at least 4 components (X:, dir, dir) deep and never a drive root.
fn safe_root(p: &Path) -> bool {
    p.is_absolute() && p.components().count() >= 4
}

// ------------------------------------------------------------------ walking

/// Size + file count of everything under `root` (symlinks/junctions are not followed, links count as nothing).
pub fn dir_size(root: &Path) -> (u64, u64) {
    let (mut bytes, mut files) = (0u64, 0u64);
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(e.path());
            } else if let Ok(m) = e.metadata() {
                bytes += m.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

struct Cleaner<'a> {
    freed: u64,
    failed: u64,
    done: u64,
    on_progress: &'a mut dyn FnMut(u64),
    last: Instant,
}

impl Cleaner<'_> {
    fn tick(&mut self, force: bool) {
        if force || self.last.elapsed() >= Duration::from_millis(100) {
            self.last = Instant::now();
            (self.on_progress)(self.done);
        }
    }

    /// Delete files below `dir` (post-order), then the emptied sub-folders. `dir` itself is kept.
    fn clean_dir(&mut self, dir: &Path) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            self.failed += 1;
            return;
        };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(ft) = e.file_type() else {
                self.failed += 1;
                continue;
            };
            if ft.is_symlink() {
                // never follow: drop the link itself only
                if std::fs::remove_file(&path).is_err() && std::fs::remove_dir(&path).is_err() {
                    self.failed += 1;
                }
            } else if ft.is_dir() {
                self.clean_dir(&path);
                let _ = std::fs::remove_dir(&path); // fails silently when something inside is in use
            } else {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        self.freed += size;
                        self.done += size;
                    }
                    Err(_) => self.failed += 1,
                }
            }
            self.tick(false);
        }
    }
}

/// Clean the contents of `roots`. Returns (freed bytes, failed items).
pub fn clean_roots(roots: &[PathBuf], on_progress: &mut dyn FnMut(u64)) -> (u64, u64) {
    let mut c = Cleaner { freed: 0, failed: 0, done: 0, on_progress, last: Instant::now() };
    for r in roots {
        if safe_root(r) {
            c.clean_dir(r);
        }
    }
    c.tick(true);
    (c.freed, c.failed)
}

// ------------------------------------------------------------------ recycle bin

fn recycle_size() -> (u64, u64) {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    unsafe {
        if SHQueryRecycleBinW(windows::core::PCWSTR::null(), &mut info).is_ok() {
            return (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64);
        }
    }
    (0, 0)
}

fn empty_recycle() {
    use windows::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
    unsafe {
        let _ = SHEmptyRecycleBinW(None, windows::core::PCWSTR::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND);
    }
}

// ------------------------------------------------------------------ commands

#[tauri::command(rename_all = "snake_case")]
pub async fn scan_junk(id: String) -> CmdResult<Value> {
    let Some(roots) = category_roots(&id) else { return err("Unknown cleanup category.") };
    let (bytes, files) = tauri::async_runtime::spawn_blocking(move || match roots {
        None => recycle_size(),
        Some(roots) => roots.iter().map(|r| dir_size(r)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1)),
    })
    .await?;
    Ok(json!({ "bytes": bytes, "files": files }))
}

#[tauri::command(rename_all = "snake_case")]
pub fn clean_start(app: AppHandle, state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<Value> {
    // re-validate: only known ids, each once, in the given order
    let mut valid: Vec<(String, Option<Vec<PathBuf>>)> = vec![];
    for id in ids {
        if valid.iter().any(|(i, _)| *i == id) {
            continue;
        }
        match category_roots(&id) {
            Some(r) => valid.push((id, r)),
            None => return err("Unknown cleanup category."),
        }
    }
    if valid.is_empty() {
        return err("Nothing selected to clean.");
    }
    let (job, _cancel) = state.jobs.start();
    let jid = job.clone();
    std::thread::spawn(move || {
        let admin = util::is_admin();
        let mut results = vec![];
        let mut total = 0u64;
        for (id, roots) in valid {
            let progress = |done: u64, tot: u64| {
                let _ = app.emit("clean:progress", json!({ "job": jid, "id": id, "done_bytes": done, "total_bytes": tot }));
            };
            let (freed, failed) = if id == "win_temp" && !admin {
                (0, 1) // needs Administrator: skipped and reported
            } else {
                match roots {
                    None => {
                        let (size, _) = recycle_size();
                        progress(0, size);
                        empty_recycle();
                        progress(size, size);
                        (size, 0)
                    }
                    Some(roots) => {
                        let tot: u64 = roots.iter().map(|r| dir_size(r).0).sum();
                        progress(0, tot);
                        clean_roots(&roots, &mut |d| progress(d.min(tot), tot))
                    }
                }
            };
            total += freed;
            crate::activity::log(
                &app,
                if failed == 0 || freed > 0 { "ok" } else { "fail" },
                format!("Cleaned {}: freed {}{}", id, fmt_bytes(freed), if failed > 0 { format!(", {failed} skipped") } else { String::new() }),
            );
            results.push(json!({ "id": id, "freed": freed, "failed": failed }));
        }
        let _ = app.emit("clean:done", json!({ "job": jid, "results": results, "total_freed": total }));
        if let Some(st) = tauri::Manager::try_state::<AppState>(&app) {
            st.jobs.finish(&jid);
        }
    });
    Ok(json!({ "job": job }))
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pk_cleanup_test_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn categories_match_contract() {
        let v = get_junk_categories().unwrap();
        let ids: Vec<&str> = v["categories"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["user_temp", "win_temp", "crash", "recycle", "browser"]);
        assert_eq!(v["categories"][3]["warn"], true);
        assert_eq!(v["categories"][4]["default_on"], false);
    }

    #[test]
    fn path_expansion() {
        assert!(category_roots("nope").is_none());
        assert!(matches!(category_roots("recycle"), Some(None)));
        let r = category_roots("win_temp").unwrap().unwrap_or_default();
        let w = util::win_dir();
        assert!(r.iter().all(|p| p.to_string_lossy().to_lowercase().starts_with(&w)));
        let u = category_roots("user_temp").unwrap().unwrap();
        let mut s: Vec<String> = u.iter().map(|p| p.to_string_lossy().to_lowercase()).collect();
        let n = s.len();
        s.dedup();
        assert_eq!(n, s.len());
        assert!(!safe_root(Path::new("C:\\")));
        assert!(!safe_root(Path::new("C:\\Windows")));
        assert!(!safe_root(Path::new("relative\\a\\b")));
        assert!(safe_root(Path::new("C:\\Users\\x\\Temp")));
    }

    #[test]
    fn clean_deletes_files_keeps_root_and_skips_locked() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tmp("clean").join("Temp");
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        fs::create_dir_all(root.join("empty")).unwrap();
        fs::write(root.join("x.tmp"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("a").join("y.tmp"), vec![0u8; 500]).unwrap();
        fs::write(root.join("a").join("b").join("z.tmp"), vec![0u8; 250]).unwrap();
        // locked file (no sharing) in its own folder
        fs::create_dir_all(root.join("lock")).unwrap();
        let lp = root.join("lock").join("held.tmp");
        let mut held = fs::OpenOptions::new().create(true).write(true).share_mode(0).open(&lp).unwrap();
        held.write_all(b"x").unwrap();
        let (sz, nf) = dir_size(&root);
        assert!(sz >= 1750 && nf == 4);
        let mut last = 0;
        let (freed, failed) = clean_roots(&[root.clone()], &mut |d| last = d);
        assert_eq!(freed, 1750);
        assert_eq!(failed, 1);
        assert_eq!(last, 1750);
        assert!(root.is_dir(), "category root must be kept");
        assert!(lp.exists() && root.join("lock").is_dir(), "locked file and its folder remain");
        assert!(!root.join("a").exists() && !root.join("empty").exists() && !root.join("x.tmp").exists());
        drop(held);
        let (f2, e2) = clean_roots(&[root.clone()], &mut |_| {});
        assert_eq!((f2, e2), (1, 0));
        assert!(root.is_dir() && fs::read_dir(&root).unwrap().next().is_none());
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn clean_refuses_unsafe_root() {
        let (f, e) = clean_roots(&[PathBuf::from("C:\\")], &mut |_| {});
        assert_eq!((f, e), (0, 0));
    }

    #[test]
    fn clean_does_not_follow_junction() {
        let base = tmp("junction");
        let target = base.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep.txt"), b"keep").unwrap();
        let root = base.join("Temp");
        fs::create_dir_all(&root).unwrap();
        let ok = util::run_hidden("cmd", &["/c", "mklink", "/J", &root.join("lnk").to_string_lossy(), &target.to_string_lossy()]).0;
        if ok {
            clean_roots(&[root.clone()], &mut |_| {});
            assert!(target.join("keep.txt").exists(), "junction target must survive");
            assert!(!root.join("lnk").exists());
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn bytes_format() {
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(1536), "1.5 KB");
    }

    #[test]
    #[ignore]
    fn smoke_read_only() {
        for (id, ..) in CATS {
            let roots = category_roots(id).unwrap();
            let r = match roots {
                None => recycle_size(),
                Some(r) => r.iter().map(|p| dir_size(p)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1)),
            };
            println!("{id}: {} bytes, {} files", r.0, r.1);
        }
        println!("mem: {:?}", avail_and_percent());
    }
}
