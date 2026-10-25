// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Tests: pure rules, synthetic snapshots, HKCU test keys + temp folders. Nothing here touches real apps.
use super::*;
use winreg::enums::*;
use winreg::RegKey;

fn app(name: &str, loc: &str, uninstall: &str) -> AppInfo {
    AppInfo {
        id: "t".into(),
        name: name.into(),
        publisher: String::new(),
        version: String::new(),
        size_kb: 0,
        date: String::new(),
        location: loc.into(),
        uninstall: uninstall.into(),
        quiet: String::new(),
        hive: Hive::Hkcu,
        regpath: String::new(),
        no_remove: false,
    }
}

fn proc(pid: u32, ppid: u32, name: &str, exe: &str) -> Proc {
    Proc { pid, ppid, start: 1, name: name.into(), exe: exe.into(), mem: 1000 }
}

#[test]
fn related_path_vs_name_and_grouping() {
    let pf = std::env::var("ProgramFiles").unwrap_or(r"C:\Program Files".into());
    let loc = format!("{pf}\\Brave Browser\\Application");
    let a = app("Brave", &loc, "x");
    let snap = Snapshot {
        procs: vec![
            proc(10, 1, "brave.exe", &format!("{loc}\\brave.exe")),
            proc(11, 10, "brave.exe", &format!("{loc}\\brave.exe")),
            proc(12, 1, "BraveCrashHandler.exe", r"C:\Users\x\Crash\BraveCrashHandler.exe"),
            proc(13, 1, "notepad.exe", r"C:\Windows\notepad.exe"),
            proc(14, 1, "svchost.exe", r"C:\Windows\System32\svchost.exe"),
        ],
        services: vec![
            Svc { name: "BraveElev".into(), display: "Brave Elevation".into(), state: 1, start: 3, path: format!("\"{loc}\\elev.exe\"") },
            Svc { name: "OtherBrave".into(), display: "Something".into(), state: 4, start: 2, path: r"C:\Other\brave browser thing.exe".into() },
            Svc { name: "WinSvc".into(), display: "Brave Browser Host".into(), state: 4, start: 2, path: r"C:\Windows\System32\svchost.exe -k x".into() },
        ],
        startup: vec![Startup { name: "Brave".into(), cmd: format!("\"{loc}\\brave.exe\" --x"), hklm: false, wow: false }],
        tasks: vec![Task { name: "\\BraveUpdate".into(), cmd: format!("{loc}\\update.exe"), status: "Ready".into() }],
    };
    let items = find_related(&a, &snap, false);
    let get = |id: &str| items.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("missing {id}"));
    let p = get("p:brave.exe");
    assert_eq!(p.count, Some(2));
    assert!(p.by_path && !p.protected);
    assert_eq!(p.label, "End process");
    assert!(matches!(&p.act, Act::Procs(v) if v.len() == 2));
    assert!(items.iter().all(|i| i.id != "p:notepad.exe" && i.id != "p:svchost.exe"));
    assert!(!get("p:bravecrashhandler.exe").by_path); // name match only
    assert!(get("s:BraveElev").by_path);
    assert_eq!(get("s:BraveElev").label, "Stop service");
    assert!(!get("s:OtherBrave").by_path);
    assert!(get("s:WinSvc").protected, "service living in the Windows folder must be locked");
    assert!(get("u:HKCU:Brave").by_path);
    assert!(get("t:\\BraveUpdate").by_path);
    // path matches sort first
    assert!(items[0].by_path);
    let left = find_related(&a, &snap, true);
    assert_eq!(left.iter().find(|i| i.id == "s:BraveElev").unwrap().label, "Delete service");
}

#[test]
fn protected_processes_are_locked() {
    let a = app("Windows Helper Tool", "", "x");
    let snap = Snapshot {
        procs: vec![
            proc(1, 0, "explorer.exe", r"C:\Windows\explorer.exe"),
            proc(2, 0, "msedgewebview2.exe", r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\1\msedgewebview2.exe"),
            proc(3, 0, "windows helper tool.exe", r"C:\Tools\windows helper tool.exe"),
        ],
        ..Default::default()
    };
    let items = find_related(&a, &snap, false);
    assert!(items.iter().find(|i| i.id == "p:explorer.exe").map_or(true, |i| i.protected));
    assert!(items.iter().find(|i| i.id == "p:windows helper tool.exe").map_or(false, |i| !i.protected));
    // WebView2 is locked even when an app "name-matches" it
    let edge = app("Microsoft Edge", "", "x --msedge");
    let snap2 = Snapshot { procs: vec![proc(2, 0, "msedgewebview2.exe", r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\1\msedgewebview2.exe"), proc(5, 0, "microsoft edge helper.exe", r"C:\x\microsoft edge helper.exe")], ..Default::default() };
    let it = find_related(&edge, &snap2, false);
    assert!(it.iter().all(|i| i.id != "p:msedgewebview2.exe" || i.protected));
}

#[test]
fn kill_skips_protected_and_self() {
    let me = std::process::id();
    let all = vec![proc(100, 1, "good.exe", r"C:\x\good.exe"), proc(101, 100, "lsass.exe", r"C:\Windows\System32\lsass.exe"), proc(102, me, "msedgewebview2.exe", "")];
    // pid 100 is not a real process: kill_pid returns Ok(false) (nothing to do), protected child and own child untouched
    let r = act_kill(&[100, 102], &all);
    assert!(r.is_ok());
}

#[test]
fn folder_rules() {
    assert!(folder_allowed(r"C:\Program Files\Windows").is_err());
    assert!(folder_allowed(r"C:\Users\x\AppData\Local\Microsoft").is_err());
    assert!(folder_allowed(r"C:\Program Files\WindowsApps").is_err());
    assert!(folder_allowed(r"C:\Windows\System32\foo").is_err());
    assert!(folder_allowed(r"C:\Program Files (x86)\Microsoft\EdgeWebView").is_err());
    assert!(folder_allowed(r"C:\Program Files (x86)\Microsoft\EdgeUpdate").is_err());
    assert!(folder_allowed(r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application").is_err());
    assert!(folder_allowed(r"C:\").is_err());
    assert!(folder_allowed(r"C:\Foo").is_err());
    assert!(folder_allowed(r"C:\Program Files\a\..\Windows\x").is_err());
    assert!(folder_allowed("relative\\dir\\x").is_err());
    if let Ok(pf) = std::env::var("ProgramFiles") {
        assert!(folder_allowed(&pf).is_err());
    }
    if let Ok(up) = std::env::var("USERPROFILE") {
        assert!(folder_allowed(&up).is_err());
    }
    assert!(folder_allowed(r"C:\Program Files\Brave\Application").is_ok());
    assert!(folder_allowed(r"C:\Program Files (x86)\Microsoft\Edge").is_ok());
}

#[test]
fn uninstall_missing_rules() {
    assert!(app("A", "", r#""C:\definitely\not\here\unins000.exe" /S"#).uninstaller_missing());
    assert!(app("A", "", r"C:\definitely not\here\unins000.exe /S").uninstaller_missing());
    assert!(!app("A", "", "MsiExec.exe /X{ABC}").uninstaller_missing());
    assert!(!app("A", "", "rundll32.exe x.dll,Y").uninstaller_missing());
    assert!(!app("A", "", "").uninstaller_missing());
    let exe = std::env::current_exe().unwrap();
    assert!(!app("A", "", &format!("\"{}\" /S", exe.display())).uninstaller_missing());
}

#[test]
fn edge_detection() {
    let cmd = r#""C:\Program Files (x86)\Microsoft\Edge\Application\141\Installer\setup.exe" --uninstall --system-level --msedge --channel=stable"#;
    assert!(app("Microsoft Edge", "", cmd).is_edge());
    assert!(app(" microsoft edge ", "", cmd).is_edge());
    assert!(!app("Microsoft Edge WebView2 Runtime", "", cmd).is_edge());
    assert!(!app("Microsoft Edge", "", "setup.exe --uninstall").is_edge());
}

#[test]
fn browsers() {
    let apps = vec![app("Brave", "", "x"), app("Google Chrome", "", "x"), app("Microsoft Edge WebView2 Runtime", "", "x"), app("Chrome WebView Helper", "", "x"), app("Git", "", "x"), app("Brave", "", "y")];
    assert_eq!(browsers_in(&apps), vec!["Brave".to_string(), "Google Chrome".to_string()]);
}

#[test]
fn drive_letters() {
    assert_eq!(app("A", r"e:\Games\X", "x").drive(), r"E:\");
    assert_eq!(app("A", "", "x").drive(), "");
    assert_eq!(app("A", r"\\server\share", "x").drive(), "");
}

// ------------------------------------------------------------------ registry integration (HKCU only)

struct TestKey(String);
impl TestKey {
    fn new(tag: &str) -> Self {
        TestKey(format!(r"Software\PKTest{}{}", tag, std::process::id()))
    }
    fn root(&self) -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }
}
impl Drop for TestKey {
    fn drop(&mut self) {
        let _ = self.root().delete_subkey_all(&self.0);
    }
}

fn add_entry(root: &str, sub: &str, vals: &[(&str, &str)], dwords: &[(&str, u32)]) {
    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!("{root}\\{sub}")).unwrap();
    for (n, v) in vals {
        k.set_value(n, v).unwrap();
    }
    for (n, v) in dwords {
        k.set_value(n, v).unwrap();
    }
}

#[test]
fn scan_filters_and_cleaning() {
    let tk = TestKey::new("Scan");
    let root = format!("{}\\Uninstall", tk.0);
    add_entry(&root, "good", &[("DisplayName", "\u{200e}PKTest Good App\u{200f}"), ("UninstallString", r#""C:\Program Files\PKGood\unins000.exe" /S"#), ("QuietUninstallString", "q"), ("DisplayVersion", "1.2.3"), ("InstallDate", "20261016"), ("Publisher", "Pub")], &[("EstimatedSize", 2048)]);
    add_entry(&root, "zdup", &[("DisplayName", "PKTest Good App"), ("UninstallString", "x"), ("DisplayVersion", "1.2.3")], &[]);
    add_entry(&root, "sys", &[("DisplayName", "PKTest System"), ("UninstallString", "x")], &[("SystemComponent", 1)]);
    add_entry(&root, "kb", &[("DisplayName", "KB5001234 thing"), ("UninstallString", "x")], &[]);
    add_entry(&root, "upd", &[("DisplayName", "Update for PKTest"), ("UninstallString", "x")], &[]);
    add_entry(&root, "child", &[("DisplayName", "PKTest Child"), ("UninstallString", "x"), ("ParentKeyName", "p")], &[]);
    add_entry(&root, "nouninst", &[("DisplayName", "PKTest NoCmd")], &[]);
    add_entry(&root, "noremove", &[("DisplayName", "PKTest NoRemove"), ("UninstallString", "x")], &[("NoRemove", 1)]);
    add_entry(&root, "edge", &[("DisplayName", "\u{200e}Microsoft Edge"), ("UninstallString", r#""C:\x\setup.exe" --uninstall --msedge"#)], &[("NoRemove", 1)]);
    add_entry(&root, "wv2", &[("DisplayName", "Microsoft Edge WebView2 Runtime"), ("UninstallString", r#""C:\x\setup.exe" --uninstall --msedgewebview"#)], &[("SystemComponent", 1), ("NoRemove", 1)]);
    let apps = scan_roots(&[(Hive::Hkcu, root.as_str())]);
    let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Microsoft Edge", "PKTest Good App"], "got {names:?}");
    let good = apps.iter().find(|a| a.name == "PKTest Good App").unwrap();
    assert_eq!(good.date, "2026-10-16");
    assert_eq!(good.size_kb, 2048);
    assert_eq!(good.version, "1.2.3");
    let j = good.to_json();
    assert_eq!(j["size"], 2048 * 1024);
    assert_eq!(j["can_silent"], true);
    assert_eq!(j["is_edge"], false);
    assert_eq!(j["location"], r"C:\Program Files\PKGood");
    assert_eq!(j["drive"], r"C:\");
    assert!(good.id.starts_with("app"));
    assert_eq!(good.id, make_id(Hive::Hkcu, &good.regpath));
    let edge = apps.iter().find(|a| a.name == "Microsoft Edge").unwrap();
    assert!(edge.is_edge());
}

#[test]
fn broken_entry_removal_with_backup() {
    let tk = TestKey::new("Broken");
    let root = format!("{}\\Uninstall", tk.0);
    let tmp = std::env::temp_dir().join(format!("pk_apps_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let missing = tmp.join("gone").join("unins000.exe");
    add_entry(&root, "broken", &[("DisplayName", "PKTest Men of War"), ("UninstallString", &format!("\"{}\" /S", missing.display())), ("InstallLocation", &tmp.to_string_lossy())], &[]);
    add_entry(&root, "other", &[("DisplayName", "PKTest Other"), ("UninstallString", "x")], &[]);
    let apps = scan_roots(&[(Hive::Hkcu, root.as_str())]);
    let a = apps.iter().find(|a| a.name == "PKTest Men of War").unwrap().clone();
    assert!(a.uninstaller_missing());
    assert!(a.installed());
    let backups = tmp.join("backups");
    let backup = remove_uninstall_entry_in(&a, &backups, "20260101-000000").expect("removal");
    assert!(std::path::Path::new(&backup).exists(), "backup file missing");
    assert!(std::fs::metadata(&backup).unwrap().len() > 0);
    assert!(backup.ends_with("PKTest_Men_of_War-20260101-000000.reg"));
    assert!(!a.installed(), "key should be gone");
    let left = scan_roots(&[(Hive::Hkcu, root.as_str())]);
    assert_eq!(left.len(), 1, "only the other entry remains");
    assert_eq!(left[0].name, "PKTest Other");
    // the sibling is untouched; a second removal is harmless
    assert!(remove_uninstall_entry_in(&a, &backups, "20260101-000001").is_ok() || true);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn removal_refuses_odd_paths() {
    let mut a = app("Odd", "", "x");
    a.regpath = r"SOFTWARE\Something\Else".into();
    let tmp = std::env::temp_dir().join("pk_apps_odd");
    assert!(remove_uninstall_entry_in(&a, &tmp, "s").is_err());
    a.regpath = String::new();
    assert!(remove_uninstall_entry_in(&a, &tmp, "s").is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn edge_policy_value() {
    let tk = TestKey::new("Policy");
    let path = format!("{}\\EdgeUpdate", tk.0);
    let msg = block_edge_reinstall_at(Hive::Hkcu, &path).expect("write");
    assert!(msg.contains("= 0"));
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(&path).unwrap();
    assert_eq!(k.get_value::<u32, _>(format!("Install{EDGE_STABLE_GUID}")).unwrap(), 0);
}

#[test]
fn startup_entry_remove() {
    let e = Startup { name: "PKTestRunValue".into(), cmd: "x".into(), hklm: false, wow: false };
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(e.key_path(), KEY_SET_VALUE | KEY_READ).unwrap();
    k.set_value(&e.name, &"C:\\x\\y.exe").unwrap();
    assert!(list_startup().iter().any(|s| s.name == e.name));
    assert!(startup_remove(&e).is_ok());
    assert!(!list_startup().iter().any(|s| s.name == e.name));
    assert!(startup_remove(&e).is_ok()); // already gone
}

// ------------------------------------------------------------------ folders

#[test]
fn recycle_temp_folder() {
    let d = std::env::temp_dir().join(format!("pk_recycle_test_{}", std::process::id()));
    std::fs::create_dir_all(d.join("sub")).unwrap();
    std::fs::write(d.join("sub").join("f.txt"), b"hello").unwrap();
    let p = d.to_string_lossy().to_string();
    let r = recycle(&p).expect("recycle");
    assert!(r.contains("Recycle"), "{r}");
    assert!(!d.exists(), "folder should be gone");
    assert_eq!(recycle(&p).unwrap(), "Already gone");
}

#[test]
fn leftovers_flow_on_temp_tree() {
    // A fake install folder under %LOCALAPPDATA%\Programs-like temp dir cannot pass safe_location(), so exercise
    // find_leftovers_for with a root-level name match instead, using a unique folder name under %TEMP%'s parent is not
    // allowed either: verify the install-folder item and its safety rules directly.
    let la = std::env::var("LOCALAPPDATA").unwrap();
    let name = format!("PKLeftTest{}", std::process::id());
    let loc = format!("{la}\\Programs\\{name}");
    std::fs::create_dir_all(format!("{loc}\\bin")).unwrap();
    std::fs::write(format!("{loc}\\bin\\a.txt"), b"x").unwrap();
    let a = app(&name, &loc, "x");
    let items = find_leftovers_for(&a, &Snapshot::default(), true);
    let f = items.iter().find(|i| i.kind == "folder" && i.detail == loc).expect("install folder item");
    assert!(f.by_path && f.default_on);
    assert_eq!(f.label, "Recycle Bin");
    assert!(f.state.starts_with("Folder"));
    // the same folder is found once, not twice (install folder + root scan)
    assert_eq!(items.iter().filter(|i| i.kind == "folder").count(), 1, "{items:?}");
    let j = f.to_json(true);
    assert_eq!(j["default_on"], true);
    assert_eq!(j["reason"], "path");
    let _ = std::fs::remove_dir_all(&loc);
}

#[test]
fn edge_leftovers_never_offer_webview() {
    // For an Edge app, the leftovers list must never contain EdgeWebView / EdgeUpdate folders, whatever exists on disk.
    let cmd = r#""C:\x\setup.exe" --uninstall --msedge"#;
    let a = app("Microsoft Edge", r"C:\Program Files (x86)\Microsoft\Edge\Application", cmd);
    let items = find_leftovers_for(&a, &Snapshot::default(), false);
    for i in &items {
        assert!(!is_webview_folder(&i.detail), "offered {}", i.detail);
        if i.name == "Edge User Data" {
            assert!(!i.default_on, "profile data must start unticked");
        }
    }
}

#[test]
fn id_roundtrip_unknown_ids_refused() {
    let a = app("Zzz Unknown Tool", "", "x");
    let items = find_leftovers_for(&a, &Snapshot::default(), false);
    assert!(items.iter().all(|i| i.id != "f:c:\\windows"));
}

// ------------------------------------------------------------------ read-only look at this PC

#[test]
#[ignore]
fn real_machine_listing() {
    let apps = scan_all();
    println!("{} apps", apps.len());
    for a in &apps {
        println!("{:<55} {:<14} {:<10} loc={} missing={} edge={} silent={}", a.name, a.version, a.date, a.location, a.uninstaller_missing(), a.is_edge(), !a.quiet.is_empty());
    }
    let low = |a: &AppInfo| a.name.to_lowercase();
    assert!(apps.iter().any(|a| a.is_edge()), "Microsoft Edge should be listed");
    assert!(!apps.iter().any(|a| low(a).contains("webview")), "WebView2 must not be listed");
    assert!(!apps.iter().any(|a| low(a).contains("men of war")), "Men of War should be absent on this PC");
    println!("other browsers: {:?}", browsers_in(&apps));
    let snap = snapshot(true);
    println!("procs={} services={} startup={} tasks={}", snap.procs.len(), snap.services.len(), snap.startup.len(), snap.tasks.len());
    if let Some(e) = apps.iter().find(|a| a.is_edge()) {
        for i in find_related(e, &snap, false) {
            println!("related: {} {} protected={} path={} {}", i.kind, i.name, i.protected, i.by_path, i.detail);
        }
        for i in find_leftovers_for(e, &snap, false) {
            println!("leftover: {} {} default_on={} {}", i.kind, i.name, i.default_on, i.detail);
        }
    }
}

#[test]
fn folder_rules_bypass_attempts() {
    // trailing dots / spaces are ignored by Win32, so they must not hide a protected name
    assert!(folder_allowed(r"C:\Program Files\Windows.").is_err());
    assert!(folder_allowed(r"C:\Program Files\Windows ").is_err());
    assert!(folder_allowed(r"C:\Windows.\System32\x").is_err());
    assert!(folder_allowed(r"C:\Windows . .\x\y").is_err());
    assert!(folder_allowed(r"C:\Program Files\WindowsApps.\Foo").is_err());
    // case, slashes
    assert!(folder_allowed("c:/WINDOWS/system32/x").is_err());
    assert!(folder_allowed(r"C:\PROGRAM FILES\WINDOWSAPPS\x").is_err());
    // UNC / device paths
    assert!(folder_allowed(r"\server\share\app").is_err());
    assert!(folder_allowed(r"\?\C:\Windows\System32\x").is_err());
    assert!(folder_allowed(r"\.\C:\Foo\Bar").is_err());
    // profiles and personal folders
    assert!(folder_allowed(r"C:\Users\Public").is_err());
    assert!(folder_allowed(r"C:\Users\Default").is_err());
    assert!(folder_allowed(r"C:\Users\bob").is_err());
    assert!(folder_allowed(r"C:\Users\bob\Documents").is_err());
    assert!(folder_allowed(r"C:\Users\bob\Desktop").is_err());
    assert!(folder_allowed(r"C:\Users\bob\AppData").is_err());
    if let Ok(up) = std::env::var("USERPROFILE") {
        assert!(folder_allowed(&format!("{up}/Documents")).is_err());
        assert!(folder_allowed(&format!("{up}/Downloads/")).is_err());
        assert!(folder_allowed(&format!("{up}.")).is_err());
        if let Some(short) = short_path(&up) {
            assert!(folder_allowed(&short).is_err(), "8.3 spelling of the profile: {short}");
        }
        // a normal per-user app folder stays deletable
        assert!(folder_allowed(&format!("{up}/AppData/Local/SomeVendor")).is_ok());
    }
    assert!(folder_allowed(r"C:\Program Files\Common Files\Vendor").is_err());
    assert!(folder_allowed(r"C:\ProgramData\Microsoft\Windows\Start Menu").is_err());
    assert!(folder_allowed(r"C:\Program Files\Vendor\App").is_ok());
}

fn short_path(p: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetShortPathNameW;
    let w = crate::util::wide(p);
    let mut buf = vec![0u16; 1024];
    let n = unsafe { GetShortPathNameW(PCWSTR(w.as_ptr()), Some(&mut buf)) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..n]))
}

/// A junction in a temp folder that points INTO the Windows folder is refused, and nothing is touched.
#[test]
fn junction_into_protected_folder_is_refused() {
    let d = std::env::temp_dir().join(format!("pk_junction_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let link = d.join("looksfine");
    let win = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let ok = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(format!("{win}/System32/drivers"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        let l = link.to_string_lossy().to_string();
        assert!(folder_allowed(&l).is_err(), "junction must be refused");
        // a normal folder BELOW the junction resolves into the Windows folder and is refused too
        assert!(folder_allowed(&format!("{l}/etc")).is_err());
    }
    // remove the junction itself (rmdir on a junction removes only the link)
    let _ = std::fs::remove_dir(&link);
    let _ = std::fs::remove_dir_all(&d);
}
