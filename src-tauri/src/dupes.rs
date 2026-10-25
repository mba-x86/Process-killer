// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Duplicate finder for the storage analyzer. Same-size files are compared by a hash of their first 64 KB and then by a full
//! SHA-256, so a "duplicate" is always byte-identical. Safety rules:
//!   - cloud placeholders (OneDrive etc.) are never read: hashing one would download the whole file;
//!   - hard links to the same data are one file, not duplicates (deleting one frees nothing);
//!   - symlinks/junctions are not followed, protected Windows folders are not walked;
//!   - deleting is done by `dupes_recycle`, which refuses to remove a file unless a kept copy still exists and
//!     nothing changed since the scan (size + modified time).
use std::collections::HashMap;
use std::io::Read;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;
use crate::util::{err, CmdResult};

const HEAD: u64 = 64 * 1024;
const MAX_GROUPS: usize = 400;
const MAX_FILES_PER_GROUP: usize = 60;
const PROGRESS_EVERY: Duration = Duration::from_millis(700);

// FILE_ATTRIBUTE_* values of files that live in the cloud: reading them triggers a download.
const ATTR_OFFLINE: u32 = 0x1000;
const ATTR_RECALL_ON_OPEN: u32 = 0x4_0000;
const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
const CLOUD_MASK: u32 = ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS;

pub fn is_cloud_placeholder(attrs: u32) -> bool {
    attrs & CLOUD_MASK != 0
}

type Digest32 = [u8; 32];

#[derive(Default)]
pub struct Prog {
    pub phase: AtomicUsize, // 0 walking, 1 comparing the first KB, 2 hashing whole files, 3 done
    pub files: AtomicU64,
    pub candidates: AtomicU64,
    pub hash_total: AtomicU64,
    pub hashed: AtomicU64,
    pub errors: AtomicU64,
    pub skipped_cloud: AtomicU64,
    pub current: Mutex<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DupFile {
    pub path: String,
    pub mtime: u64,
}

#[derive(Clone, Debug)]
pub struct DupGroup {
    pub size: u64,
    pub files: Vec<DupFile>,
}

pub struct Outcome {
    pub cancelled: bool,
    pub groups: Vec<DupGroup>, // every group, biggest waste first
}

fn mtime_secs(m: &std::fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

fn skip_dir(name: &str) -> bool {
    let n = name.to_lowercase();
    n == "$recycle.bin" || n == "system volume information" || n == "$windows.~bt" || crate::protect::is_protected_dir_name(&n)
}

/// Phase 0: every file >= `min_size`, grouped by size (only sizes that occur twice or more are kept).
fn walk(roots: &[PathBuf], min_size: u64, cancel: &AtomicBool, p: &Prog) -> HashMap<u64, Vec<DupFile>> {
    let mut by_size: HashMap<u64, Vec<DupFile>> = HashMap::new();
    let mut stack: Vec<PathBuf> = roots.to_vec();
    let mut seen = 0u64;
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if crate::protect::is_protected_file(&dir.to_string_lossy()) {
            continue;
        }
        seen += 1;
        if seen % 16 == 1 {
            if let Ok(mut c) = p.current.try_lock() {
                *c = dir.to_string_lossy().into_owned();
            }
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            p.errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        for e in rd {
            let Ok(e) = e else {
                p.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            };
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                if !skip_dir(&e.file_name().to_string_lossy()) {
                    stack.push(e.path());
                }
            } else if ft.is_file() {
                let Ok(m) = e.metadata() else {
                    p.errors.fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                p.files.fetch_add(1, Ordering::Relaxed);
                if m.len() < min_size || m.len() == 0 {
                    continue;
                }
                if is_cloud_placeholder(m.file_attributes()) {
                    p.skipped_cloud.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                by_size.entry(m.len()).or_default().push(DupFile { path: e.path().to_string_lossy().into_owned(), mtime: mtime_secs(&m) });
            }
        }
    }
    by_size.retain(|_, v| v.len() > 1);
    by_size
}

fn hash_file(path: &str, limit: u64, cancel: &AtomicBool, p: &Prog) -> Option<Digest32> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut left = limit;
    while left > 0 {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let want = left.min(buf.len() as u64) as usize;
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            return None; // file shrank since the walk: cannot be a duplicate any more
        }
        h.update(&buf[..n]);
        p.hashed.fetch_add(n as u64, Ordering::Relaxed);
        left -= n as u64;
    }
    Some(h.finalize().into())
}

/// Run `f` over `items` on a few threads; results keep the order of `items`.
fn par_hash<T: Sync>(items: &[T], threads: usize, f: &(dyn Fn(&T) -> Option<Digest32> + Sync)) -> Vec<Option<Digest32>> {
    let out: Mutex<Vec<Option<Digest32>>> = Mutex::new(vec![None; items.len()]);
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    return;
                }
                let r = f(&items[i]);
                out.lock().unwrap()[i] = r;
            });
        }
    });
    out.into_inner().unwrap()
}

/// (volume serial, file index) identifies the data behind a path: two paths with the same id are hard links.
fn file_id(path: &str) -> Option<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    let f = std::fs::File::open(path).ok()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut info).ok()? };
    Some((info.dwVolumeSerialNumber, ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64))
}

fn collapse_hard_links(files: Vec<DupFile>) -> Vec<DupFile> {
    let mut seen: Vec<(u32, u64)> = Vec::new();
    let mut out = Vec::with_capacity(files.len());
    for f in files {
        match file_id(&f.path) {
            Some(id) if seen.contains(&id) => {}
            Some(id) => {
                seen.push(id);
                out.push(f);
            }
            None => out.push(f),
        }
    }
    out
}

/// Blocking scan used by the job thread and by the tests.
/// Progress is read from `p` by whoever owns the AppHandle (see `dupes_scan_start`); this function only updates it.
pub fn find_duplicates(roots: &[PathBuf], min_size: u64, cancel: &AtomicBool, threads: usize, p: &Prog) -> Outcome {
    let cancelled = |c: &AtomicBool| c.load(Ordering::Relaxed);
    let result = {
        let r = (|| {
            p.phase.store(0, Ordering::Relaxed);
            let by_size = walk(roots, min_size, cancel, p);
            let cand: Vec<(u64, DupFile)> = by_size.into_iter().flat_map(|(sz, v)| v.into_iter().map(move |f| (sz, f))).collect();
            p.candidates.store(cand.len() as u64, Ordering::Relaxed);
            if cancelled(cancel) {
                return Vec::new();
            }
            // phase 1: first 64 KB
            p.phase.store(1, Ordering::Relaxed);
            p.hash_total.store(cand.iter().map(|(sz, _)| (*sz).min(HEAD)).sum(), Ordering::Relaxed);
            p.hashed.store(0, Ordering::Relaxed);
            let heads = par_hash(&cand, threads, &|(sz, f)| hash_file(&f.path, (*sz).min(HEAD), cancel, p));
            if cancelled(cancel) {
                return Vec::new();
            }
            let mut groups: HashMap<(u64, Digest32), Vec<DupFile>> = HashMap::new();
            for ((sz, f), h) in cand.into_iter().zip(heads) {
                match h {
                    Some(h) => groups.entry((sz, h)).or_default().push(f),
                    None => {
                        p.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            groups.retain(|_, v| v.len() > 1);
            // phase 2: whole files (files that fit in the first 64 KB are already fully compared)
            p.phase.store(2, Ordering::Relaxed);
            let (small, big): (Vec<_>, Vec<_>) = groups.into_iter().partition(|((sz, _), _)| *sz <= HEAD);
            let mut full: Vec<DupGroup> = small.into_iter().map(|((sz, _), files)| DupGroup { size: sz, files }).collect();
            let todo: Vec<(u64, DupFile)> = big.into_iter().flat_map(|((sz, _), v)| v.into_iter().map(move |f| (sz, f))).collect();
            p.hash_total.store(todo.iter().map(|(sz, _)| *sz).sum(), Ordering::Relaxed);
            p.hashed.store(0, Ordering::Relaxed);
            let fulls = par_hash(&todo, threads, &|(sz, f)| hash_file(&f.path, *sz, cancel, p));
            if cancelled(cancel) {
                return Vec::new();
            }
            let mut g2: HashMap<(u64, Digest32), Vec<DupFile>> = HashMap::new();
            for ((sz, f), h) in todo.into_iter().zip(fulls) {
                match h {
                    Some(h) => g2.entry((sz, h)).or_default().push(f),
                    None => {
                        p.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            full.extend(g2.into_iter().filter(|(_, v)| v.len() > 1).map(|((sz, _), files)| DupGroup { size: sz, files }));
            // hard links are the same data: keep one path per file id, drop groups that end up with a single file
            let mut out: Vec<DupGroup> = full
                .into_iter()
                .filter_map(|g| {
                    let mut files = collapse_hard_links(g.files);
                    if files.len() < 2 {
                        return None;
                    }
                    files.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
                    Some(DupGroup { size: g.size, files })
                })
                .collect();
            out.sort_by(|a, b| (b.size * (b.files.len() as u64 - 1)).cmp(&(a.size * (a.files.len() as u64 - 1))).then_with(|| a.files[0].path.cmp(&b.files[0].path)));
            out
        })();
        r
    };
    p.phase.store(3, Ordering::Relaxed);
    Outcome { cancelled: cancel.load(Ordering::Relaxed), groups: result }
}

// ------------------------------------------------------------------ commands

fn progress_json(job: &str, p: &Prog) -> Value {
    let phase = ["walk", "head", "hash", "done"][p.phase.load(Ordering::Relaxed).min(3)];
    json!({
        "job": job,
        "phase": phase,
        "files": p.files.load(Ordering::Relaxed),
        "candidates": p.candidates.load(Ordering::Relaxed),
        "hash_total": p.hash_total.load(Ordering::Relaxed),
        "hashed": p.hashed.load(Ordering::Relaxed),
        "errors": p.errors.load(Ordering::Relaxed),
        "skipped_cloud": p.skipped_cloud.load(Ordering::Relaxed),
        "current": p.current.lock().map(|c| c.clone()).unwrap_or_default(),
    })
}

fn group_json(g: &DupGroup) -> Value {
    let shown: Vec<Value> = g.files.iter().take(MAX_FILES_PER_GROUP).map(|f| json!({ "path": f.path, "mtime": f.mtime })).collect();
    json!({ "size": g.size, "count": g.files.len(), "files": shown })
}

#[tauri::command(rename_all = "snake_case")]
pub fn dupes_scan_start(app: AppHandle, state: State<'_, AppState>, roots: Vec<String>, min_size: Option<u64>) -> CmdResult<Value> {
    let roots: Vec<String> = roots.iter().map(|r| r.trim().trim_matches('"').replace('/', "\\")).filter(|r| !r.is_empty()).collect();
    if roots.is_empty() {
        return err("Choose a drive or folder to scan.");
    }
    for r in &roots {
        if !Path::new(r).is_dir() {
            return err(format!("Cannot open {r}. Choose an existing drive or folder."));
        }
    }
    // one heavy scan at a time (shared with the storage scan)
    if crate::storage::SCAN_RUNNING.swap(true, Ordering::SeqCst) {
        return err("A scan is already running.");
    }
    let (job, cancel) = state.jobs.start();
    let jid = job.clone();
    let min_size = min_size.unwrap_or(1_048_576).max(1);
    std::thread::spawn(move || {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                crate::storage::SCAN_RUNNING.store(false, Ordering::SeqCst);
            }
        }
        let _g = Guard;
        let paths: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
        let prog = std::sync::Arc::new(Prog::default());
        let (pp, ap, jj, stop) = (prog.clone(), app.clone(), jid.clone(), std::sync::Arc::new(AtomicBool::new(false)));
        let stop2 = stop.clone();
        let pusher = std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                std::thread::sleep(PROGRESS_EVERY);
                let _ = ap.emit("dupes:progress", progress_json(&jj, &pp));
            }
        });
        let out = find_duplicates(&paths, min_size, &cancel, 3, &prog);
        stop.store(true, Ordering::Relaxed);
        let _ = pusher.join();
        let total_groups = out.groups.len();
        let wasted: u64 = out.groups.iter().map(|g| g.size * (g.files.len() as u64 - 1)).sum();
        let dup_files: usize = out.groups.iter().map(|g| g.files.len()).sum();
        let groups: Vec<Value> = out.groups.iter().take(MAX_GROUPS).map(group_json).collect();
        let _ = app.emit(
            "dupes:done",
            json!({ "job": jid, "cancelled": out.cancelled, "errors": prog.errors.load(Ordering::Relaxed),
                    "skipped_cloud": prog.skipped_cloud.load(Ordering::Relaxed), "files": prog.files.load(Ordering::Relaxed),
                    "total_groups": total_groups, "total_files": dup_files, "wasted": wasted, "groups": groups }),
        );
        crate::activity::log(
            &app,
            "info",
            format!("Duplicate scan {}: {} groups, {} reclaimable", if out.cancelled { "cancelled" } else { "finished" }, total_groups, crate::cleanup::fmt_bytes(wasted)),
        );
        if let Some(st) = app.try_state::<AppState>() {
            st.jobs.finish(&jid);
        }
    });
    Ok(json!({ "job": job }))
}

#[derive(Deserialize, Clone)]
pub struct Item {
    pub path: String,
    pub mtime: u64,
}

#[derive(Deserialize)]
pub struct Group {
    pub size: u64,
    pub keep: Item,
    pub remove: Vec<Item>,
}

fn unchanged(it: &Item, size: u64) -> Result<(), String> {
    let Some(p) = crate::storage::validate_path(&it.path) else { return Err("invalid path".into()) };
    let m = std::fs::symlink_metadata(&p).map_err(|_| "file no longer exists".to_string())?;
    if !m.is_file() {
        return Err("not a regular file".into());
    }
    if m.len() != size || mtime_secs(&m) != it.mtime {
        return Err("changed since the scan - scan again".into());
    }
    Ok(())
}

/// Move the `remove` files of each group to the Recycle Bin - but only when the group still has a verified kept copy.
pub fn recycle_group(g: &Group) -> Vec<Value> {
    let refuse = |why: &str| g.remove.iter().map(|r| json!({ "path": r.path, "ok": false, "message": why })).collect::<Vec<_>>();
    if g.remove.is_empty() {
        return Vec::new();
    }
    let norm = |s: &str| crate::protect::normalize(s);
    let keep_n = norm(&g.keep.path);
    if g.remove.iter().any(|r| norm(&r.path) == keep_n) {
        return refuse("the file to keep is also marked for removal");
    }
    if let Err(e) = unchanged(&g.keep, g.size) {
        return refuse(&format!("the copy to keep is not safe ({e})"));
    }
    // two removals of the same path must not count as two copies
    let mut seen = std::collections::HashSet::new();
    g.remove
        .iter()
        .map(|r| {
            if !seen.insert(norm(&r.path)) {
                return json!({ "path": r.path, "ok": true, "message": "listed twice" });
            }
            if let Err(e) = unchanged(r, g.size) {
                return json!({ "path": r.path, "ok": false, "message": e });
            }
            let (ok, message) = crate::storage::recycle_one(&r.path);
            json!({ "path": r.path, "ok": ok, "message": message })
        })
        .collect()
}

#[tauri::command(rename_all = "snake_case")]
pub async fn dupes_recycle(app: AppHandle, groups: Vec<Group>) -> CmdResult<Value> {
    let rows = tauri::async_runtime::spawn_blocking(move || groups.iter().flat_map(recycle_group).collect::<Vec<_>>())
        .await
        .map_err(|e| crate::util::CmdError(e.to_string()))?;
    let moved = rows.iter().filter(|r| r["ok"] == true && r["message"] == "moved to Recycle Bin").count();
    if moved > 0 {
        crate::activity::log(&app, "ok", format!("Moved {moved} duplicate file(s) to the Recycle Bin"));
    }
    let failed = rows.iter().filter(|r| r["ok"] == false).count();
    if failed > 0 {
        crate::activity::log(&app, "fail", format!("{failed} duplicate file(s) were not removed"));
    }
    Ok(json!({ "results": rows }))
}

// ------------------------------------------------------------------ tests (temp folders only; the Recycle Bin is never touched)

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pk_dupes_test_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn mk(root: &Path, rel: &str, data: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }
    fn scan(root: &Path, min: u64) -> Outcome {
        find_duplicates(&[root.to_path_buf()], min, &AtomicBool::new(false), 3, &Prog::default())
    }

    #[test]
    fn finds_identical_files_only() {
        let root = tmp("find");
        let a = vec![7u8; 200_000];
        let mut b = a.clone();
        *b.last_mut().unwrap() = 8; // same size and same first 64 KB, different tail
        mk(&root, "one\\a.bin", &a);
        mk(&root, "two\\a copy.bin", &a);
        mk(&root, "three\\other.bin", &b);
        mk(&root, "three\\small.txt", b"hi");
        mk(&root, "four\\small.txt", b"hi"); // below min size
        let o = scan(&root, 1000);
        assert!(!o.cancelled);
        assert_eq!(o.groups.len(), 1);
        assert_eq!(o.groups[0].files.len(), 2);
        assert_eq!(o.groups[0].size, 200_000);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn small_files_and_biggest_waste_first() {
        let root = tmp("order");
        for d in ["a", "b", "c"] {
            mk(&root, &format!("{d}\\x.bin"), &[1u8; 5000]);
        }
        for d in ["a", "b"] {
            mk(&root, &format!("{d}\\y.bin"), &[2u8; 20_000]);
        }
        let o = scan(&root, 1);
        assert_eq!(o.groups.len(), 2);
        assert_eq!(o.groups[0].size, 20_000); // waste 20k vs 10k
        assert_eq!(o.groups[1].files.len(), 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn hard_links_are_not_duplicates() {
        let root = tmp("hl");
        mk(&root, "a\\f.bin", &[3u8; 4000]);
        let ok = fs::hard_link(root.join("a\\f.bin"), root.join("a\\link.bin")).is_ok();
        let o = scan(&root, 1);
        if ok {
            assert!(o.groups.is_empty(), "a hard link must not be offered for deletion");
        }
        mk(&root, "b\\real copy.bin", &[3u8; 4000]);
        assert_eq!(scan(&root, 1).groups.len(), 1);
        assert_eq!(scan(&root, 1).groups[0].files.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cloud_attributes_are_skipped() {
        assert!(is_cloud_placeholder(0x40_0000)); // RECALL_ON_DATA_ACCESS
        assert!(is_cloud_placeholder(0x4_0000 | 0x20)); // RECALL_ON_OPEN
        assert!(is_cloud_placeholder(0x1000)); // OFFLINE
        assert!(!is_cloud_placeholder(0x20 | 0x400)); // archive + reparse point (hydrated files stay scannable)
    }

    #[test]
    fn cancel_returns_nothing() {
        let root = tmp("cancel");
        mk(&root, "a\\x.bin", &[1u8; 3000]);
        mk(&root, "b\\x.bin", &[1u8; 3000]);
        let o = find_duplicates(&[root.clone()], 1, &AtomicBool::new(true), 2, &Prog::default());
        assert!(o.cancelled && o.groups.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    fn item(root: &Path, rel: &str) -> Item {
        let p = root.join(rel);
        Item { path: p.to_string_lossy().into_owned(), mtime: mtime_secs(&fs::metadata(&p).unwrap()) }
    }

    #[test]
    fn backend_refuses_to_remove_the_last_copy() {
        let root = tmp("keep");
        mk(&root, "a.bin", &[9u8; 3000]);
        mk(&root, "b.bin", &[9u8; 3000]);
        // keep == remove
        let g = Group { size: 3000, keep: item(&root, "a.bin"), remove: vec![item(&root, "a.bin")] };
        assert!(recycle_group(&g).iter().all(|r| r["ok"] == false));
        // keep file vanished
        let mut keep = item(&root, "b.bin");
        keep.path = root.join("gone.bin").to_string_lossy().into_owned();
        let g = Group { size: 3000, keep, remove: vec![item(&root, "a.bin")] };
        assert!(recycle_group(&g).iter().all(|r| r["ok"] == false));
        // keep file changed size since the scan
        let g = Group { size: 3001, keep: item(&root, "b.bin"), remove: vec![item(&root, "a.bin")] };
        assert!(recycle_group(&g).iter().all(|r| r["ok"] == false));
        // keep mtime changed
        let mut k = item(&root, "b.bin");
        k.mtime += 5;
        let g = Group { size: 3000, keep: k, remove: vec![item(&root, "a.bin")] };
        assert!(recycle_group(&g).iter().all(|r| r["ok"] == false));
        assert!(root.join("a.bin").exists() && root.join("b.bin").exists());
        // the file marked for removal changed since the scan
        let mut r = item(&root, "a.bin");
        r.mtime += 5;
        let g = Group { size: 3000, keep: item(&root, "b.bin"), remove: vec![r] };
        assert!(recycle_group(&g).iter().all(|r| r["ok"] == false));
        assert!(root.join("a.bin").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
