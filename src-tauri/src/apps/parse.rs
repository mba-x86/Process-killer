// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Pure string logic ported from core.py (no I/O, easy to unit-test): uninstall-string parsing, name cleaning,
//! install-location safety, MSI command rewrite, task-XML / CSV parsing.

use crate::util::win_dir;

/// `%VAR%` expansion like `os.path.expandvars` on Windows (unknown variables are left untouched).
pub fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(0) => {
                out.push('%');
                rest = &after[1..];
            }
            Some(j) => {
                let var = &after[..j];
                match std::env::var(var) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(var);
                        out.push('%');
                    }
                }
                rest = &after[j + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Executable named at the start of an uninstall command. Handles quoted paths and the common unquoted
/// `C:\Program Files (x86)\App\unins000.exe /SILENT` form (path contains spaces).
pub fn exe_of(cmd: &str) -> String {
    let c = expand_env(cmd);
    let t = c.trim_start();
    // 1) "quoted path"
    if let Some(r) = t.strip_prefix('"') {
        if let Some(e) = r.find('"') {
            if e > 0 {
                return r[..e].to_string();
            }
        }
    }
    let lower = t.to_ascii_lowercase();
    // 2) X:\anything.exe followed by whitespace or end
    let b = t.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
        let mut from = 3;
        while let Some(p) = lower[from..].find(".exe") {
            let s = from + p;
            let end = s + 4;
            if s >= 4 && t[end..].chars().next().map_or(true, |c| c.is_whitespace()) {
                return t[..end].to_string();
            }
            from = s + 1;
        }
    }
    // 3) first whitespace-delimited token ending in .exe
    let tok_end = t.find(char::is_whitespace).unwrap_or(t.len());
    if let Some(s) = lower[..tok_end].find(".exe") {
        if s >= 1 {
            return t[..s + 4].to_string();
        }
    }
    String::new()
}

/// Directory part of a Windows path (`""` when there is none).
pub fn dirname(p: &str) -> String {
    match p.rfind(|c| c == '\\' || c == '/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Remove `\bv?\d+(\.\d+)+\S*` tokens (version numbers) from `s`.
fn strip_versions(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let boundary_ok = i == 0 || !is_word(chars[i - 1]);
        if boundary_ok && (chars[i].is_ascii_digit() || chars[i] == 'v') {
            let mut j = i;
            if chars[j] == 'v' {
                j += 1;
            }
            let ds = j;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j > ds {
                let mut groups = 0;
                loop {
                    if j + 1 < chars.len() && chars[j] == '.' && chars[j + 1].is_ascii_digit() {
                        j += 1;
                        while j < chars.len() && chars[j].is_ascii_digit() {
                            j += 1;
                        }
                        groups += 1;
                    } else {
                        break;
                    }
                }
                if groups > 0 {
                    while j < chars.len() && !chars[j].is_whitespace() {
                        j += 1;
                    }
                    out.push(' ');
                    i = j;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Remove whole-word (case-insensitive) occurrences of `word`.
fn strip_word_ci(s: &str, word: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let w: Vec<char> = word.to_lowercase().chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + w.len() <= chars.len()
            && (i == 0 || !is_word(chars[i - 1]))
            && chars[i..i + w.len()].iter().flat_map(|c| c.to_lowercase()).eq(w.iter().cloned())
            && (i + w.len() == chars.len() || !is_word(chars[i + w.len()]))
        {
            out.push(' ');
            i += w.len();
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 'Python 3.14.7 (64-bit)' -> 'Python'. `""` when too short to match safely.
pub fn core_name(name: &str) -> String {
    // 1) drop (...) groups
    let mut n = String::new();
    let mut depth_open = false;
    let chars: Vec<char> = name.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '(' {
            if let Some(off) = chars[i..].iter().position(|&c| c == ')') {
                n.push(' ');
                i += off + 1;
                continue;
            }
            depth_open = true;
        }
        n.push(chars[i]);
        i += 1;
    }
    let _ = depth_open;
    // 2) versions, 3) arch words
    let mut n = strip_versions(&n);
    for w in ["x64", "x86", "64-bit", "32-bit", "version"] {
        n = strip_word_ci(&n, w);
    }
    let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = n.trim_matches(|c| c == ' ' || c == '-' || c == '_').to_string();
    if n.chars().count() >= 4 {
        n
    } else {
        String::new()
    }
}

/// Registry display names (Microsoft Edge!) can carry invisible direction marks; strip them like Python's `isprintable`.
pub fn clean_name(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            if c == ' ' {
                return true;
            }
            if c.is_control() || c.is_whitespace() {
                return false;
            }
            !matches!(c as u32,
                0x00AD | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E
                | 0x2060..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB | 0xE000..=0xF8FF)
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn normalise_path(p: &str) -> String {
    let p = p.replace('/', "\\");
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('\\') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            // Win32 ignores trailing dots/spaces: "C:\Windows." is "C:\Windows"
            s => {
                let t = s.trim_end_matches(['.', ' ']);
                if t.is_empty() {
                    continue;
                }
                parts.push(t)
            }
        }
    }
    parts.join("\\")
}

/// The long (non-8.3) spelling of an existing path, or None when it does not exist / cannot be resolved.
pub fn long_path(p: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetLongPathNameW;
    let w = crate::util::wide(p);
    unsafe {
        let n = GetLongPathNameW(PCWSTR(w.as_ptr()), None) as usize;
        if n == 0 || n > 32768 {
            return None;
        }
        let mut buf = vec![0u16; n + 1];
        let m = GetLongPathNameW(PCWSTR(w.as_ptr()), Some(&mut buf)) as usize;
        if m == 0 || m > n {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..m]))
    }
}

fn broad_roots() -> Vec<String> {
    let mut v = Vec::new();
    for k in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData", "LOCALAPPDATA", "APPDATA", "USERPROFILE"] {
        if let Ok(x) = std::env::var(k) {
            v.push(normalise_path(&x).to_lowercase());
        }
    }
    if let Ok(x) = std::env::var("LOCALAPPDATA") {
        v.push(format!("{}\\programs", normalise_path(&x).to_lowercase()));
    }
    v
}

/// Normalised lower-case install folder, or None if it is too broad to trust (drive root, Windows folder, Program Files...).
pub fn safe_location(loc: &str) -> Option<String> {
    let loc = loc.trim().trim_matches('"');
    if loc.is_empty() {
        return None;
    }
    let mut p = normalise_path(loc).to_lowercase();
    // 8.3 short names ("C:\Users\BOB~1") would dodge the broad-folder comparison below: compare the long spelling
    if let Some(l) = long_path(&p) {
        p = normalise_path(&l).to_lowercase();
    }
    let b = p.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' {
        return None;
    }
    let win = win_dir();
    if p == win || p.starts_with(&format!("{win}\\")) {
        return None;
    }
    if p.split('\\').count() < 3 || broad_roots().contains(&p) {
        return None;
    }
    Some(p)
}

/// `msiexec /I{GUID}` -> `/X{GUID}`; silent adds `/qn /norestart` unless a /q switch is already there.
pub fn rewrite_msi(cmd: &str, silent: bool) -> String {
    let lower = cmd.to_ascii_lowercase();
    if !lower.contains("msiexec") {
        return cmd.to_string();
    }
    let b = cmd.as_bytes();
    let mut out = String::with_capacity(cmd.len() + 16);
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && (b[i + 1] == b'i' || b[i + 1] == b'I') {
            let mut j = i + 2;
            while j < b.len() && (b[j] as char).is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'{' {
                out.push_str("/X");
                i += 2;
                continue;
            }
        }
        // copy one full char
        let Some(ch) = cmd[i..].chars().next() else { break };
        out.push(ch);
        i += ch.len_utf8();
    }
    if silent && !out.to_ascii_lowercase().contains("/q") {
        out.push_str(" /qn /norestart");
    }
    out
}

/// Is this path (an uninstall command's executable) one of the "no file to check" launchers?
pub fn is_launcher_exe(exe: &str) -> bool {
    let l = exe.to_lowercase();
    ["msiexec", "rundll32", "cmd.exe", "powershell", "explorer.exe", "ms-settings"].iter().any(|k| l.contains(k))
}

pub fn is_abs(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

/// `YYYYMMDD` -> `YYYY-MM-DD`; "" stays ""; anything unparsable -> "Unknown".
pub fn normalise_date(d: &str) -> String {
    let d = d.trim();
    if d.is_empty() {
        return String::new();
    }
    if d.len() == 8 && d.bytes().all(|c| c.is_ascii_digit()) {
        let (y, m, day): (u32, u32, u32) = (d[..4].parse().unwrap_or(0), d[4..6].parse().unwrap_or(0), d[6..].parse().unwrap_or(0));
        if (1980..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&day) {
            return format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]);
        }
    }
    "Unknown".to_string()
}

/// Folder name that is safe in a file name: runs of non `[\w.-]` become `_`, max 60 chars.
pub fn file_stem(name: &str) -> String {
    let mut s = String::new();
    let mut last_us = false;
    for c in name.chars() {
        if c.is_alphanumeric() || c == '_' || c == '.' || c == '-' {
            s.push(c);
            last_us = false;
        } else if !last_us {
            s.push('_');
            last_us = true;
        }
    }
    s.chars().take(60).collect()
}

pub fn fmt_bytes(b: u64) -> String {
    if b == 0 {
        return String::new();
    }
    let kb = b / 1024;
    let mb = kb as f64 / 1024.0;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else if mb >= 1.0 {
        format!("{:.0} MB", mb)
    } else {
        format!("{kb} KB")
    }
}

// ------------------------------------------------------------------ scheduled tasks

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let a = s.find(open)? + open.len();
    let b = s[a..].find(close)? + a;
    Some(&s[a..b])
}

pub fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Decode a task definition file (UTF-16 with BOM is what Windows writes).
pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let u: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        return String::from_utf16_lossy(&u);
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(b).into_owned()
}

/// (command lines joined with " | ", enabled)
pub fn parse_task_xml(xml: &str) -> (String, bool) {
    let mut cmds = Vec::new();
    let mut rest = xml;
    while let Some(a) = rest.find("<Exec>") {
        let after = &rest[a + 6..];
        let end = after.find("</Exec>").unwrap_or(after.len());
        let blk = &after[..end];
        let c = between(blk, "<Command>", "</Command>").map(xml_unescape).unwrap_or_default();
        let ar = between(blk, "<Arguments>", "</Arguments>").map(xml_unescape).unwrap_or_default();
        cmds.push(format!("{} {}", c.trim(), ar.trim()).trim().to_string());
        rest = &after[end..];
    }
    let enabled = between(xml, "<Settings>", "</Settings>")
        .and_then(|s| between(s, "<Enabled>", "</Enabled>"))
        .map_or(true, |v| v.trim().to_lowercase() != "false");
    (cmds.join(" | "), enabled)
}

/// Minimal CSV reader (quoted fields, doubled quotes, newlines inside quotes).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut inq = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if inq {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    inq = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => inq = true,
                ',' => row.push(std::mem::take(&mut field)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_quoted() {
        assert_eq!(exe_of(r#""C:\Program Files\App\unins000.exe" /SILENT"#), r"C:\Program Files\App\unins000.exe");
    }
    #[test]
    fn exe_unquoted_with_spaces() {
        assert_eq!(exe_of(r"C:\Program Files (x86)\My App\unins000.exe /VERYSILENT /NORESTART"), r"C:\Program Files (x86)\My App\unins000.exe");
        assert_eq!(exe_of(r"C:\Prog Files\a.exe"), r"C:\Prog Files\a.exe");
        // an ".exe" inside a folder name that is not followed by whitespace is skipped
        assert_eq!(exe_of(r"C:\a.exeb\x\u.exe -x"), r"C:\a.exeb\x\u.exe");
    }
    #[test]
    fn exe_bare_and_msi() {
        assert_eq!(exe_of("MsiExec.exe /I{ABC}"), "MsiExec.exe");
        assert_eq!(exe_of(r"rundll32.exe shell32.dll,Foo"), "rundll32.exe");
        assert_eq!(exe_of(""), "");
        assert_eq!(exe_of("something without extension"), "");
    }
    #[test]
    fn exe_env() {
        std::env::set_var("PK_TEST_DIR", r"C:\Tools");
        assert_eq!(exe_of(r"%PK_TEST_DIR%\u.exe /S"), r"C:\Tools\u.exe");
        assert_eq!(expand_env("%NOPE_PK%\\x"), "%NOPE_PK%\\x");
    }
    #[test]
    fn msi_rewrite() {
        assert_eq!(rewrite_msi("MsiExec.exe /I{1234-ABCD}", false), "MsiExec.exe /X{1234-ABCD}");
        assert_eq!(rewrite_msi("msiexec /i {1234}", false), "msiexec /X {1234}");
        assert_eq!(rewrite_msi("MsiExec.exe /X{1234}", true), "MsiExec.exe /X{1234} /qn /norestart");
        assert_eq!(rewrite_msi("MsiExec.exe /X{1234} /quiet", true), "MsiExec.exe /X{1234} /quiet");
        assert_eq!(rewrite_msi(r"C:\x\unins000.exe /I{1}", true), r"C:\x\unins000.exe /I{1}");
        assert_eq!(rewrite_msi("msiexec /install foo", false), "msiexec /install foo");
    }
    #[test]
    fn core_names() {
        assert_eq!(core_name("Python 3.14.7 (64-bit)"), "Python");
        assert_eq!(core_name("7-Zip 25.01 (x64)"), "7-Zip");
        assert_eq!(core_name("GBT_VGA 26.08.24.01"), "GBT_VGA");
        assert_eq!(core_name("Microsoft Edge"), "Microsoft Edge");
        assert_eq!(core_name("WinRAR 7.13 (64-bit)"), "WinRAR");
        assert_eq!(core_name("Git"), "");
        assert_eq!(core_name("Zoom Workplace (64-bit)"), "Zoom Workplace");
        assert_eq!(core_name("Java 8 Update 461"), "Java 8 Update 461");
        assert_eq!(core_name("Foo v2.0.1-beta Bar"), "Foo Bar");
        assert_eq!(core_name("Tool x64 version"), "Tool");
        assert_eq!(core_name("Windows SDK 10.0.26100.1 x86"), "Windows SDK");
    }
    #[test]
    fn version_agnostic_cleaning() {
        // the same product at different versions has the same core name
        assert_eq!(core_name("Python 3.13.7 (64-bit)"), core_name("Python 3.14.0 (64-bit)"));
        assert_eq!(core_name("CnCNet Yuri's Revenge 9.3.2"), core_name("CnCNet Yuri's Revenge 9.4.0"));
    }
    #[test]
    fn invisible_marks() {
        assert_eq!(clean_name("\u{200e}Microsoft Edge\u{200f}"), "Microsoft Edge");
        assert_eq!(clean_name("  Mi\u{202a}cro\u{feff}soft\u{a0}Edge "), "MicrosoftEdge");
        assert_eq!(clean_name("Battlefield\u{2122} 6"), "Battlefield\u{2122} 6");
    }
    #[test]
    fn safe_locations() {
        assert!(safe_location("").is_none());
        assert!(safe_location(r"C:\").is_none());
        assert!(safe_location(r"C:\Windows").is_none());
        assert!(safe_location(r"C:\Windows\System32\foo").is_none());
        assert!(safe_location(r"C:\Foo").is_none()); // only two components
        assert!(safe_location(r"C:\Program Files\..\Windows\x").is_none());
        let pf = std::env::var("ProgramFiles").unwrap_or(r"C:\Program Files".into());
        assert!(safe_location(&pf).is_none());
        assert!(safe_location(&format!("{pf}\\")).is_none());
        assert_eq!(safe_location(&format!("\"{pf}\\Brave\\\"")).unwrap(), format!("{}\\brave", pf.to_lowercase()));
        if let Ok(up) = std::env::var("USERPROFILE") {
            assert!(safe_location(&up).is_none());
        }
        if let Ok(l) = std::env::var("LOCALAPPDATA") {
            assert!(safe_location(&l).is_none());
            assert!(safe_location(&format!("{l}\\Programs")).is_none());
            assert!(safe_location(&format!("{l}\\Programs\\Foo")).is_some());
        }
        assert!(safe_location("relative\\path\\here").is_none());
    }
    #[test]
    fn launchers() {
        assert!(is_launcher_exe("C:\\Windows\\System32\\MsiExec.exe"));
        assert!(is_launcher_exe("rundll32.exe"));
        assert!(!is_launcher_exe(r"C:\x\unins000.exe"));
    }
    #[test]
    fn dates() {
        assert_eq!(normalise_date("20261016"), "2026-10-16");
        assert_eq!(normalise_date(""), "");
        assert_eq!(normalise_date("00000000"), "Unknown");
        assert_eq!(normalise_date("garbage"), "Unknown");
    }
    #[test]
    fn stems() {
        assert_eq!(file_stem("Men of War: Assault Squad 2"), "Men_of_War_Assault_Squad_2");
        assert_eq!(file_stem("a/b\\c"), "a_b_c");
    }
    #[test]
    fn sizes() {
        assert_eq!(fmt_bytes(0), "");
        assert_eq!(fmt_bytes(412 * 1024 * 1024), "412 MB");
        assert_eq!(fmt_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(fmt_bytes(10 * 1024), "10 KB");
    }
    #[test]
    fn task_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-16"?><Task><Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>
<Settings><Enabled>false</Enabled></Settings><Actions Context="Author"><Exec><Command>"C:\Program Files\Brave &amp; Co\update.exe"</Command><Arguments>/ua /installsource scheduler</Arguments></Exec><Exec><Command>b.exe</Command></Exec></Actions></Task>"#;
        let (cmd, en) = parse_task_xml(xml);
        assert_eq!(cmd, r#""C:\Program Files\Brave & Co\update.exe" /ua /installsource scheduler | b.exe"#);
        assert!(!en);
        assert!(parse_task_xml("<Task><Settings></Settings></Task>").1);
    }
    #[test]
    fn utf16_decode() {
        let mut b = vec![0xFF, 0xFE];
        for u in "<Task>é</Task>".encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_text(&b), "<Task>é</Task>");
        assert_eq!(decode_text(b"\xEF\xBB\xBFabc"), "abc");
    }
    #[test]
    fn csv() {
        let rows = parse_csv("\"a\",\"b,c\",\"d \"\"q\"\"\"\r\n\"x\",\"y\nz\",\"\"\r\n");
        assert_eq!(rows, vec![vec!["a", "b,c", "d \"q\""], vec!["x", "y\nz", ""]]);
    }
}
