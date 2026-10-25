// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Applications page backend (contract: docs/api/applications.md). Behaviour and safety rules are ported from core.py:
//! list_apps / find_related / find_leftovers / run_uninstall / remove_uninstall_entry / block_edge_reinstall.
//!
//! Safety: every id coming from the UI is resolved against what this module itself produced (app ids from the registry
//! scan, item ids re-derived from a fresh snapshot); protected processes, protected folder names, the Windows folder,
//! and WebView2 / EdgeUpdate folders are refused here, whatever the UI sends.
#![allow(dead_code)]

mod launch;
mod model;
mod parse;
mod system;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::protect;
use crate::state::AppState;
use crate::util::{self, err, CmdResult};
use model::*;
use parse::*;
use system::*;

static APPS: Mutex<Option<HashMap<String, AppInfo>>> = Mutex::new(None);

/// Remember every app seen (never forgotten, so leftovers still resolve after the app left the registry).
fn remember(apps: &[AppInfo]) {
    let mut g = APPS.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    for a in apps {
        m.insert(a.id.clone(), a.clone());
    }
}

fn resolve_app(id: &str) -> CmdResult<AppInfo> {
    if let Some(a) = APPS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(id)) {
        return Ok(a.clone());
    }
    let apps = scan_all();
    remember(&apps);
    match apps.into_iter().find(|a| a.id == id) {
        Some(a) => Ok(a),
        None => err("That application is no longer in the list. Refresh and try again."),
    }
}

// ------------------------------------------------------------------ items

#[derive(Clone, Debug)]
pub enum Act {
    Procs(Vec<u32>),
    Service(String),
    Startup(Startup),
    Task(String),
    Folder(String),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub detail: String,
    pub state: String,
    pub by_path: bool,
    pub protected: bool,
    pub label: &'static str,
    pub count: Option<usize>,
    pub mem: Option<u64>,
    pub default_on: bool,
    pub act: Act,
}

impl Item {
    fn to_json(&self, leftover: bool) -> Value {
        let mut v = json!({
            "id": self.id, "kind": self.kind, "name": self.name, "detail": self.detail, "state": self.state,
            "reason": if self.by_path { "path" } else { "name" }, "protected": self.protected, "action_label": self.label,
        });
        if let Some(o) = v.as_object_mut() {
            if let Some(c) = self.count {
                o.insert("count".into(), json!(c));
            }
            if let Some(m) = self.mem {
                o.insert("mem".into(), json!(m));
            }
            if leftover {
                o.insert("default_on".into(), json!(self.default_on));
            }
        }
        v
    }
}

fn is_webview_text(s: &str) -> bool {
    let l = s.to_lowercase();
    l.contains("msedgewebview2") || l.contains("\\edgewebview\\") || l.contains("\\edgewebview2")
}

fn label_for(kind: &str, clean: bool) -> &'static str {
    match (kind, clean) {
        ("process", _) => "End process",
        ("service", false) => "Stop service",
        ("service", true) => "Delete service",
        ("startup", _) => "Remove entry",
        ("task", _) => "Delete task",
        _ => "Recycle Bin",
    }
}

/// Port of core.find_related. `clean` only changes the action labels (PRE_LABELS vs CLEAN_LABELS).
pub fn find_related(app: &AppInfo, snap: &Snapshot, clean: bool) -> Vec<Item> {
    let loc = safe_location(&app.location);
    let core = core_name(&app.name).to_lowercase();
    let by_path = |text: &str| loc.as_ref().map_or(false, |l| text.to_lowercase().replace('"', "").contains(l.as_str()));
    let by_name = |texts: &[&str]| !core.is_empty() && texts.iter().any(|t| t.to_lowercase().contains(&core));

    let mut out: Vec<Item> = Vec::new();
    let mut groups: HashMap<String, usize> = HashMap::new();
    for p in &snap.procs {
        let exe = p.exe.to_lowercase();
        let path = loc.as_ref().map_or(false, |l| exe.starts_with(&format!("{l}\\")));
        if !path && !by_name(&[&p.name, &p.exe]) {
            continue;
        }
        let prot = protect::is_protected_process(&p.name, &p.exe) || is_webview_text(&p.name) || is_webview_text(&p.exe);
        let key = p.name.to_lowercase();
        if let Some(&i) = groups.get(&key) {
            let it = &mut out[i];
            it.count = Some(it.count.unwrap_or(0) + 1);
            it.mem = Some(it.mem.unwrap_or(0) + p.mem);
            it.by_path |= path;
            it.protected |= prot;
            if let Act::Procs(v) = &mut it.act {
                v.push(p.pid);
            }
        } else {
            groups.insert(key.clone(), out.len());
            out.push(Item {
                id: format!("p:{key}"),
                kind: "process",
                name: p.name.clone(),
                detail: String::new(),
                state: "Running".into(),
                by_path: path,
                protected: prot,
                label: label_for("process", clean),
                count: Some(1),
                mem: Some(p.mem),
                default_on: false,
                act: Act::Procs(vec![p.pid]),
            });
        }
    }
    for s in &snap.services {
        let path = by_path(&s.path);
        if !path && !by_name(&[&s.name, &s.display]) {
            continue;
        }
        // Services whose binary lives in the Windows folder (svchost-hosted etc.) are never offered for action.
        let prot = protect::is_protected_file(&s.path.trim_matches('"').replace('/', "\\"))
            || s.path.to_lowercase().trim_start_matches('"').starts_with(&format!("{}\\", util::win_dir()))
            || is_webview_text(&s.path);
        out.push(Item {
            id: format!("s:{}", s.name),
            kind: "service",
            name: if s.display.is_empty() { s.name.clone() } else { s.display.clone() },
            detail: format!("Service \u{b7} {}", start_type_label(s.start)),
            state: state_label(s.state).into(),
            by_path: path,
            protected: prot,
            label: label_for("service", clean),
            count: None,
            mem: None,
            default_on: false,
            act: Act::Service(s.name.clone()),
        });
    }
    for e in &snap.startup {
        let path = by_path(&e.cmd);
        if !path && !by_name(&[&e.name, &e.cmd]) {
            continue;
        }
        out.push(Item {
            id: format!("u:{}:{}", e.tag(), e.name),
            kind: "startup",
            name: e.name.clone(),
            detail: format!("Startup entry \u{b7} {}", e.label()),
            state: "Enabled".into(),
            by_path: path,
            protected: false,
            label: label_for("startup", clean),
            count: None,
            mem: None,
            default_on: false,
            act: Act::Startup(e.clone()),
        });
    }
    for t in &snap.tasks {
        let path = by_path(&t.cmd);
        if !path && !by_name(&[&t.name, &t.cmd]) {
            continue;
        }
        out.push(Item {
            id: format!("t:{}", t.name),
            kind: "task",
            name: t.name.clone(),
            detail: if t.cmd.is_empty() { "Scheduled task".into() } else { t.cmd.clone() },
            state: t.status.clone(),
            by_path: path,
            protected: false,
            label: label_for("task", clean),
            count: None,
            mem: None,
            default_on: false,
            act: Act::Task(t.name.clone()),
        });
    }
    let order = |k: &str| match k {
        "process" => 0,
        "service" => 1,
        "startup" => 2,
        "task" => 3,
        _ => 4,
    };
    out.sort_by(|a, b| (!a.by_path, order(a.kind), a.name.to_lowercase()).cmp(&(!b.by_path, order(b.kind), b.name.to_lowercase())));
    for it in &mut out {
        it.default_on = it.by_path;
    }
    out
}

// ------------------------------------------------------------------ folders (leftovers)

/// Folder names that belong to WebView2 / the Edge updater: never offered, never deleted.
fn is_webview_folder(path: &str) -> bool {
    path.to_lowercase().replace('/', "\\").split('\\').any(|c| matches!(c, "edgewebview" | "edgeupdate" | "edgecore") || c.starts_with("edgewebview"))
}

/// Components that mean "this belongs to Windows / is shared": a folder below one of these is never ours to delete.
const PROTECTED_ANCESTORS: &[&str] =
    &["windowsapps", "modifiablewindowsapps", "windows", "windows nt", "windows defender", "common files", "microsoft shared", "internet explorer", "reference assemblies"];

/// Profile sub-folders that hold the user's own data: never deleted, whatever an `InstallLocation` claims.
const PROFILE_FOLDERS: &[&str] = &[
    "documents", "desktop", "downloads", "pictures", "music", "videos", "onedrive", "contacts", "favorites", "links", "saved games", "searches", "appdata", "3d objects",
    "my documents", "my music", "my pictures", "my videos", "application data", "local settings",
];

fn trim_component(c: &str) -> &str {
    if c == "." || c == ".." {
        c
    } else {
        c.trim_end_matches(['.', ' '])
    }
}

/// Lower-case, backslash-separated, trailing dots/spaces removed from every component (Win32 ignores them, so
/// `C:\Windows.` IS `C:\Windows`), no trailing slash.
fn norm_cmp(p: &str) -> String {
    p.replace('/', "\\").split('\\').map(trim_component).collect::<Vec<_>>().join("\\").trim_end_matches('\\').to_lowercase()
}

/// The text-level rules, applied to every spelling of the path (as given, long name, canonical target).
fn check_spelling(p: &str) -> Result<(), String> {
    let n = norm_cmp(p);
    let comps: Vec<&str> = n.split('\\').collect();
    if comps.len() < 3 {
        return Err("Refused: not a specific folder".into());
    }
    if comps.iter().any(|c| *c == "..") {
        return Err("Refused: unsafe path".into());
    }
    let leaf = comps[comps.len() - 1];
    if protect::is_protected_dir_name(leaf) {
        return Err("Protected folder".into());
    }
    if comps[..comps.len() - 1].iter().any(|c| PROTECTED_ANCESTORS.contains(c)) {
        return Err("Protected: part of Windows or shared components".into());
    }
    let win = util::win_dir();
    if n == win || protect::is_protected_file(&n) {
        return Err("Protected: inside the Windows folder".into());
    }
    if is_webview_folder(&n) {
        return Err("Protected: WebView2 / Edge updater files are never touched".into());
    }
    for k in [
        "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData", "LOCALAPPDATA", "APPDATA", "USERPROFILE", "SystemDrive", "PUBLIC", "ALLUSERSPROFILE",
        "HOMEDRIVE", "CommonProgramFiles", "CommonProgramFiles(x86)", "TEMP", "TMP",
    ] {
        if let Ok(v) = std::env::var(k) {
            let v = norm_cmp(&v);
            if !v.is_empty() && v == n {
                return Err("Refused: that is a system folder".into());
            }
        }
    }
    // <drive>\Users\<anything> is a user profile root (including Public / Default), and its well-known sub-folders hold personal data
    if comps.len() == 3 && comps[1] == "users" {
        return Err("Refused: that is a user profile".into());
    }
    if comps.len() == 4 && comps[1] == "users" && PROFILE_FOLDERS.contains(&leaf) {
        return Err("Refused: that is a personal folder".into());
    }
    if let Ok(up) = std::env::var("USERPROFILE") {
        let up = norm_cmp(&up);
        if let Some(rest) = n.strip_prefix(&format!("{up}\\")) {
            if !rest.contains('\\') && PROFILE_FOLDERS.contains(&rest) {
                return Err("Refused: that is a personal folder".into());
            }
        }
    }
    Ok(())
}

fn is_reparse(p: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::symlink_metadata(p).map(|m| m.file_attributes() & 0x400 != 0).unwrap_or(false)
}

/// Last line of defence before a folder goes to the Recycle Bin.
pub fn folder_allowed(path: &str) -> Result<(), String> {
    let p = path.trim().trim_end_matches(['\\', '/']);
    if !is_abs(p) || p.len() < 4 || p.matches('\\').count() + p.matches('/').count() < 2 {
        return Err("Refused: not a specific folder".into());
    }
    if p.contains("..") {
        return Err("Refused: unsafe path".into());
    }
    check_spelling(p)?;
    if Path::new(p).exists() {
        // junctions / symlinks: a harmless-looking folder can point INTO a protected one, and 8.3 names hide the real name
        if is_reparse(p) {
            return Err("Refused: it is a link (junction or symbolic link)".into());
        }
        let long = long_path(p);
        if let Some(l) = &long {
            check_spelling(l)?;
        }
        if let Ok(canon) = std::fs::canonicalize(p) {
            let c = canon.to_string_lossy().into_owned();
            let c = c.strip_prefix(r"\\?\").unwrap_or(&c).to_string();
            check_spelling(&c)?;
            let same = long.map(|l| norm_cmp(&l)).unwrap_or_else(|| norm_cmp(p)) == norm_cmp(&c);
            if !same {
                return Err("Refused: the path goes through a link".into());
            }
        }
    }
    Ok(())
}

fn folder_item(path: &str, name: &str, detail: &str, extra_state: &str, by_path: bool, default_on: bool, size: bool) -> Item {
    let state = if size {
        let (b, capped) = dir_size(path);
        let sz = fmt_bytes(b);
        let mut s = if sz.is_empty() { "Folder".to_string() } else { format!("Folder \u{b7} {sz}{}", if capped { "+" } else { "" }) };
        if !extra_state.is_empty() {
            s.push_str(" \u{b7} ");
            s.push_str(extra_state);
        }
        s
    } else {
        "Folder".into()
    };
    Item {
        id: format!("f:{}", path.to_lowercase()),
        kind: "folder",
        name: name.into(),
        detail: detail.into(),
        state,
        by_path,
        protected: false,
        label: label_for("folder", true),
        count: None,
        mem: None,
        default_on,
        act: Act::Folder(path.into()),
    }
}

/// Port of core.find_leftovers: related things + install folder + name-matched folders in the usual roots.
pub fn find_leftovers_for(app: &AppInfo, snap: &Snapshot, sizes: bool) -> Vec<Item> {
    let mut items: Vec<Item> = find_related(app, snap, true).into_iter().filter(|r| !r.protected).collect();
    let core = core_name(&app.name).to_lowercase();
    let loc = safe_location(&app.location);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let edge = app.is_edge();
    if let Some(l) = &loc {
        if Path::new(&app.location.trim().trim_matches('"')).is_dir() && !edge {
            let path = app.location.trim().trim_matches('"').trim_end_matches('\\').to_string();
            if folder_allowed(&path).is_ok() {
                seen.insert(l.clone());
                let bn = path.rsplit('\\').next().unwrap_or(&path).to_string();
                items.push(folder_item(&path, &bn, &path, "", true, true, sizes));
            }
        }
    }
    if edge {
        // Edge's own folder, and (unticked) the user's profile data. EdgeUpdate / EdgeWebView are deliberately NOT offered.
        let pf = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
        let p = format!("{pf}\\Microsoft\\Edge");
        if Path::new(&p).is_dir() && folder_allowed(&p).is_ok() && seen.insert(p.to_lowercase()) {
            items.push(folder_item(&p, "Edge", &p, "", true, true, sizes));
        }
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            let ud = format!("{la}\\Microsoft\\Edge\\User Data");
            if Path::new(&ud).is_dir() && folder_allowed(&ud).is_ok() && seen.insert(ud.to_lowercase()) {
                items.push(folder_item(&ud, "Edge User Data", &ud, "your profile, history and passwords", false, false, sizes));
            }
        }
    }
    if !core.is_empty() {
        for root in env_roots() {
            let Ok(rd) = std::fs::read_dir(&root) else { continue };
            for e in rd.flatten() {
                if !e.file_type().map(|t| t.is_dir() && !t.is_symlink()).unwrap_or(false) {
                    continue;
                }
                let fname = e.file_name().to_string_lossy().into_owned();
                let path = e.path().to_string_lossy().into_owned();
                if seen.contains(&path.to_lowercase()) || !fname.to_lowercase().contains(&core) {
                    continue;
                }
                if protect::is_protected_dir_name(&fname) || folder_allowed(&path).is_err() {
                    continue;
                }
                if edge && path.to_lowercase().contains("\\microsoft\\edge") {
                    continue; // handled above
                }
                seen.insert(path.to_lowercase());
                let rootname = Path::new(&root).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.clone());
                let exact = fname.to_lowercase() == core;
                items.push(folder_item(&path, &fname, &path, "", exact, exact, sizes));
                let _ = rootname;
            }
        }
    }
    // folders last by default order, keep path matches first
    items.sort_by_key(|i| (!i.by_path, if i.kind == "folder" { 4 } else { 0 }));
    items
}

// ------------------------------------------------------------------ actions

fn own_tree(all: &[Proc]) -> Vec<u32> {
    descendants(all, std::process::id())
}

/// End every (non-protected) process of the item, children included. Returns a one-line summary.
fn act_kill(pids: &[u32], all: &[Proc]) -> Result<String, String> {
    let own = own_tree(all);
    let mut targets: Vec<u32> = Vec::new();
    for &pid in pids {
        for d in descendants(all, pid) {
            if !targets.contains(&d) {
                targets.push(d);
            }
        }
    }
    let mut ended = 0;
    let mut last_err = None;
    // children first, roots last
    targets.reverse();
    for pid in targets {
        if own.contains(&pid) || pid == std::process::id() {
            continue;
        }
        if let Some(p) = all.iter().find(|p| p.pid == pid) {
            if protect::is_protected_process(&p.name, &p.exe) || is_webview_text(&p.name) || is_webview_text(&p.exe) {
                continue;
            }
        }
        match kill_pid(pid) {
            Ok(true) => ended += 1,
            Ok(false) => {}
            Err(e) => last_err = Some(e),
        }
    }
    match (ended, last_err) {
        (0, Some(e)) => Err(e),
        (0, None) => Ok("already gone".into()),
        (n, _) => Ok(format!("ended {n} process(es)")),
    }
}

/// Run the action for an item. `all` = process snapshot.
fn perform(item: &Item, all: &[Proc]) -> Result<String, String> {
    if item.protected {
        return Err("protected item".into());
    }
    let r = match &item.act {
        Act::Procs(p) => act_kill(p, all),
        Act::Service(n) => svc_stop(n, Duration::from_secs(20)),
        Act::Startup(e) => startup_remove(e),
        Act::Task(n) => task_delete(n),
        Act::Folder(p) => {
            folder_allowed(p)?;
            recycle(p)
        }
    };
    if !matches!(item.act, Act::Procs(_)) {
        invalidate_caches();
    }
    r
}

fn perform_clean(item: &Item, all: &[Proc]) -> Result<String, String> {
    match &item.act {
        Act::Service(n) => {
            if item.protected {
                return Err("protected item".into());
            }
            svc_delete(n)
        }
        Act::Procs(p) => {
            if item.protected {
                return Err("protected item".into());
            }
            act_kill(p, all).map(|m| capitalize(&m))
        }
        Act::Startup(e) => startup_remove(e).map(|m| capitalize(&m)),
        Act::Task(n) => task_delete(n),
        Act::Folder(p) => {
            folder_allowed(p)?;
            recycle(p)
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ------------------------------------------------------------------ uninstall

pub struct UninstallOutcome {
    pub ok: bool,
    pub message: String,
    pub needs_restart: bool,
}

/// Port of core.run_uninstall. `say(level, text)` receives live-log lines.
pub fn run_uninstall(app: &AppInfo, silent: bool, say: &dyn Fn(&str, &str)) -> UninstallOutcome {
    let fail = |m: String| UninstallOutcome { ok: false, message: m, needs_restart: false };
    if app.uninstaller_missing() {
        return fail(format!("The uninstaller is missing: {}", exe_of(&app.uninstall)));
    }
    let mut cmd = if silent && !app.quiet.is_empty() { app.quiet.clone() } else { app.uninstall.clone() };
    if app.is_edge() {
        if !util::is_admin() {
            return fail("Removing Microsoft Edge needs Administrator rights.".into());
        }
        if !cmd.to_lowercase().contains("--force-uninstall") {
            cmd.push_str(" --force-uninstall"); // Windows blocks the plain command for Edge
        }
    }
    let cmd = rewrite_msi(&cmd, silent);
    say("info", if silent { "Starting the silent uninstaller\u{2026}" } else { "Starting the uninstaller\u{2026}" });
    say("info", "Waiting for the uninstaller to finish\u{2026}");
    // started directly (never through cmd.exe / a shell), ended with its children if it hangs for an hour
    let rc = match launch::run(&cmd, Duration::from_secs(3600)) {
        Ok(rc) => rc,
        Err(e) => return fail(e),
    };
    std::thread::sleep(Duration::from_secs(2));
    if rc == 1602 || rc == 1223 {
        return fail("The uninstall was cancelled.".into());
    }
    let restart = rc == 1641 || rc == 3010;
    // some uninstallers hand over to a helper process and exit at once: give the registry entry a moment to vanish
    let ok_codes = rc == 0 || restart;
    let mut waited = 0;
    while app.installed() && ok_codes && waited < 20 {
        std::thread::sleep(Duration::from_millis(500));
        waited += 1;
    }
    if !app.installed() {
        return UninstallOutcome { ok: true, message: format!("{} was uninstalled.", app.name), needs_restart: restart };
    }
    if ok_codes {
        return UninstallOutcome { ok: true, message: "The uninstaller finished (the app may still be closing).".into(), needs_restart: restart };
    }
    fail(format!("The uninstaller exited with code {rc}."))
}

fn local_stamp() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

fn emit_progress(app: &AppHandle, job: &str, level: &str, text: &str) {
    let _ = app.emit("uninstall:progress", json!({ "job": job, "level": level, "text": text }));
}

fn uninstall_job(app: AppHandle, job: String, a: AppInfo, stop_ids: Vec<String>, silent: bool, block_edge: bool) {
    let say = |level: &str, text: &str| emit_progress(&app, &job, level, text);
    let done = |ok: bool, message: &str, restart: bool| {
        let _ = app.emit("uninstall:done", json!({ "job": job, "ok": ok, "message": message, "needs_restart": restart }));
        app.state::<AppState>().jobs.finish(&job);
    };

    // 1) stop the ticked items (ids re-derived from a live snapshot; unknown / protected ids are skipped)
    if !stop_ids.is_empty() {
        let snap = snapshot(false);
        let items = find_related(&a, &snap, false);
        for id in &stop_ids {
            let Some(it) = items.iter().find(|i| &i.id == id) else { continue };
            if it.protected {
                say("info", &format!("{} \u{201c}{}\u{201d} \u{2014} skipped (protected)", kind_title(it.kind), it.name));
                continue;
            }
            match perform(it, &snap.procs) {
                Ok(m) => say("ok", &format!("{} \u{201c}{}\u{201d} \u{2014} {}", kind_title(it.kind), it.name, m)),
                Err(e) => say("fail", &format!("{} \u{201c}{}\u{201d} \u{2014} {}", kind_title(it.kind), it.name, e)),
            }
        }
        invalidate_caches();
    }

    // 2) broken entry: back up + remove the key
    if a.uninstaller_missing() {
        say("info", "Removing the dead entry from Installed apps\u{2026}");
        let dir = util::data_dir().join("backups");
        match remove_uninstall_entry_in(&a, &dir, &local_stamp()) {
            Ok(backup) => {
                say("ok", &format!("Backup saved to {backup}"));
                say("ok", "Removed the dead entry.");
                crate::activity::log(&app, "ok", format!("Removed the dead entry of {} (backup: {backup})", a.name));
                done(true, "Removed the dead entry.", false);
            }
            Err(e) => {
                let m = format!("Could not remove the entry: {e}");
                say("fail", &m);
                crate::activity::log(&app, "fail", format!("{}: {m}", a.name));
                done(false, &m, false);
            }
        }
        return;
    }

    // 3) run the uninstaller
    let out = run_uninstall(&a, silent, &say);
    if !out.ok {
        say("fail", &out.message);
        crate::activity::log(&app, "fail", format!("Uninstall of {} failed: {}", a.name, out.message));
        done(false, &out.message, out.needs_restart);
        return;
    }
    say("ok", &out.message);
    crate::activity::log(&app, "ok", format!("Uninstalled {}", a.name));
    if a.is_edge() && block_edge {
        match block_edge_reinstall() {
            Ok(m) => {
                say("ok", &format!("Edge updater told not to install Edge again ({m})"));
                crate::activity::log(&app, "ok", "Blocked Microsoft Edge reinstall (EdgeUpdate policy)");
            }
            Err(e) => say("fail", &format!("Could not block the Edge reinstall: {e}")),
        }
    }
    invalidate_caches();
    done(true, &out.message, out.needs_restart);
}

fn kind_title(k: &str) -> &'static str {
    match k {
        "process" => "Process",
        "service" => "Service",
        "startup" => "Startup entry",
        "task" => "Scheduled task",
        _ => "Folder",
    }
}

// ------------------------------------------------------------------ commands

#[tauri::command(rename_all = "snake_case")]
pub async fn list_apps() -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(|| {
        let apps = scan_all();
        remember(&apps);
        Ok(json!({ "apps": apps.iter().map(|a| a.to_json()).collect::<Vec<_>>() }))
    })
    .await?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_related(app_id: String) -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let a = resolve_app(&app_id)?;
        let items = find_related(&a, &snapshot(false), false);
        Ok(json!({ "items": items.iter().map(|i| i.to_json(false)).collect::<Vec<_>>() }))
    })
    .await?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn other_browsers() -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(|| Ok(json!({ "names": browsers_in(&scan_all()) }))).await?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn uninstall_start(
    app: AppHandle,
    state: State<'_, AppState>,
    app_id: String,
    stop_ids: Vec<String>,
    silent: bool,
    block_edge_reinstall: bool,
) -> CmdResult<Value> {
    let a = tauri::async_runtime::spawn_blocking({
        let app_id = app_id.clone();
        move || -> CmdResult<AppInfo> {
            let a = resolve_app(&app_id)?;
            let lname = a.name.to_lowercase();
            if lname.contains("webview") {
                return err("WebView2 is used by other apps and is never removed by Task Reaper.");
            }
            if lname.contains("edge update") || lname.contains("edgeupdate") {
                return err("The Microsoft Edge updater is shared with WebView2 and is never removed by Task Reaper.");
            }
            if !a.installed() {
                return err("This application is no longer installed. Refresh the list.");
            }
            if a.is_edge() {
                if !util::is_admin() {
                    return err("Removing Microsoft Edge needs Administrator rights.");
                }
                if browsers_in(&scan_all()).is_empty() {
                    return err("No other web browser was found. Install one (Chrome, Brave, Firefox...) before removing Microsoft Edge.");
                }
            }
            Ok(a)
        }
    })
    .await??;
    let (job, _cancel) = state.jobs.start();
    let j = job.clone();
    let handle = app.clone();
    let silent = silent && !a.quiet.is_empty() && !a.is_edge() && !a.uninstaller_missing();
    std::thread::spawn(move || uninstall_job(handle, j, a, stop_ids, silent, block_edge_reinstall));
    Ok(json!({ "job": job }))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn find_leftovers(app_id: String) -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let a = resolve_app(&app_id)?;
        let items = find_leftovers_for(&a, &snapshot(true), true);
        Ok(json!({ "items": items.iter().map(|i| i.to_json(true)).collect::<Vec<_>>() }))
    })
    .await?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn clean_leftovers(app: AppHandle, app_id: String, ids: Vec<String>) -> CmdResult<Value> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let a = resolve_app(&app_id)?;
        let snap = snapshot(true);
        let items = find_leftovers_for(&a, &snap, false);
        let mut results = Vec::new();
        let (mut okc, mut failc) = (0, 0);
        for id in ids {
            let found = items.iter().find(|i| i.id == id);
            let r = match found {
                Some(it) => perform_clean(it, &snap.procs),
                None => {
                    // a folder that is already gone no longer shows up: harmless; anything else is not ours to touch
                    match id.strip_prefix("f:") {
                        Some(p) if !Path::new(p).exists() => Ok("Already gone".to_string()),
                        _ => Err("Not recognised (it may already be gone)".to_string()),
                    }
                }
            };
            match r {
                Ok(m) => {
                    okc += 1;
                    results.push(json!({ "id": id, "ok": true, "message": m }));
                }
                Err(m) => {
                    failc += 1;
                    results.push(json!({ "id": id, "ok": false, "message": m }));
                }
            }
        }
        invalidate_caches();
        crate::activity::log(
            &handle,
            if failc == 0 { "ok" } else { "info" },
            format!("Cleaned leftovers of {}: {okc} removed, {failc} failed", a.name),
        );
        Ok(json!({ "results": results }))
    })
    .await?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_folder(path: String) -> CmdResult<()> {
    let p = path.trim().trim_matches('"').to_string();
    if !is_abs(&p) || !Path::new(&p).is_dir() {
        return err("That folder does not exist.");
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _ = std::process::Command::new("explorer.exe").arg(&p).spawn();
    })
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
