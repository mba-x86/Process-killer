// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
#![allow(dead_code)] // commands are registered by the orchestrator in lib.rs
//! Storage analyzer: parallel directory walk keeping the N largest files and first-level folder totals,
//! native folder picker and Recycle Bin deletes (ported from optimizer.py ScanState/scan_disk/delete_to_recycle).
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::util::{self, err, CmdResult};

const TOP_N: usize = 3000;
const UI_FILES: usize = 1500;
const UI_FOLDERS: usize = 500;
const PROGRESS_EVERY: Duration = Duration::from_millis(1500);

static SCAN_RUNNING: AtomicBool = AtomicBool::new(false);

type Item = (u64, String, u64); // size, path, mtime (heap orders by size first)

/// Keeps the `cap` largest items.
pub struct TopN {
    cap: usize,
    heap: BinaryHeap<Reverse<Item>>,
}
impl TopN {
    pub fn new(cap: usize) -> Self {
        TopN { cap, heap: BinaryHeap::new() }
    }
    /// Smallest size that would still be accepted (0 while not full).
    pub fn threshold(&self) -> u64 {
        if self.heap.len() < self.cap { 0 } else { self.heap.peek().map_or(0, |r| r.0 .0) }
    }
    pub fn push(&mut self, it: Item) {
        if self.heap.len() < self.cap {
            self.heap.push(Reverse(it));
        } else if self.heap.peek().is_some_and(|r| it.0 > r.0 .0) {
            self.heap.pop();
            self.heap.push(Reverse(it));
        }
    }
    /// Largest first.
    pub fn sorted(&self, limit: usize) -> Vec<Item> {
        let mut v: Vec<Item> = self.heap.iter().map(|r| r.0.clone()).collect();
        v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        v.truncate(limit);
        v
    }
}

#[derive(Clone)]
struct Work {
    path: PathBuf,
    key: Arc<str>,       // first-level folder key this subtree is accounted to
    is_root: bool,
    root: Arc<str>,      // root path without trailing backslash
}

struct Queue {
    stack: Vec<Work>,
    active: usize,
}

struct Shared {
    min_size: u64,
    files: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    threshold: AtomicU64,
    cancel: Arc<AtomicBool>,
    top: Mutex<TopN>,
    folders: Mutex<HashMap<Arc<str>, u64>>,
    current: Mutex<String>,
    q: Mutex<Queue>,
    cv: Condvar,
    dirs_done: AtomicUsize,
}

fn mtime_secs(m: &std::fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

fn worker(sh: &Shared) {
    loop {
        let item = {
            let mut q = sh.q.lock().unwrap();
            loop {
                if sh.cancel.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(w) = q.stack.pop() {
                    q.active += 1;
                    break w;
                }
                if q.active == 0 {
                    sh.cv.notify_all();
                    return;
                }
                q = sh.cv.wait(q).unwrap();
            }
        };
        process_dir(sh, &item);
        let mut q = sh.q.lock().unwrap();
        q.active -= 1;
        drop(q);
        sh.cv.notify_all();
    }
}

fn process_dir(sh: &Shared, w: &Work) {
    if sh.dirs_done.fetch_add(1, Ordering::Relaxed) % 16 == 0 {
        if let Ok(mut c) = sh.current.try_lock() {
            c.clear();
            c.push_str(&w.path.to_string_lossy());
        }
    }
    let rd = match std::fs::read_dir(&w.path) {
        Ok(r) => r,
        Err(_) => {
            sh.errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };
    let mut subdirs: Vec<Work> = Vec::new();
    let (mut files, mut bytes, mut errors) = (0u64, 0u64, 0u64);
    let mut cands: Vec<Item> = Vec::new();
    let thr = sh.threshold.load(Ordering::Relaxed).max(sh.min_size.saturating_sub(1));
    for e in rd {
        let Ok(e) = e else {
            errors += 1;
            continue;
        };
        let Ok(ft) = e.file_type() else {
            errors += 1;
            continue;
        };
        if ft.is_symlink() {
            continue; // symlinks and junctions are never followed
        }
        if ft.is_dir() {
            let key = if w.is_root {
                Arc::from(format!("{}\\{}", w.root, e.file_name().to_string_lossy()))
            } else {
                w.key.clone()
            };
            subdirs.push(Work { path: e.path(), key, is_root: false, root: w.root.clone() });
        } else if ft.is_file() {
            match e.metadata() {
                Ok(m) => {
                    let sz = m.len();
                    files += 1;
                    bytes += sz;
                    if sz > thr && sz >= sh.min_size {
                        cands.push((sz, e.path().to_string_lossy().into_owned(), mtime_secs(&m)));
                    }
                }
                Err(_) => errors += 1,
            }
        }
    }
    sh.files.fetch_add(files, Ordering::Relaxed);
    sh.bytes.fetch_add(bytes, Ordering::Relaxed);
    if errors > 0 {
        sh.errors.fetch_add(errors, Ordering::Relaxed);
    }
    if bytes > 0 {
        let mut f = sh.folders.lock().unwrap();
        match f.get_mut(&w.key) {
            Some(v) => *v += bytes,
            None => {
                f.insert(w.key.clone(), bytes);
            }
        }
    }
    if !cands.is_empty() {
        let mut t = sh.top.lock().unwrap();
        for c in cands {
            t.push(c);
        }
        sh.threshold.store(t.threshold(), Ordering::Relaxed);
    }
    if !subdirs.is_empty() {
        let mut q = sh.q.lock().unwrap();
        q.stack.extend(subdirs);
        drop(q);
        sh.cv.notify_all();
    }
}

fn snapshot(sh: &Shared, job: &str, total_bytes: Option<u64>) -> Value {
    let top: Vec<Value> = sh
        .top
        .lock()
        .unwrap()
        .sorted(UI_FILES)
        .into_iter()
        .map(|(size, path, mtime)| json!({ "path": path, "size": size, "mtime": mtime }))
        .collect();
    let mut fv: Vec<(Arc<str>, u64)> = sh.folders.lock().unwrap().iter().map(|(k, v)| (k.clone(), *v)).collect();
    fv.sort_by(|a, b| b.1.cmp(&a.1));
    fv.truncate(UI_FOLDERS);
    let folders: Vec<Value> = fv.into_iter().map(|(p, s)| json!({ "path": &*p, "size": s })).collect();
    json!({
        "job": job,
        "files": sh.files.load(Ordering::Relaxed),
        "bytes": sh.bytes.load(Ordering::Relaxed),
        "errors": sh.errors.load(Ordering::Relaxed),
        "current": sh.current.lock().unwrap().clone(),
        "total_bytes": total_bytes,
        "top_files": top,
        "folders": folders,
    })
}

struct ScanResult {
    cancelled: bool,
    sh: Arc<Shared>,
}

/// Blocking scan used by the job thread and by tests. `on_progress` is called about every `PROGRESS_EVERY`.
fn run_scan(roots: &[PathBuf], min_size: u64, cancel: Arc<AtomicBool>, threads: usize, on_progress: &mut dyn FnMut(&Shared)) -> ScanResult {
    let mut stack = vec![];
    for r in roots {
        let s = r.to_string_lossy().into_owned();
        let trimmed: String = s.trim_end_matches('\\').to_string();
        let key: Arc<str> = Arc::from(format!("{trimmed}  (files in this folder)"));
        stack.push(Work { path: r.clone(), key, is_root: true, root: Arc::from(trimmed) });
    }
    let sh = Arc::new(Shared {
        min_size,
        files: 0.into(),
        bytes: 0.into(),
        errors: 0.into(),
        threshold: 0.into(),
        cancel,
        top: Mutex::new(TopN::new(TOP_N)),
        folders: Mutex::new(HashMap::new()),
        current: Mutex::new(String::new()),
        q: Mutex::new(Queue { stack, active: 0 }),
        cv: Condvar::new(),
        dirs_done: 0.into(),
    });
    let handles: Vec<_> = (0..threads.max(1))
        .map(|_| {
            let s = sh.clone();
            std::thread::spawn(move || worker(&s))
        })
        .collect();
    let mut last = Instant::now();
    while !handles.iter().all(|h| h.is_finished()) {
        std::thread::sleep(Duration::from_millis(40));
        if last.elapsed() >= PROGRESS_EVERY {
            last = Instant::now();
            on_progress(&sh);
        }
    }
    for h in handles {
        let _ = h.join();
    }
    let cancelled = sh.cancel.load(Ordering::Relaxed);
    ScanResult { cancelled, sh }
}

fn drive_used(root: &str) -> Option<u64> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let w = util::wide(root);
    let (mut total, mut free) = (0u64, 0u64);
    unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), None, Some(&mut total), Some(&mut free)).ok()? };
    Some(total.saturating_sub(free))
}

fn is_drive_root(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() && p[2..].trim_matches('\\').is_empty()
}

fn clean_root(s: &str) -> String {
    let s = s.trim().trim_matches('"').replace('/', "\\");
    if is_drive_root(&s) { format!("{}:\\", &s[..1]) } else { s.trim_end_matches('\\').to_string() }
}

#[tauri::command(rename_all = "snake_case")]
pub fn storage_scan_start(app: AppHandle, state: State<'_, AppState>, roots: Vec<String>, min_size: Option<u64>) -> CmdResult<Value> {
    let roots: Vec<String> = roots.iter().map(|r| clean_root(r)).filter(|r| !r.is_empty()).collect();
    if roots.is_empty() {
        return err("Choose a drive or folder to scan.");
    }
    for r in &roots {
        if !Path::new(r).is_dir() {
            return err(format!("Cannot open {r}. Choose an existing drive or folder."));
        }
    }
    if SCAN_RUNNING.swap(true, Ordering::SeqCst) {
        return err("A scan is already running.");
    }
    let total_bytes: Option<u64> = if roots.iter().all(|r| is_drive_root(r)) {
        roots.iter().map(|r| drive_used(r)).sum::<Option<u64>>()
    } else {
        None
    };
    let (job, cancel) = state.jobs.start();
    let jid = job.clone();
    let min_size = min_size.unwrap_or(1_048_576);
    std::thread::spawn(move || {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                SCAN_RUNNING.store(false, Ordering::SeqCst);
            }
        }
        let _g = Guard;
        let paths: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(4, 12);
        let res = run_scan(&paths, min_size, cancel, threads, &mut |sh| {
            let _ = app.emit("storage:progress", snapshot(sh, &jid, total_bytes));
        });
        let _ = app.emit("storage:progress", snapshot(&res.sh, &jid, total_bytes));
        let errors = res.sh.errors.load(Ordering::Relaxed);
        let _ = app.emit("storage:done", json!({ "job": jid, "cancelled": res.cancelled, "errors": errors }));
        crate::activity::log(
            &app,
            "info",
            format!(
                "Storage scan {}: {} files, {}",
                if res.cancelled { "cancelled" } else { "finished" },
                res.sh.files.load(Ordering::Relaxed),
                crate::cleanup::fmt_bytes(res.sh.bytes.load(Ordering::Relaxed))
            ),
        );
        if let Some(st) = app.try_state::<AppState>() {
            st.jobs.finish(&jid);
        }
    });
    Ok(json!({ "job": job }))
}

#[tauri::command(rename_all = "snake_case")]
pub fn storage_scan_cancel(state: State<'_, AppState>, job: String) -> CmdResult<Value> {
    state.jobs.cancel(&job);
    Ok(Value::Null)
}

// ------------------------------------------------------------------ folder picker

fn pick_folder(owner: isize) -> Option<String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};
    unsafe {
        let inited = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let out = (|| {
            let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let opts = dlg.GetOptions().ok()?;
            dlg.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
            dlg.Show(Some(HWND(owner as *mut _))).ok()?; // Err when dismissed
            let item = dlg.GetResult().ok()?;
            let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            s
        })();
        if inited {
            CoUninitialize();
        }
        out
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn choose_folder(window: tauri::WebviewWindow) -> CmdResult<Value> {
    let owner = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
    let p = tauri::async_runtime::spawn_blocking(move || pick_folder(owner)).await?;
    Ok(json!({ "path": p }))
}

// ------------------------------------------------------------------ recycle

/// Normalise lexically (no `..`, forward slashes) and require an absolute `X:\...` path.
pub fn validate_path(p: &str) -> Option<String> {
    let s = p.trim().trim_matches('"').replace('/', "\\");
    let b = s.as_bytes();
    if b.len() < 4 || b[1] != b':' || b[2] != b'\\' || !b[0].is_ascii_alphabetic() {
        return None;
    }
    if s.contains(['*', '?', '<', '>', '|', '\0']) || s[2..].split('\\').any(|c| c == ".." || c == ".") {
        return None;
    }
    Some(s.trim_end_matches('\\').to_string())
}

fn recycle_one(path: &str) -> (bool, String) {
    let Some(p) = validate_path(path) else { return (false, "invalid path".into()) };
    if crate::protect::is_protected_file(&p) {
        return (false, "protected system file".into());
    }
    let pb = Path::new(&p);
    let Ok(meta) = std::fs::symlink_metadata(pb) else { return (true, "already gone".into()) };
    if !meta.is_file() && !meta.file_type().is_symlink() {
        return (false, "not a file".into());
    }
    // a link could point into the Windows folder: judge the real location too
    if let Ok(real) = std::fs::canonicalize(pb) {
        let r = real.to_string_lossy().trim_start_matches("\\\\?\\").to_string();
        if crate::protect::is_protected_file(&r) {
            return (false, "protected system file".into());
        }
    }
    // IFileOperation + FOFX_RECYCLEONDELETE (src/recycle.rs): an item that cannot be recycled FAILS instead of being
    // silently destroyed (SHFileOperationW permanently deletes items too large for the bin).
    match crate::recycle::recycle(&p) {
        Ok(()) => (true, "moved to Recycle Bin".into()),
        Err(msg) => (false, msg),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn recycle_files(app: AppHandle, paths: Vec<String>) -> CmdResult<Value> {
    let rows = tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| {
                let (ok, msg) = recycle_one(&p);
                json!({ "path": p, "ok": ok, "message": msg })
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let moved = rows.iter().filter(|r| r["ok"] == true && r["message"] == "moved to Recycle Bin").count();
    if moved > 0 {
        crate::activity::log(&app, "ok", format!("Moved {moved} file(s) to the Recycle Bin"));
    }
    let failed = rows.iter().filter(|r| r["ok"] == false).count();
    if failed > 0 {
        crate::activity::log(&app, "fail", format!("{failed} file(s) could not be moved to the Recycle Bin"));
    }
    Ok(json!({ "results": rows }))
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pk_storage_test_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn topn_keeps_largest() {
        let mut t = TopN::new(3);
        assert_eq!(t.threshold(), 0);
        for (i, s) in [5u64, 1, 9, 7, 3, 8].iter().enumerate() {
            t.push((*s, format!("f{i}"), 0));
        }
        let v: Vec<u64> = t.sorted(10).iter().map(|i| i.0).collect();
        assert_eq!(v, [9, 8, 7]);
        assert_eq!(t.threshold(), 7);
        assert_eq!(t.sorted(2).len(), 2);
    }

    #[test]
    fn protection_and_path_validation() {
        assert!(validate_path("relative\\x").is_none());
        assert!(validate_path("C:\\a\\..\\b").is_none());
        assert!(validate_path("C:\\a\\*.txt").is_none());
        assert_eq!(validate_path("C:/a/b.txt").unwrap(), "C:\\a\\b.txt");
        assert_eq!(recycle_one("C:\\pagefile.sys"), (false, "protected system file".into()));
        assert_eq!(recycle_one("D:\\HIBERFIL.SYS"), (false, "protected system file".into()));
        let w = format!("{}\\System32\\notepad.exe", std::env::var("SystemRoot").unwrap());
        assert_eq!(recycle_one(&w), (false, "protected system file".into()));
        assert_eq!(recycle_one("C:\\this_does_not_exist_pk\\x.bin"), (true, "already gone".into()));
        assert!(!recycle_one("nonsense").0);
    }

    fn mk(root: &Path, rel: &str, size: usize) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![1u8; size]).unwrap();
    }

    #[test]
    fn scan_orders_files_and_totals_folders() {
        let root = tmp("scan");
        mk(&root, "top.bin", 3000);
        mk(&root, "a\\big.bin", 9000);
        mk(&root, "a\\deep\\mid.bin", 5000);
        mk(&root, "a\\deep\\tiny.bin", 10);
        mk(&root, "b\\x.bin", 4000);
        let r = run_scan(&[root.clone()], 1000, Arc::new(AtomicBool::new(false)), 4, &mut |_| {});
        assert!(!r.cancelled);
        assert_eq!(r.sh.files.load(Ordering::Relaxed), 5);
        assert_eq!(r.sh.bytes.load(Ordering::Relaxed), 21010);
        let top: Vec<u64> = r.sh.top.lock().unwrap().sorted(10).iter().map(|i| i.0).collect();
        assert_eq!(top, [9000, 5000, 4000, 3000]);
        let f = r.sh.folders.lock().unwrap();
        let base = root.to_string_lossy().to_string();
        assert_eq!(f[&*format!("{base}\\a")], 14010);
        assert_eq!(f[&*format!("{base}\\b")], 4000);
        assert_eq!(f[&*format!("{base}  (files in this folder)")], 3000);
        drop(f);
        let snap = snapshot(&r.sh, "j1", None);
        assert_eq!(snap["top_files"][0]["size"], 9000);
        assert!(snap["total_bytes"].is_null());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_cancel_stops() {
        let root = tmp("cancel");
        for i in 0..50 {
            mk(&root, &format!("d{i}\\f.bin"), 2000);
        }
        let cancel = Arc::new(AtomicBool::new(true));
        let r = run_scan(&[root.clone()], 1, cancel, 4, &mut |_| {});
        assert!(r.cancelled);
        assert!(r.sh.files.load(Ordering::Relaxed) < 50);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_counts_errors_and_skips_junction() {
        let root = tmp("junc");
        mk(&root, "real\\f.bin", 2000);
        let link = root.join("link");
        let made = util::run_hidden("cmd", &["/c", "mklink", "/J", &link.to_string_lossy(), &root.join("real").to_string_lossy()]).0;
        let r = run_scan(&[root.clone()], 1, Arc::new(AtomicBool::new(false)), 2, &mut |_| {});
        assert_eq!(r.sh.files.load(Ordering::Relaxed), 1, "junction must not be followed (made={made})");
        let missing = run_scan(&[root.join("nope")], 1, Arc::new(AtomicBool::new(false)), 2, &mut |_| {});
        assert_eq!(missing.sh.errors.load(Ordering::Relaxed), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn root_helpers() {
        assert!(is_drive_root("C:\\") && is_drive_root("d:"));
        assert!(!is_drive_root("C:\\Users"));
        assert_eq!(clean_root("c:"), "c:\\");
        assert_eq!(clean_root("D:\\Games\\"), "D:\\Games");
    }

    #[test]
    #[ignore]
    fn smoke_scan_ui_folder() {
        let root = PathBuf::from(r"E:\DEV\Process Killer\tauri-app\ui");
        let t = Instant::now();
        let r = run_scan(&[root], 1024, Arc::new(AtomicBool::new(false)), 8, &mut |_| {});
        println!("{} files, {} bytes, {} errors in {:?}", r.sh.files.load(Ordering::Relaxed), r.sh.bytes.load(Ordering::Relaxed), r.sh.errors.load(Ordering::Relaxed), t.elapsed());
        for (s, p, _) in r.sh.top.lock().unwrap().sorted(5) {
            println!("  {s:>10}  {p}");
        }
        for (k, v) in r.sh.folders.lock().unwrap().iter() {
            println!("  folder {v:>10}  {k}");
        }
    }

    #[test]
    #[ignore]
    fn bench_scan() {
        let root = PathBuf::from(std::env::var("PK_BENCH_ROOT").unwrap_or_else(|_| r"E:\DEV".into()));
        for th in [1usize, 4, 8, 12] {
            let t = Instant::now();
            let r = run_scan(&[root.clone()], 1_048_576, Arc::new(AtomicBool::new(false)), th, &mut |_| {});
            println!("threads={th}: {} files in {:?}", r.sh.files.load(Ordering::Relaxed), t.elapsed());
        }
    }
}
