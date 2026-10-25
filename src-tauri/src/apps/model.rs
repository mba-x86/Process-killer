// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Installed applications: registry discovery + the guarded registry operations (core.py: list_apps, is_edge,
//! uninstaller_missing, block_edge_reinstall, remove_uninstall_entry).
use std::path::Path;

use winreg::enums::*;
use winreg::RegKey;

use super::parse::*;

pub const EDGE_STABLE_GUID: &str = "{56EB18F8-B008-4CBD-B6D2-8C97FE7E9062}";
pub const EDGE_POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\EdgeUpdate";
const BROWSER_WORDS: &[&str] = &["chrome", "brave", "firefox", "opera", "vivaldi"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hive {
    Hklm,
    Hkcu,
}
impl Hive {
    pub fn name(self) -> &'static str {
        match self {
            Hive::Hklm => "HKLM",
            Hive::Hkcu => "HKCU",
        }
    }
    pub fn key(self) -> RegKey {
        RegKey::predef(match self {
            Hive::Hklm => HKEY_LOCAL_MACHINE,
            Hive::Hkcu => HKEY_CURRENT_USER,
        })
    }
}

pub const UNINSTALL_ROOTS: &[(Hive, &str)] = &[
    (Hive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    (Hive::Hklm, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    (Hive::Hkcu, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
];

#[derive(Clone, Debug)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub size_kb: u64,
    pub date: String,
    pub location: String,
    pub uninstall: String,
    pub quiet: String,
    pub hive: Hive,
    pub regpath: String,
    pub no_remove: bool,
}

impl AppInfo {
    pub fn is_edge(&self) -> bool {
        self.name.trim().to_lowercase() == "microsoft edge" && self.uninstall.to_lowercase().contains("--msedge")
    }
    pub fn uninstaller_missing(&self) -> bool {
        let exe = exe_of(&self.uninstall);
        if exe.is_empty() || is_launcher_exe(&exe) {
            return false;
        }
        is_abs(&exe) && !Path::new(&exe).exists()
    }
    pub fn drive(&self) -> String {
        let b = self.location.as_bytes();
        if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
            format!("{}:\\", (b[0] as char).to_ascii_uppercase())
        } else {
            String::new()
        }
    }
    pub fn installed(&self) -> bool {
        self.hive.key().open_subkey_with_flags(&self.regpath, KEY_READ | KEY_WOW64_64KEY).is_ok()
    }
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id, "name": self.name, "publisher": self.publisher, "version": self.version,
            "size": self.size_kb.saturating_mul(1024), "date": self.date, "location": self.location, "drive": self.drive(),
            "uninstaller_missing": self.uninstaller_missing(), "is_edge": self.is_edge(),
            "can_silent": !self.quiet.is_empty(),
        })
    }
}

/// Stable opaque id from hive + registry path (FNV-1a).
pub fn make_id(hive: Hive, regpath: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in format!("{}|{}", hive.name(), regpath.to_lowercase()).bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("app{:012x}", h & 0xFFFF_FFFF_FFFF)
}

fn s(k: &RegKey, name: &str) -> String {
    k.get_value::<String, _>(name).unwrap_or_default()
}
fn n(k: &RegKey, name: &str) -> Option<u64> {
    k.get_value::<u32, _>(name).map(|v| v as u64).or_else(|_| k.get_value::<u64, _>(name)).ok()
}
fn has(k: &RegKey, name: &str) -> bool {
    k.get_raw_value(name).is_ok()
}

fn infer_location(install_location: &str, display_icon: &str, uninstall: &str) -> String {
    let loc = install_location.trim().trim_matches('"').to_string();
    if !loc.is_empty() {
        return loc;
    }
    let icon = display_icon.split(',').next().unwrap_or("").trim().trim_matches('"').to_string();
    let ext = Path::new(&icon).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !icon.is_empty() && (ext == "exe" || ext == "ico") {
        let d = dirname(&icon);
        if safe_location(&d).is_some() {
            return d;
        }
    }
    let exe = exe_of(uninstall);
    if !exe.is_empty() {
        let l = exe.to_lowercase();
        if !(l.contains("msiexec") || l.contains("rundll32")) {
            let d = dirname(&exe);
            if safe_location(&d).is_some() {
                return d;
            }
        }
    }
    String::new()
}

/// Scan the given uninstall roots (testable with an HKCU test key).
pub fn scan_roots(roots: &[(Hive, &str)]) -> Vec<AppInfo> {
    let mut apps: Vec<AppInfo> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (hive, root) in roots {
        let Ok(k) = hive.key().open_subkey_with_flags(root, KEY_READ | KEY_WOW64_64KEY) else { continue };
        for sub in k.enum_keys().flatten() {
            let Ok(sk) = k.open_subkey_with_flags(&sub, KEY_READ) else { continue };
            let name = clean_name(&s(&sk, "DisplayName"));
            let uninstall_s = s(&sk, "UninstallString");
            let quiet = s(&sk, "QuietUninstallString");
            let cmd = if !uninstall_s.is_empty() { uninstall_s.clone() } else { quiet.clone() };
            let edge = name.to_lowercase() == "microsoft edge" && cmd.to_lowercase().contains("--msedge");
            let no_remove = n(&sk, "NoRemove") == Some(1);
            let rel = s(&sk, "ReleaseType");
            let lname = name.to_lowercase();
            let is_kb = lname.strip_prefix("kb").map_or(false, |r| r.starts_with(|c: char| c.is_ascii_digit()));
            if name.is_empty()
                || cmd.is_empty()
                || n(&sk, "SystemComponent") == Some(1)
                || (no_remove && !edge)
                || has(&sk, "ParentKeyName")
                || matches!(rel.as_str(), "Update" | "Hotfix" | "Security Update")
                || is_kb
                || lname.starts_with("update for")
                || lname.starts_with("security update")
            {
                continue;
            }
            let version = s(&sk, "DisplayVersion");
            if !seen.insert((lname.clone(), version.clone())) {
                continue;
            }
            let regpath = format!("{root}\\{sub}");
            apps.push(AppInfo {
                id: make_id(*hive, &regpath),
                name,
                publisher: s(&sk, "Publisher"),
                version,
                size_kb: n(&sk, "EstimatedSize").unwrap_or(0),
                date: normalise_date(&s(&sk, "InstallDate")),
                location: infer_location(&s(&sk, "InstallLocation"), &s(&sk, "DisplayIcon"), &uninstall_s).replace('/', "\\"),
                uninstall: if uninstall_s.is_empty() { cmd } else { uninstall_s },
                quiet,
                hive: *hive,
                regpath,
                no_remove,
            });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

pub fn scan_all() -> Vec<AppInfo> {
    scan_roots(UNINSTALL_ROOTS)
}

/// Names of other installed browsers (so removing Edge never leaves the PC without one).
pub fn browsers_in(apps: &[AppInfo]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for a in apps {
        let l = a.name.to_lowercase();
        if BROWSER_WORDS.iter().any(|b| l.contains(b)) && !l.contains("webview") && !names.contains(&a.name) {
            names.push(a.name.clone());
        }
    }
    names
}

/// Tell Edge's updater (policy "Allow installation" = 0 for Edge Stable) not to install Edge again.
/// Undo: delete that value, or set it to 1. WebView2 is not affected.
pub fn block_edge_reinstall_at(hive: Hive, path: &str) -> Result<String, String> {
    let value = format!("Install{EDGE_STABLE_GUID}");
    match hive.key().create_subkey_with_flags(path, KEY_SET_VALUE | KEY_WOW64_64KEY) {
        Ok((k, _)) => match k.set_value(&value, &0u32) {
            Ok(()) => Ok(format!("reinstall blocked ({}\\{path} \\ {value} = 0)", hive.name())),
            Err(e) => Err(e.to_string()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err("access denied - run Task Reaper as Administrator".into()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn block_edge_reinstall() -> Result<String, String> {
    block_edge_reinstall_at(Hive::Hklm, EDGE_POLICY_KEY)
}

/// Delete a stale "Installed apps" entry. A .reg backup is saved into `backup_dir` first so it can be restored by
/// double-clicking it. Returns the backup path.
pub fn remove_uninstall_entry_in(app: &AppInfo, backup_dir: &Path, stamp: &str) -> Result<String, String> {
    if app.regpath.is_empty() || !app.regpath.to_lowercase().contains("\\uninstall\\") {
        return Err("unsupported registry location".into());
    }
    if let Err(e) = std::fs::create_dir_all(backup_dir) {
        return Err(format!("could not save a registry backup first ({e}) - nothing was changed"));
    }
    let backup = backup_dir.join(format!("{}-{}.reg", file_stem(&app.name), stamp));
    let bs = backup.to_string_lossy().to_string();
    let (ok, out) = crate::util::run_hidden("reg", &["export", &format!("{}\\{}", app.hive.name(), app.regpath), &bs, "/y"]);
    let good = ok && std::fs::metadata(&backup).map(|m| m.len() > 0).unwrap_or(false);
    if !good {
        let last = out.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim().to_string();
        return Err(format!("could not save a registry backup first ({last}) - nothing was changed"));
    }
    let (parent, leaf) = match app.regpath.rsplit_once('\\') {
        Some(x) => x,
        None => return Err("unsupported registry location".into()),
    };
    let r = app
        .hive
        .key()
        .open_subkey_with_flags(parent, KEY_ALL_ACCESS | KEY_WOW64_64KEY)
        .and_then(|p| p.delete_subkey_all(leaf));
    match r {
        Ok(()) => Ok(bs),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err("access denied - run Task Reaper as Administrator".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(bs),
        Err(e) => Err(e.to_string()),
    }
}
