// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
#![allow(dead_code)]
//! Startup page: Run-key entries (HKLM, HKLM 32-bit, HKCU) and non-Microsoft scheduled tasks.
//! Tasks are read straight from `System32\Tasks` (fast); `schtasks /query` is only the fallback.
use std::path::Path;

use serde_json::{json, Value};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

use crate::util::{err, run_hidden, CmdError, CmdResult};

// ---------------------------------------------------------------- model

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hive {
    Lm,
    Cu,
}
impl Hive {
    fn root(self) -> RegKey {
        RegKey::predef(match self {
            Hive::Lm => HKEY_LOCAL_MACHINE,
            Hive::Cu => HKEY_CURRENT_USER,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunEntry {
    pub hive: Hive,
    pub path: String,
    pub name: String,
    pub command: String,
    pub location: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskEntry {
    pub name: String,
    pub command: String,
    pub status: String,
}

const RUN_KEYS: &[(Hive, &str, &str)] = &[
    (Hive::Lm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "HKLM Run"),
    (Hive::Lm, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run", "HKLM (32-bit) Run"),
    (Hive::Cu, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "HKCU Run"),
];

/// Deterministic opaque id: the same entry gets the same id in every listing (FNV-1a), so ids can be re-validated
/// against a fresh listing.
fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}
fn run_id(e: &RunEntry) -> String {
    format!("s{}", fnv(&format!("{:?}|{}|{}", e.hive, e.path.to_lowercase(), e.name.to_lowercase())))
}
fn task_id(name: &str) -> String {
    format!("t{}", fnv(&name.to_lowercase()))
}

// ---------------------------------------------------------------- registry

pub fn list_run_at(hive: Hive, path: &str, label: &str) -> Vec<RunEntry> {
    let mut out = Vec::new();
    let Ok(key) = hive.root().open_subkey_with_flags(path, KEY_READ) else {
        return out;
    };
    for item in key.enum_values() {
        let Ok((name, val)) = item else { continue };
        let RegValue { vtype, .. } = &val;
        if !matches!(vtype, REG_SZ | REG_EXPAND_SZ) {
            continue;
        }
        let command = String::from_utf16_lossy(
            &val.bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<u16>>(),
        )
        .trim_end_matches('\0')
        .to_string();
        out.push(RunEntry { hive, path: path.to_string(), name, command, location: label.to_string() });
    }
    out
}

pub fn list_run_entries() -> Vec<RunEntry> {
    RUN_KEYS.iter().flat_map(|(h, p, l)| list_run_at(*h, p, l)).collect()
}

/// Delete one registry value. "Not there" counts as success.
pub fn remove_run_value(hive: Hive, path: &str, name: &str) -> Result<&'static str, String> {
    match hive.root().open_subkey_with_flags(path, KEY_SET_VALUE) {
        Ok(k) => match k.delete_value(name) {
            Ok(()) => Ok("startup entry removed"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("already gone"),
            Err(e) => Err(io_msg(&e)),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("already gone"),
        Err(e) => Err(io_msg(&e)),
    }
}

fn io_msg(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "access denied (run as Administrator)".into()
    } else {
        e.to_string()
    }
}

// ---------------------------------------------------------------- task XML

fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let u: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)).into_owned()
    }
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Text of the first `<tag ...>text</tag>` in `xml` (namespace prefixes ignored). Self-closing tags give None.
fn tag_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let (s, _) = tag_span(xml, tag, 0)?;
    Some(s)
}

/// (inner text, end offset) of the first `<tag>` at or after `from`.
fn tag_span<'a>(xml: &'a str, tag: &str, from: usize) -> Option<(&'a str, usize)> {
    let lower_open = format!("<{}", tag.to_ascii_lowercase());
    let hay = &xml[from..];
    // ASCII-only: full Unicode lowercasing changes byte lengths (U+0130, U+212A) and would shift every offset below
    let hay_l = hay.to_ascii_lowercase();
    let mut search = 0;
    loop {
        let rel = hay_l[search..].find(&lower_open)? + search;
        let after = rel + lower_open.len();
        let c = hay_l.as_bytes().get(after).copied().unwrap_or(b'>');
        if !(c == b'>' || c == b' ' || c == b'/' || c == b'\t' || c == b'\r' || c == b'\n') {
            search = after; // a longer tag name such as <CommandX>
            continue;
        }
        let gt = hay_l[after..].find('>')? + after;
        if hay_l.as_bytes()[gt - 1] == b'/' {
            return Some(("", from + gt + 1));
        }
        let close = format!("</{}>", tag.to_ascii_lowercase());
        let end = hay_l[gt + 1..].find(&close)? + gt + 1;
        return Some((&hay[gt + 1..end], from + end + close.len()));
    }
}

/// Parse the interesting parts of a Task Scheduler XML definition: all `<Exec>` commands joined with " | "
/// and the `Settings/Enabled` flag.
pub fn parse_task_xml(xml: &str) -> (String, bool) {
    let mut cmds = Vec::new();
    if let Some(actions) = tag_text(xml, "Actions") {
        let mut pos = 0;
        while let Some((exec, end)) = tag_span(actions, "Exec", pos) {
            let c = tag_text(exec, "Command").map(|s| unescape(s.trim())).unwrap_or_default();
            let a = tag_text(exec, "Arguments").map(|s| unescape(s.trim())).unwrap_or_default();
            let line = format!("{c} {a}").trim().to_string();
            if !line.is_empty() {
                cmds.push(line);
            }
            pos = end;
        }
    }
    let enabled = tag_text(xml, "Settings")
        .and_then(|s| tag_text(s, "Enabled"))
        .map(|v| !v.trim().eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    (cmds.join(" | "), enabled)
}

/// Returns None when the Tasks folder cannot be used (then the schtasks fallback runs).
fn tasks_from_files(dir: &Path) -> Option<Vec<TaskEntry>> {
    let top = std::fs::read_dir(dir).ok()?;
    let mut stack: Vec<std::path::PathBuf> = Vec::new();
    for e in top.flatten() {
        let skip = e.file_type().map(|t| t.is_dir()).unwrap_or(false) && e.file_name().to_string_lossy().eq_ignore_ascii_case("microsoft");
        if !skip {
            stack.push(e.path());
        }
    }
    let (mut rows, mut files) = (Vec::new(), 0usize);
    while let Some(p) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
        if meta.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
            continue;
        }
        files += 1;
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let (command, enabled) = parse_task_xml(&decode_text(&bytes));
        let rel = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy().replace('/', "\\");
        rows.push(TaskEntry {
            name: format!("\\{rel}"),
            command,
            status: if enabled { "Ready" } else { "Disabled" }.into(),
        });
    }
    if files > 0 && rows.is_empty() {
        return None; // folder exists but nothing readable (not elevated)
    }
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Some(rows)
}

// ---------------------------------------------------------------- schtasks CSV fallback

/// Minimal RFC-4180 CSV parser (quoted fields, doubled quotes, newlines inside quotes).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '"' => quoted = true,
                ',' => row.push(std::mem::take(&mut cur)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut cur));
                    rows.push(std::mem::take(&mut row));
                }
                _ => cur.push(c),
            }
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

/// Rows of `schtasks /query /fo csv /v` -> non-Microsoft tasks (TaskName = col 1, Status = col 3, Task To Run = col 8).
pub fn tasks_from_csv(text: &str) -> Vec<TaskEntry> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for r in parse_csv(text) {
        if r.len() < 9 || !r[1].starts_with('\\') || !seen.insert(r[1].clone()) {
            continue;
        }
        if r[1].to_lowercase().starts_with("\\microsoft\\") {
            continue;
        }
        out.push(TaskEntry { name: r[1].clone(), command: r[8].clone(), status: r[3].clone() });
    }
    out
}

fn tasks_from_schtasks() -> Vec<TaskEntry> {
    let (ok, text) = run_hidden("schtasks", &["/query", "/fo", "csv", "/v"]);
    if ok {
        tasks_from_csv(&text)
    } else {
        Vec::new()
    }
}

pub fn list_tasks() -> Vec<TaskEntry> {
    let dir = Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into())).join("System32").join("Tasks");
    tasks_from_files(&dir).unwrap_or_else(tasks_from_schtasks)
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn list_startup() -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut items: Vec<Value> = Vec::new();
        for e in list_run_entries() {
            items.push(json!({ "id": run_id(&e), "kind": "startup", "name": e.name, "command": e.command, "location": e.location, "status": "" }));
        }
        for t in list_tasks() {
            items.push(json!({ "id": task_id(&t.name), "kind": "task", "name": t.name, "command": t.command, "location": "", "status": t.status }));
        }
        Ok(json!({ "items": items }))
    })
    .await
    .map_err(|e| CmdError(e.to_string()))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn remove_startup(app: tauri::AppHandle, ids: Vec<String>) -> CmdResult<Value> {
    let results = tauri::async_runtime::spawn_blocking(move || -> CmdResult<Vec<Value>> {
        let runs = if ids.iter().any(|i| i.starts_with('s')) { list_run_entries() } else { Vec::new() };
        let tasks = if ids.iter().any(|i| i.starts_with('t')) { list_tasks() } else { Vec::new() };
        let mut out = Vec::new();
        for id in ids {
            let (ok, message) = if let Some(e) = runs.iter().find(|e| run_id(e) == id) {
                match remove_run_value(e.hive, &e.path, &e.name) {
                    Ok(m) => (true, m.to_string()),
                    Err(m) => (false, m),
                }
            } else if let Some(t) = tasks.iter().find(|t| task_id(&t.name) == id) {
                let (ok, text) = run_hidden("schtasks", &["/delete", "/tn", &t.name, "/f"]);
                if ok {
                    (true, "task deleted".to_string())
                } else {
                    (false, last_line(&text, "could not delete the task (run as Administrator?)"))
                }
            } else if id.starts_with('s') || id.starts_with('t') {
                (true, "already gone".to_string())
            } else {
                (false, "unknown entry".to_string())
            };
            let label = runs.iter().find(|e| run_id(e) == id).map(|e| format!("startup entry {}", e.name))
                .or_else(|| tasks.iter().find(|t| task_id(&t.name) == id).map(|t| format!("task {}", t.name)))
                .unwrap_or_else(|| "startup entry".into());
            crate::activity::log(&app, if ok { "ok" } else { "fail" }, format!("Remove {label}: {message}"));
            out.push(json!({ "id": id, "ok": ok, "message": message }));
        }
        Ok(out)
    })
    .await
    .map_err(|e| CmdError(e.to_string()))??;
    if results.is_empty() {
        return err("nothing selected");
    }
    Ok(json!({ "results": results }))
}

fn last_line(text: &str, fallback: &str) -> String {
    text.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()).unwrap_or_else(|| fallback.to_string())
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str) -> Vec<u8> {
        let mut v = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>
  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><Enabled>false</Enabled></Settings>
  <Actions Context="Author">
    <Exec><Command>"C:\Program Files\GIGABYTE\GCC.exe"</Command><Arguments>-b &amp; --x</Arguments></Exec>
    <Exec><Command>cmd.exe</Command></Exec>
  </Actions>
</Task>"#;

    #[test]
    fn task_xml_utf16() {
        let text = decode_text(&utf16(XML));
        let (cmd, enabled) = parse_task_xml(&text);
        assert_eq!(cmd, "\"C:\\Program Files\\GIGABYTE\\GCC.exe\" -b & --x | cmd.exe");
        assert!(!enabled, "Settings/Enabled=false must win over the trigger's Enabled=true");
    }

    /// Full Unicode lowercasing changes byte lengths (U+0130 gains a byte, U+212A loses one): offsets found in the
    /// lowered copy then slice the original at the wrong place and panic (= abort in release).
    #[test]
    fn task_xml_survives_unicode_that_changes_case_length() {
        for ch in ["\u{130}", "\u{212A}", "\u{1E9E}", "\u{3A3}"] {
            let x = format!("<Task><RegistrationInfo><Author>{ch}{ch}{ch}</Author><Description>{ch}</Description></RegistrationInfo><Settings><Enabled>false</Enabled></Settings><Actions><Exec><Command>C:\\x\\{ch}.exe</Command><Arguments>-a</Arguments></Exec></Actions></Task>");
            let (c, e) = parse_task_xml(&x);
            assert_eq!(c, format!("C:\\x\\{ch}.exe -a"));
            assert!(!e);
        }
        // garbage must never panic either
        for junk in ["<", "<Actions", "<Actions><Exec", "<Actions><Exec>", "<Actions><Exec><Command>", "</", "<Task><Actions/>", "\u{130}<Actions><Exec>"] {
            let _ = parse_task_xml(junk);
        }
    }

    #[test]
    fn task_xml_defaults_and_utf8() {
        let x = "<Task><Actions><Exec><Command>a.exe</Command><Arguments/></Exec></Actions></Task>";
        let (c, e) = parse_task_xml(&decode_text(x.as_bytes()));
        assert_eq!(c, "a.exe");
        assert!(e);
        let (c2, _) = parse_task_xml("<Task><Actions><ComHandler><ClassId>x</ClassId></ComHandler></Actions></Task>");
        assert_eq!(c2, "");
    }

    #[test]
    fn csv_fallback() {
        let csv = "\"HostName\",\"TaskName\",\"Next Run Time\",\"Status\",\"Logon Mode\",\"Last Run Time\",\"Last Result\",\"Author\",\"Task To Run\"\r\n\
\"PC\",\"\\GCC\",\"N/A\",\"Ready\",\"Interactive\",\"x\",\"0\",\"me\",\"\"\"C:\\Program Files\\GCC.exe\"\" -b\"\r\n\
\"PC\",\"\\Microsoft\\Windows\\Foo\",\"N/A\",\"Ready\",\"\",\"\",\"\",\"\",\"c.exe\"\r\n\
\"HostName\",\"TaskName\",\"Next Run Time\",\"Status\",\"Logon Mode\",\"Last Run Time\",\"Last Result\",\"Author\",\"Task To Run\"\r\n\
\"PC\",\"\\GCC\",\"N/A\",\"Ready\",\"\",\"\",\"\",\"\",\"dup\"\r\n\
\"PC\",\"\\Folder\\Zoom, Inc\",\"N/A\",\"Disabled\",\"\",\"\",\"\",\"\",\"z.exe\"\r\n";
        let t = tasks_from_csv(csv);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].name, "\\GCC");
        assert_eq!(t[0].command, "\"C:\\Program Files\\GCC.exe\" -b");
        assert_eq!(t[1].name, "\\Folder\\Zoom, Inc");
        assert_eq!(t[1].status, "Disabled");
    }

    #[test]
    fn ids_are_stable() {
        let e = RunEntry { hive: Hive::Cu, path: "P".into(), name: "Discord".into(), command: "".into(), location: "".into() };
        assert_eq!(run_id(&e), run_id(&e.clone()));
        assert!(run_id(&e).starts_with('s') && task_id("\\GCC").starts_with('t'));
        assert_ne!(task_id("\\A"), task_id("\\B"));
    }

    #[test]
    fn tasks_folder_scan() {
        let d = std::env::temp_dir().join(format!("pk_tasks_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("Microsoft")).unwrap();
        std::fs::create_dir_all(d.join("Vendor")).unwrap();
        std::fs::write(d.join("Microsoft").join("Skip"), utf16(XML)).unwrap();
        std::fs::write(d.join("Vendor").join("Upd"), utf16(XML)).unwrap();
        let t = tasks_from_files(&d).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "\\Vendor\\Upd");
        assert_eq!(t[0].status, "Disabled");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hkcu_run_roundtrip() {
        let path = r"Software\ProcessKillerTest\StartupRoundtrip\Run";
        let cu = RegKey::predef(HKEY_CURRENT_USER);
        let (k, _) = cu.create_subkey(path).unwrap();
        k.set_value("PkTestA", &"\"C:\\x\\a.exe\" --go").unwrap();
        k.set_value("PkTestB", &1u32).unwrap(); // DWORD ignored
        let list = list_run_at(Hive::Cu, path, "HKCU Run");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "PkTestA");
        assert_eq!(list[0].command, "\"C:\\x\\a.exe\" --go");
        assert_eq!(remove_run_value(Hive::Cu, path, "PkTestA"), Ok("startup entry removed"));
        assert_eq!(remove_run_value(Hive::Cu, path, "PkTestA"), Ok("already gone"));
        assert!(list_run_at(Hive::Cu, path, "x").is_empty());
        assert_eq!(remove_run_value(Hive::Cu, r"Software\ProcessKillerTest\NoSuch\Run", "x"), Ok("already gone"));
        let _ = cu.delete_subkey_all(r"Software\ProcessKillerTest");
    }

    /// Read-only: lists real startup entries and tasks.
    #[test]
    #[ignore]
    fn smoke_list_startup() {
        let t = std::time::Instant::now();
        let runs = list_run_entries();
        let tasks = list_tasks();
        println!("{} run entries, {} tasks in {:?}", runs.len(), tasks.len(), t.elapsed());
        for r in runs.iter().take(5) {
            println!("{} | {} | {}", r.location, r.name, r.command);
        }
        for r in tasks.iter().take(5) {
            println!("{} | {} | {}", r.name, r.status, r.command);
        }
    }
}
