// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Safety rules ported from the 1.x app (core.py), hardened after review. EVERY destructive command must call these - the
//! UI checks too, but the backend is the last line of defence.
use crate::util::{wide, win_dir};

/// Processes that can never be ended (also used when the exe path cannot be read).
const PROTECTED_PROCS: &[&str] = &[
    "system", "system idle process", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe",
    "services.exe", "lsass.exe", "svchost.exe", "dwm.exe", "explorer.exe", "fontdrvhost.exe", "memory compression",
    "audiodg.exe", "lsaiso.exe", "sihost.exe", "taskhostw.exe", "msmpeng.exe", "nissrv.exe",
    "securityhealthservice.exe", "spoolsv.exe",
];

/// Folders (by name) that are never offered for deletion.
const PROTECTED_DIRS: &[&str] = &[
    "microsoft", "windows", "windowsapps", "common files", "windows nt", "internet explorer", "windows defender",
    "modifiablewindowsapps", "packages", "temp", "microsoft shared", "dotnet", "reference assemblies",
];

/// Files that can never be deleted by the storage analyzer.
const UNDELETABLE_FILES: &[&str] = &["pagefile.sys", "hiberfil.sys", "swapfile.sys"];

pub fn is_protected_process(name: &str, exe: &str) -> bool {
    let n = name.to_lowercase();
    PROTECTED_PROCS.contains(&n.as_str()) || normalize(exe).starts_with(&format!("{}\\", win_dir()))
}

pub fn is_protected_dir_name(name: &str) -> bool {
    PROTECTED_DIRS.contains(&name.trim_end_matches(['.', ' ']).to_lowercase().as_str())
}

/// Resolve 8.3 short names (PROGRA~1) to the long spelling when the path exists; otherwise return it unchanged.
fn long_path(p: &str) -> String {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetLongPathNameW;
    let w = wide(p);
    let n = unsafe { GetLongPathNameW(PCWSTR(w.as_ptr()), None) } as usize;
    if n == 0 {
        return p.to_string();
    }
    let mut buf = vec![0u16; n + 1];
    let m = unsafe { GetLongPathNameW(PCWSTR(w.as_ptr()), Some(&mut buf)) } as usize;
    if m == 0 || m >= buf.len() {
        return p.to_string();
    }
    String::from_utf16_lossy(&buf[..m])
}

/// Canonical comparison spelling: forward slashes -> `\`, no `\\?\` prefix, no trailing dots/spaces per component (Windows ignores
/// them, so `C:\Windows.\x` IS `C:\Windows\x`), 8.3 names expanded, lower-case. `..` components are NOT resolved here: callers treat
/// them as hostile (see `has_dot_components`).
pub fn normalize(path: &str) -> String {
    let mut s = path.trim().trim_matches('"').replace('/', "\\");
    if let Some(r) = s.strip_prefix("\\\\?\\") {
        s = r.to_string();
    }
    let cleaned: Vec<&str> = s
        .split('\\')
        .map(|c| if c == "." || c == ".." { c } else { c.trim_end_matches(['.', ' ']) })
        .collect();
    long_path(&cleaned.join("\\")).to_lowercase().trim_end_matches('\\').to_string()
}

/// True when any path component is `.` or `..` (never legitimate in a path we produced ourselves).
pub fn has_dot_components(path: &str) -> bool {
    path.replace('/', "\\").split('\\').any(|c| c == "." || c == "..")
}

/// True for anything inside the Windows folder, a known system file, or a path we cannot judge safely.
pub fn is_protected_file(path: &str) -> bool {
    if has_dot_components(path) {
        return true;
    }
    let p = normalize(path);
    let base = p.rsplit('\\').next().unwrap_or("");
    UNDELETABLE_FILES.contains(&base) || p == win_dir() || p.starts_with(&format!("{}\\", win_dir()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_folder_is_protected_in_every_spelling() {
        let w = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        for p in [format!("{w}\\system32\\cmd.exe"), format!("{}\\SYSTEM32\\cmd.exe", w.to_uppercase()), format!("{}\\system32\\cmd.exe", w.replace('\\', "/")),
                  format!("{w}.\\system32\\cmd.exe"), format!("{w} \\system32\\cmd.exe"), format!("\\\\?\\{w}\\system32\\cmd.exe"), w.clone()] {
            assert!(is_protected_file(&p), "should be protected: {p}");
        }
    }

    #[test]
    fn traversal_and_system_files_are_protected() {
        assert!(is_protected_file("C:\\Users\\x\\..\\..\\Windows\\a.dll"));
        assert!(is_protected_file("C:\\pagefile.sys"));
        assert!(is_protected_file("D:\\HIBERFIL.SYS"));
        assert!(is_protected_file("D:\\data\\.\\file.txt"));
    }

    #[test]
    fn ordinary_user_files_are_allowed() {
        assert!(!is_protected_file("E:\\DEV\\project\\big.iso"));
        assert!(!is_protected_file("D:\\Games\\file.bin"));
    }

    #[test]
    fn protected_processes() {
        assert!(is_protected_process("LSASS.EXE", ""));
        assert!(is_protected_process("sihost.exe", ""));
        assert!(is_protected_process("something.exe", "C:\\Windows\\System32\\something.exe"));
        assert!(!is_protected_process("brave.exe", "C:\\Program Files\\Brave\\brave.exe"));
    }

    #[test]
    fn protected_dir_names_ignore_case_and_trailing_dots() {
        assert!(is_protected_dir_name("Microsoft"));
        assert!(is_protected_dir_name("WindowsApps."));
        assert!(!is_protected_dir_name("MyGame"));
    }
}
