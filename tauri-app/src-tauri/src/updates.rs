// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Activity log reader, settings, About info and the self-updater (port of 1.x `updater.py`).
//!
//! Flow: `check_update` fetches the manifest (`latest.json`); `start_update` downloads the installer on a worker
//! thread (events `update:progress` / `update:done`), verifies SHA-256 (and the minisign signature when configured),
//! launches the Inno Setup installer silently and exits the app so the installer can replace it.
#![allow(dead_code)]
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;
use crate::util::{self, err, CmdResult};

/// Minisign public key (the base64 line of the `.pub` file, e.g. `RWQ...`) used to verify installers.
/// Generate a key pair once with rsign2 (`cargo install rsign2`, then `rsign generate -p pk.pub -s pk.key`),
/// keep `pk.key` SECRET (release machine only), paste the public key here and sign each installer
/// (`rsign sign -s pk.key ProcessKiller-Setup-X.exe`), putting the contents of the `.minisig` file in the
/// manifest as `"signature"`. To rotate: ship a release signed with the OLD key containing the NEW key.
/// While this is empty the signature check is skipped (SHA-256 is still enforced).
pub const UPDATE_PUBLIC_KEY: &str = "RWQVwkqO8SCX+h8YqjoGBDjdbkluvgPK3oHHwdxAIVAu1I0lxYXRg2UP";

const DEFAULT_SOURCE: &str = "github:mba-x86/Process-killer";
const REPO_URL: &str = "https://github.com/mba-x86/Process-killer";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const INSTALLER_ARGS: [&str; 6] = ["/SILENT", "/SUPPRESSMSGBOXES", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS", "/NORESTART", "/RELAUNCH=1"];
const OFFLINE_MSG: &str = "Could not reach the update server. Check your internet connection.";

// ------------------------------------------------------------------ settings

fn settings_path() -> PathBuf {
    util::data_dir().join("settings.json")
}

fn load_settings_from(path: &Path) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| if let Value::Object(m) = v { Some(m) } else { None })
        .unwrap_or_default()
}

/// Merge `changes` into the settings file, keeping every other key (1.x compatibility).
fn save_settings_to(path: &Path, changes: Map<String, Value>) -> Result<(), String> {
    let mut data = load_settings_from(path);
    // A non-empty file that is not a JSON object (damaged, hand-edited, or a future format) must not be overwritten
    // silently: keep a copy so the user's other keys are never lost.
    if data.is_empty() {
        if let Ok(old) = std::fs::read_to_string(path) {
            let ok = matches!(serde_json::from_str::<Value>(old.trim_start_matches('\u{feff}')), Ok(Value::Object(_)));
            if !ok && !old.trim().is_empty() {
                let _ = std::fs::copy(path, path.with_extension("json.bak"));
            }
        }
    }
    for (k, v) in changes {
        data.insert(k, v);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = serde_json::to_string_pretty(&Value::Object(data)).map_err(|e| e.to_string())?;
    // write-then-rename: a crash or full disk mid-write cannot leave a truncated settings.json behind
    let tmp = path.with_extension("json.tmp");
    let saved = std::fs::write(&tmp, &text).and_then(|_| std::fs::rename(&tmp, path));
    if saved.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return std::fs::write(path, text).map_err(|_| "Could not save the settings.".to_string());
    }
    Ok(())
}

fn auto_check_of(s: &Map<String, Value>) -> bool {
    s.get("auto_check").and_then(Value::as_bool).unwrap_or(true)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_settings() -> CmdResult<Value> {
    Ok(json!({ "auto_check": auto_check_of(&load_settings_from(&settings_path())) }))
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_settings(auto_check: bool) -> CmdResult<()> {
    let mut m = Map::new();
    m.insert("auto_check".into(), Value::Bool(auto_check));
    save_settings_to(&settings_path(), m).map_err(util::CmdError)
}

// ------------------------------------------------------------------ about

#[tauri::command(rename_all = "snake_case")]
pub fn get_about() -> CmdResult<Value> {
    Ok(json!({ "version": CURRENT_VERSION, "copyright": "\u{a9} 2026 mba-x86", "license": "MIT License", "repo_url": REPO_URL }))
}

/// True only for the project repo URL (or a page below it). Case-insensitive like GitHub itself.
fn is_repo_url(url: &str) -> bool {
    let u = url.trim();
    if u.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    if u.len() < REPO_URL.len() || !u.is_char_boundary(REPO_URL.len()) {
        return false;
    }
    if !u[..REPO_URL.len()].eq_ignore_ascii_case(REPO_URL) {
        return false;
    }
    matches!(u[REPO_URL.len()..].chars().next(), None | Some('/') | Some('?') | Some('#'))
}

fn shell_open(target: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let op = util::wide("open");
    let file = util::wide(target);
    unsafe {
        let r = ShellExecuteW(None, PCWSTR(op.as_ptr()), PCWSTR(file.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        r.0 as isize > 32
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn open_url(url: String) -> CmdResult<()> {
    if !is_repo_url(&url) {
        return err("Only the Task Reaper project page can be opened from here.");
    }
    if shell_open(url.trim()) {
        Ok(())
    } else {
        err("Could not open the web browser.")
    }
}

// ------------------------------------------------------------------ activity

#[derive(Serialize, Debug, PartialEq)]
pub struct LogEntry {
    pub ts: u64,
    pub level: String,
    pub message: String,
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_to_secs(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
    days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s
}

/// Seconds to add to a local wall-clock reading to get UTC (Win32: GetLocalTime vs GetSystemTime).
fn local_offset_secs() -> i64 {
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    let (l, u) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let lt = civil_to_secs(l.wYear as i64, l.wMonth as i64, l.wDay as i64, l.wHour as i64, l.wMinute as i64, l.wSecond as i64);
    let ut = civil_to_secs(u.wYear as i64, u.wMonth as i64, u.wDay as i64, u.wHour as i64, u.wMinute as i64, u.wSecond as i64);
    ut - lt
}

/// Parse `YYYY-MM-DD HH:MM:SS [level] message`. Returns (local-as-if-UTC seconds, level, message).
fn parse_line(line: &str) -> Option<(i64, String, String)> {
    let head = line.get(..19)?;
    let b = head.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| head.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, s) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let rest = line[19..].trim_start();
    let (level, msg) = match rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((lv, m)) => (lv.trim().to_ascii_lowercase(), m.trim().to_string()),
        None => ("info".to_string(), rest.trim().to_string()),
    };
    let level = match level.as_str() {
        "ok" | "success" => "ok",
        "fail" | "error" | "failed" => "fail",
        _ => "info",
    };
    Some((civil_to_secs(y, mo, d, h, mi, s), level.to_string(), msg))
}

/// Parse log text into entries (oldest first), keeping the last `limit`. Malformed lines become `info` entries
/// with the previous line's timestamp; blank lines are skipped. `offset` converts local to UTC seconds.
fn parse_log(text: &str, limit: usize, offset: i64) -> Vec<LogEntry> {
    let mut out: Vec<LogEntry> = Vec::new();
    let mut last_ts = 0u64;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if let Some((secs, level, message)) = parse_line(line) {
            last_ts = (secs + offset).max(0) as u64;
            out.push(LogEntry { ts: last_ts, level, message });
        } else {
            out.push(LogEntry { ts: last_ts, level: "info".into(), message: line.trim().to_string() });
        }
    }
    if out.len() > limit {
        out.drain(..out.len() - limit);
    }
    out
}

fn read_tail(path: &Path, max_bytes: u64) -> String {
    let Ok(mut f) = std::fs::File::open(path) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).into_owned();
    if start > 0 {
        // the first line is probably cut in half
        text.split_once('\n').map(|(_, r)| r.to_string()).unwrap_or_default()
    } else {
        text
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_activity(limit: Option<usize>) -> CmdResult<Value> {
    let limit = limit.unwrap_or(500).clamp(1, 5000);
    let text = read_tail(&crate::activity::log_path(), 2 * 1024 * 1024);
    Ok(json!({ "entries": parse_log(&text, limit, local_offset_secs()) }))
}

#[tauri::command(rename_all = "snake_case")]
pub fn clear_activity() -> CmdResult<()> {
    match std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(crate::activity::log_path()) {
        Ok(_) => Ok(()),
        Err(_) => err("Could not clear the activity log."),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn open_log_file() -> CmdResult<()> {
    let p = crate::activity::log_path();
    if !p.exists() && std::fs::write(&p, b"").is_err() {
        return err("Could not create the activity log file.");
    }
    if shell_open(&p.to_string_lossy()) {
        Ok(())
    } else {
        err("Could not open the log file.")
    }
}

// ------------------------------------------------------------------ manifest + URL policy

/// Up to 4 numeric parts, like updater.py `parse_version` ("2.0.0-dev" -> [2,0,0]).
pub fn parse_version(v: &str) -> Vec<u64> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    for c in v.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            cur.push(c);
        } else if !cur.is_empty() {
            parts.push(cur.parse::<u64>().unwrap_or(u64::MAX));
            cur.clear();
        }
    }
    parts.truncate(4);
    if parts.is_empty() {
        parts.push(0);
    }
    parts
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

/// https only; plain http for loopback in test builds only. No userinfo tricks.
pub fn url_allowed(url: &str) -> bool {
    let (scheme, rest) = match url.split_once("://") {
        Some(x) => x,
        None => return false,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let host = if let Some(r) = authority.strip_prefix('[') {
        r.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    match scheme.to_ascii_lowercase().as_str() {
        "https" => !host.is_empty(),
        "http" => cfg!(test) && matches!(host.to_ascii_lowercase().as_str(), "127.0.0.1" | "localhost" | "::1"),
        _ => false,
    }
}

/// `settings.json` is writable by any process of the user while this app runs elevated, so a custom `update_source`
/// is only honoured when installers are verified against the built-in minisign key (`honour_custom`).
fn manifest_url_from(settings: &Map<String, Value>, honour_custom: bool) -> String {
    let src = if honour_custom { settings.get("update_source").and_then(Value::as_str).unwrap_or("").trim() } else { "" };
    let src = if src.is_empty() { DEFAULT_SOURCE } else { src };
    match src.strip_prefix("github:") {
        Some(repo) => format!("https://github.com/{}/releases/latest/download/latest.json", repo.trim().trim_matches('/')),
        None => src.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub notes: String,
    pub signature: Option<String>,
    pub published: Option<u64>,
}

fn as_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// `YYYY-MM-DDTHH:MM:SS...` (UTC) -> unix seconds.
fn parse_iso(s: &str) -> Option<u64> {
    let h = s.get(..19)?;
    let n = |r: std::ops::Range<usize>| h.get(r)?.parse::<i64>().ok();
    let secs = civil_to_secs(n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    u64::try_from(secs).ok()
}

pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let bad = || "The update information from the server is not valid.".to_string();
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| bad())?;
    let o = v.as_object().ok_or_else(bad)?;
    let version = match o.get("version") {
        Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => return Err("The update manifest is missing the version.".into()),
    };
    let url = o
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("The update manifest is missing the download URL.")?;
    if !url_allowed(url) {
        return Err("Updates must be served over HTTPS.".into());
    }
    let published = o.get("published").and_then(|p| as_u64(p).or_else(|| p.as_str().and_then(parse_iso)));
    Ok(Manifest {
        version,
        url: url.to_string(),
        sha256: o.get("sha256").and_then(Value::as_str).unwrap_or("").trim().to_ascii_lowercase(),
        size: o.get("size").and_then(as_u64).unwrap_or(0),
        notes: o.get("notes").and_then(Value::as_str).unwrap_or("").to_string(),
        signature: o.get("signature").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from),
        published,
    })
}

// ------------------------------------------------------------------ HTTP

fn user_agent() -> String {
    format!("ProcessKiller/{CURRENT_VERSION}")
}

/// GET with the URL policy applied to the first request and to every redirect hop.
fn http_get(url: &str, total_timeout: Option<Duration>) -> Result<ureq::Response, String> {
    let mut b = ureq::AgentBuilder::new()
        .redirects(0)
        .user_agent(&user_agent())
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30));
    if let Some(t) = total_timeout {
        b = b.timeout(t);
    }
    let agent = b.build();
    let mut cur = url.to_string();
    for _ in 0..6 {
        if !url_allowed(&cur) {
            return Err("Updates must be served over HTTPS.".into());
        }
        match agent.get(&cur).call() {
            Ok(r) if (300..400).contains(&r.status()) => {
                cur = r.header("Location").ok_or("The update server sent an invalid redirect.")?.to_string();
            }
            Ok(r) => return Ok(r),
            Err(ureq::Error::Status(code, _)) => return Err(format!("The update server returned an error ({code}).")),
            Err(_) => return Err(OFFLINE_MSG.into()),
        }
    }
    Err("The update server redirected too many times.".into())
}

pub fn fetch_manifest(url: &str) -> Result<Manifest, String> {
    let resp = http_get(url, Some(Duration::from_secs(15)))?;
    let mut body = Vec::new();
    resp.into_reader().take(1024 * 1024).read_to_end(&mut body).map_err(|_| OFFLINE_MSG.to_string())?;
    parse_manifest(&String::from_utf8_lossy(&body))
}

// ------------------------------------------------------------------ verification

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Verify a minisign signature (`sig` = contents of the .minisig file, or its base64 encoding).
pub fn verify_signature(data: &[u8], sig: &str, public_key: &str) -> Result<(), String> {
    let bad = || "The update's signature is not valid.".to_string();
    let key = public_key.trim();
    let pk = if key.contains('\n') { minisign_verify::PublicKey::decode(key) } else { minisign_verify::PublicKey::from_base64(key) }
        .map_err(|_| "The built-in update key is invalid.".to_string())?;
    let text = if sig.contains("untrusted comment") {
        sig.to_string()
    } else {
        String::from_utf8(b64_decode(sig).ok_or_else(bad)?).map_err(|_| bad())?
    };
    let s = minisign_verify::Signature::decode(&text).map_err(|_| bad())?;
    pk.verify(data, &s, false).map_err(|_| bad())
}

// ------------------------------------------------------------------ download

/// Largest installer we will download when the manifest does not announce a size.
const MAX_INSTALLER_BYTES: u64 = 512 * 1024 * 1024;
/// The installer is always stored under this fixed name: the URL's file name is attacker-influenced text.
const INSTALLER_FILE: &str = "ProcessKiller-Setup.exe";

/// Download `m` into `dir`, verify checksum (+ signature when `public_key` is non-empty and the manifest has one).
/// The file is deleted on any failure. `progress(done, total)` is throttled to ~20/s.
pub fn download(m: &Manifest, dir: &Path, public_key: &str, cancel: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64, u64)) -> Result<PathBuf, String> {
    if m.sha256.len() != 64 || !m.sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("The update has no valid checksum, so it was not installed.".into());
    }
    if !public_key.trim().is_empty() && m.signature.is_none() {
        // once a key is built in, an unsigned manifest must never fall back to "checksum only"
        return Err("The update is not signed, so it was not installed.".into());
    }
    std::fs::create_dir_all(dir).map_err(|_| "Could not create a temporary folder for the update.".to_string())?;
    let path = dir.join(INSTALLER_FILE);
    let result = download_inner(m, &path, public_key, cancel, progress);
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    result.map(|_| path)
}

fn download_inner(m: &Manifest, path: &Path, public_key: &str, cancel: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let resp = http_get(&m.url, None)?;
    let cap = if m.size > 0 { m.size.min(MAX_INSTALLER_BYTES) } else { MAX_INSTALLER_BYTES };
    let announced = resp.header("Content-Length").and_then(|v| v.trim().parse::<u64>().ok());
    if announced.map_or(false, |n| n > cap) {
        return Err("The update file is larger than announced, so it was not downloaded.".into());
    }
    let total = announced.unwrap_or(m.size);
    let mut reader = resp.into_reader();
    let _ = std::fs::remove_file(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "Could not write the update to disk.".to_string())?;
    let mut sha = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut last) = (0u64, Instant::now() - Duration::from_secs(1));
    loop {
        if cancel() {
            return Err("Download cancelled.".into());
        }
        let n = reader.read(&mut buf).map_err(|_| "The download was interrupted. Check your internet connection.".to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|_| "Could not write the update to disk (is the disk full?).".to_string())?;
        sha.update(&buf[..n]);
        done += n as u64;
        if done > cap {
            return Err("The update file is larger than announced, so it was not downloaded.".into());
        }
        if last.elapsed() >= Duration::from_millis(50) {
            last = Instant::now();
            progress(done, total);
        }
    }
    file.flush().map_err(|_| "Could not write the update to disk.".to_string())?;
    drop(file);
    progress(done, if total == 0 { done } else { total });
    if hex(&sha.finalize()) != m.sha256 {
        return Err("The downloaded file is corrupted (checksum mismatch). Try again.".into());
    }
    if let (false, Some(sig)) = (public_key.trim().is_empty(), m.signature.as_deref()) {
        let data = std::fs::read(path).map_err(|_| "Could not verify the update.".to_string())?;
        verify_signature(&data, sig, public_key)?;
    }
    Ok(())
}

/// Re-open the verified installer so that nobody can modify or replace it between the check and the launch:
/// the handle denies other writers/deleters and the content is hashed again through that very handle.
/// Keep the returned `File` alive until the installer has been started.
pub fn lock_verified(path: &Path, sha256: &str) -> Result<std::fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 1;
    let bad = || "The downloaded update could not be verified.".to_string();
    let mut f = std::fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(path).map_err(|_| bad())?;
    let mut sha = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|_| bad())?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
    }
    if hex(&sha.finalize()) != sha256 {
        return Err("The downloaded file changed after it was checked, so it was not installed.".into());
    }
    Ok(f)
}

fn launch_installer(path: &Path) -> Result<(), String> {
    const DETACHED_PROCESS: u32 = 0x8;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;
    std::process::Command::new(path)
        .args(INSTALLER_ARGS)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not start the installer.".to_string())
}

// ------------------------------------------------------------------ commands

fn manifest_url() -> String {
    manifest_url_from(&load_settings_from(&settings_path()), !UPDATE_PUBLIC_KEY.trim().is_empty())
}

fn check_info(m: &Manifest) -> Value {
    let mut v = json!({
        "available": is_newer(&m.version, CURRENT_VERSION),
        "current": CURRENT_VERSION,
        "version": m.version,
        "notes": m.notes,
        "size": m.size,
    });
    if let Some(p) = m.published {
        v["published"] = json!(p);
    }
    v
}

#[tauri::command(rename_all = "snake_case")]
pub async fn check_update() -> CmdResult<Value> {
    let r = tauri::async_runtime::spawn_blocking(|| fetch_manifest(&manifest_url()).map(|m| check_info(&m))).await;
    match r {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => err(e),
        Err(_) => err("The update check failed unexpectedly."),
    }
}

/// Remove update folders left behind by earlier runs (older than a day; the running installer keeps its own copy locked).
fn clean_old_update_dirs() {
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in rd.flatten() {
        if !e.file_name().to_string_lossy().starts_with("pk_update_") {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map_or(false, |a| a > Duration::from_secs(24 * 3600));
        if old && e.file_type().map(|t| t.is_dir() && !t.is_symlink()).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

static UPDATE_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn run_update(app: &AppHandle, job: &str, cancel: &std::sync::atomic::AtomicBool) -> Result<(), String> {
    let m = fetch_manifest(&manifest_url())?;
    if !is_newer(&m.version, CURRENT_VERSION) {
        return Err("You already have the latest version.".into());
    }
    crate::activity::log(app, "info", format!("Downloading update {}", m.version));
    clean_old_update_dirs();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("pk_update_{}_{}", std::process::id(), nanos));
    // create_dir (not _all): fail if somebody pre-created the folder, so we never write into a folder we do not own
    std::fs::create_dir(&dir).map_err(|_| "Could not create a temporary folder for the update.".to_string())?;
    let path = download(
        &m,
        &dir,
        UPDATE_PUBLIC_KEY,
        &|| cancel.load(std::sync::atomic::Ordering::Relaxed),
        &mut |done, total| {
            let _ = app.emit("update:progress", json!({ "job": job, "done": done, "total": total }));
        },
    )?;
    let _locked = lock_verified(&path, &m.sha256)?;
    launch_installer(&path)?;
    crate::activity::log(app, "ok", format!("Installing update {}", m.version));
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn start_update(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Value> {
    // one update at a time: a second click (or window) must not download and launch a second installer
    if UPDATE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return err("An update is already being downloaded.");
    }
    let (job, cancel) = state.jobs.start();
    let (app2, job2) = (app.clone(), job.clone());
    std::thread::spawn(move || {
        let res = run_update(&app2, &job2, &cancel);
        if res.is_err() {
            UPDATE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        if let Some(st) = tauri::Manager::try_state::<AppState>(&app2) {
            st.jobs.finish(&job2);
        }
        match res {
            Ok(()) => {
                let _ = app2.emit("update:done", json!({ "job": job2, "ok": true, "message": "Installing..." }));
                std::thread::sleep(Duration::from_millis(1200));
                app2.exit(0);
            }
            Err(msg) => {
                crate::activity::log(&app2, "fail", format!("Update failed: {msg}"));
                let _ = app2.emit("update:done", json!({ "job": job2, "ok": false, "message": msg }));
            }
        }
    });
    Ok(json!({ "job": job }))
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn version_compare() {
        assert!(is_newer("2.0.0", "1.0.3"));
        assert!(is_newer("2.0.10", "2.0.0"));
        assert!(!is_newer("2.0.0", "2.0.0"));
        assert!(!is_newer("1.0.3", "2.0.0"));
        assert_eq!(parse_version("2.0.0-dev"), vec![2, 0, 0]);
        assert_eq!(parse_version("v1.2.3.4.5"), vec![1, 2, 3, 4]);
        assert_eq!(parse_version("junk"), vec![0]);
        assert!(is_newer("2.0.1", "2.0.0-dev"));
        assert!(!is_newer("junk", "0.0.1"));
    }

    #[test]
    fn url_policy() {
        assert!(url_allowed("https://github.com/x/y/latest.json"));
        assert!(url_allowed("http://127.0.0.1:8080/latest.json"));
        assert!(url_allowed("http://localhost/x"));
        assert!(url_allowed("http://[::1]:9/x"));
        assert!(!url_allowed("http://example.com/latest.json"));
        assert!(!url_allowed("ftp://example.com/x"));
        assert!(!url_allowed("file:///c:/x.exe"));
        assert!(!url_allowed("https://"));
        assert!(!url_allowed("http://127.0.0.1@evil.com/x"));
        assert!(!url_allowed("http://localhost.evil.com/x"));
        assert!(!url_allowed("no-scheme"));
    }

    #[test]
    fn repo_url_only() {
        assert!(is_repo_url("https://github.com/mba-x86/Process-killer"));
        assert!(is_repo_url("https://github.com/mba-x86/Process-killer/releases"));
        assert!(is_repo_url("https://github.com/mba-x86/process-killer"));
        assert!(!is_repo_url("https://github.com/mba-x86/Process-killer-evil"));
        assert!(!is_repo_url("https://github.com/mba-x86/Process-killer.evil.com"));
        assert!(!is_repo_url("https://evil.com/?https://github.com/mba-x86/Process-killer"));
        assert!(!is_repo_url("file:///c:/windows/system32/calc.exe"));
        assert!(!is_repo_url(""));
    }

    #[test]
    fn manifest_source() {
        let mut s = Map::new();
        let default = "https://github.com/mba-x86/Process-killer/releases/latest/download/latest.json";
        assert_eq!(manifest_url_from(&s, true), default);
        s.insert("update_source".into(), json!("github:foo/bar/"));
        assert_eq!(manifest_url_from(&s, true), "https://github.com/foo/bar/releases/latest/download/latest.json");
        s.insert("update_source".into(), json!("https://example.com/latest.json"));
        assert_eq!(manifest_url_from(&s, true), "https://example.com/latest.json");
        // without a built-in signing key a settings.json override is ignored (it would be an elevated-code-execution hole)
        assert_eq!(manifest_url_from(&s, false), default);

    }

    #[test]
    fn manifest_parsing() {
        let m = parse_manifest(r#"{"version":"2.0.1","url":"https://x.test/s.exe","sha256":"ABCD","size":123,"notes":"New: a","extra":{"a":1},"signature":"sig","published":"2026-10-01T00:00:00Z"}"#).unwrap();
        assert_eq!((m.version.as_str(), m.size, m.sha256.as_str()), ("2.0.1", 123, "abcd"));
        assert_eq!(m.signature.as_deref(), Some("sig"));
        assert_eq!(m.published, Some(civil_to_secs(2026, 10, 1, 0, 0, 0) as u64));
        assert!(parse_manifest("\u{feff}{\"version\":\"1\",\"url\":\"https://x/y\"}").is_ok());
        assert!(parse_manifest("{\"url\":\"https://x/y\"}").is_err());
        assert!(parse_manifest("{\"version\":\"1\"}").is_err());
        assert!(parse_manifest("{\"version\":\"1\",\"url\":\"http://evil.com/y\"}").is_err());
        assert!(parse_manifest("not json").is_err());
        assert!(parse_manifest("[1]").is_err());
    }

    #[test]
    fn sha256_known() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn signature_rejects_garbage() {
        let key = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        assert!(verify_signature(b"data", "bm90IGEgc2ln", key).is_err());
        assert!(verify_signature(b"data", "untrusted comment: x\nRUQ=\n", key).is_err());
        assert!(verify_signature(b"data", "!!!", key).is_err());
    }

    #[test]
    fn log_parsing() {
        let text = "2026-10-19 00:48:11 [ok] Cleaned 3 MB\n\n2026-10-19 00:48:12 [fail] Boom\ngarbage line\n2026-10-19 00:48:13 [info] Slow task: X took 3.3s\n2026-10-19 00:48:14 no level\n2026-13-45 99:99:99 [ok] bad";
        let e = parse_log(text, 100, 0);
        let t0 = civil_to_secs(2026, 10, 19, 0, 48, 11) as u64;
        assert_eq!(e.len(), 6);
        assert_eq!(e[0], LogEntry { ts: t0, level: "ok".into(), message: "Cleaned 3 MB".into() });
        assert_eq!(e[1].level, "fail");
        assert_eq!((e[2].level.as_str(), e[2].ts, e[2].message.as_str()), ("info", e[1].ts, "garbage line"));
        assert_eq!(e[3].message, "Slow task: X took 3.3s");
        assert_eq!((e[4].level.as_str(), e[4].message.as_str()), ("info", "no level"));
        assert_eq!(parse_log(text, 2, 3600).len(), 2);
        assert_eq!(parse_log("2026-10-19 00:48:11 [ok] x", 5, 3600)[0].ts, t0 + 3600);
        assert_eq!(civil_to_secs(1970, 1, 2, 0, 0, 0), 86400);
    }

    #[test]
    fn settings_roundtrip_keeps_unknown_keys() {
        let dir = std::env::temp_dir().join(format!("pk_set_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("settings.json");
        assert!(auto_check_of(&load_settings_from(&p)));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&p, "{\"update_source\":\"github:a/b\",\"custom\":{\"x\":[1,2]},\"auto_check\":true}").unwrap();
        let mut ch = Map::new();
        ch.insert("auto_check".into(), json!(false));
        save_settings_to(&p, ch).unwrap();
        let s = load_settings_from(&p);
        assert!(!auto_check_of(&s));
        assert_eq!(s["update_source"], "github:a/b");
        assert_eq!(s["custom"]["x"][1], 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Tiny HTTP server: /latest.json, /redir -> /latest.json, /setup.exe (payload), anything else 404.
    fn serve(payload: Vec<u8>, sha: String) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
        let b2 = base.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let (payload, sha, base) = (payload.clone(), sha.clone(), b2.clone());
                std::thread::spawn(move || {
                    let mut s = s;
                    let mut line = String::new();
                    let mut rd = BufReader::new(s.try_clone().unwrap());
                    rd.read_line(&mut line).unwrap_or(0);
                    let mut h = String::new();
                    while rd.read_line(&mut h).unwrap_or(0) > 2 {
                        h.clear();
                    }
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let (head, body): (String, Vec<u8>) = match path.as_str() {
                        "/latest.json" => {
                            let j = format!(
                                "{{\"version\":\"99.0.0\",\"url\":\"{base}/setup.exe\",\"sha256\":\"{sha}\",\"size\":{},\"notes\":\"New: x\",\"future_field\":true}}",
                                payload.len()
                            );
                            ("200 OK".into(), j.into_bytes())
                        }
                        "/redir" => (format!("302 Found\r\nLocation: {base}/latest.json"), vec![]),
                        "/setup.exe" => ("200 OK".into(), payload),
                        _ => ("404 Not Found".into(), vec![]),
                    };
                    let _ = write!(s, "HTTP/1.1 {head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = s.write_all(&body);
                });
            }
        });
        base
    }

    #[test]
    fn mock_server_check_and_download() {
        let payload: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect();
        let base = serve(payload.clone(), sha256_hex(&payload));

        let m = fetch_manifest(&format!("{base}/latest.json")).unwrap();
        assert!(is_newer(&m.version, CURRENT_VERSION));
        assert_eq!(check_info(&m)["available"], true);
        // redirect is followed (policy re-checked on each hop)
        assert_eq!(fetch_manifest(&format!("{base}/redir")).unwrap().version, "99.0.0");
        // 404 and offline give friendly errors
        assert!(fetch_manifest(&format!("{base}/nope")).unwrap_err().contains("404"));
        assert_eq!(fetch_manifest("http://127.0.0.1:1/latest.json").unwrap_err(), OFFLINE_MSG);
        assert!(fetch_manifest("http://example.com/latest.json").is_err());

        let dir = std::env::temp_dir().join(format!("pk_dl_test_{}", std::process::id()));
        let mut events = Vec::new();
        let p = download(&m, &dir, "", &|| false, &mut |d, t| events.push((d, t))).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), payload);
        assert_eq!(events.last().copied(), Some((700_000, 700_000)));
        assert!(events.windows(2).all(|w| w[0].0 <= w[1].0));

        // wrong checksum -> file deleted + error
        let mut bad = m.clone();
        bad.sha256 = "0".repeat(64);
        let e = download(&bad, &dir, "", &|| false, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("checksum"));
        assert!(!dir.join(INSTALLER_FILE).exists());
        // missing checksum is refused
        bad.sha256.clear();
        assert!(download(&bad, &dir, "", &|| false, &mut |_, _| {}).is_err());
        // bad signature with a key configured -> deleted + error
        let mut sg = m.clone();
        sg.signature = Some("garbage".into());
        let e = download(&sg, &dir, "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3", &|| false, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("signature"));
        assert!(!dir.join(INSTALLER_FILE).exists());
        // cancel
        let flag = AtomicBool::new(true);
        assert!(download(&m, &dir, "", &|| flag.load(Ordering::Relaxed), &mut |_, _| {}).unwrap_err().contains("cancelled"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn caps_signature_and_lock() {
        let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 241) as u8).collect();
        let base = serve(payload.clone(), sha256_hex(&payload));
        let m = fetch_manifest(&format!("{base}/latest.json")).unwrap();
        let dir = std::env::temp_dir().join(format!("pk_cap_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // announced size smaller than what the server sends -> refused, nothing left on disk
        let mut small = m.clone();
        small.size = 1000;
        let e = download(&small, &dir, "", &|| false, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("larger than announced"), "{e}");
        assert!(!dir.join(INSTALLER_FILE).exists());

        // a key is configured but the manifest carries no signature -> hard failure (no checksum-only fallback)
        let key = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        let mut unsigned = m.clone();
        unsigned.signature = None;
        let e = download(&unsigned, &dir, key, &|| false, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("not signed"), "{e}");

        // happy path, then the lock: content is re-hashed and nobody can write to / delete the file meanwhile
        let p = download(&m, &dir, "", &|| false, &mut |_, _| {}).unwrap();
        assert_eq!(p.file_name().unwrap(), INSTALLER_FILE);
        assert!(lock_verified(&p, &"0".repeat(64)).is_err());
        let held = lock_verified(&p, &m.sha256).unwrap();
        assert!(std::fs::OpenOptions::new().write(true).open(&p).is_err(), "writers must be locked out");
        assert!(std::fs::remove_file(&p).is_err(), "deleting must be locked out");
        drop(held);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_settings_are_kept() {
        let dir = std::env::temp_dir().join(format!("pk_set_bak_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        std::fs::write(&p, "{\"update_source\": \"github:a/b\", \"x\": [1, 2,").unwrap(); // truncated
        let mut ch = Map::new();
        ch.insert("auto_check".into(), json!(false));
        save_settings_to(&p, ch).unwrap();
        let bak = std::fs::read_to_string(dir.join("settings.json.bak")).unwrap();
        assert!(bak.contains("github:a/b"));
        assert!(!auto_check_of(&load_settings_from(&p)));
        // a BOM-prefixed 1.x file is read and its unknown keys survive
        std::fs::write(&p, "\u{feff}{\"auto_check\": true, \"legacy\": {\"k\": 1}}").unwrap();
        let mut ch = Map::new();
        ch.insert("auto_check".into(), json!(false));
        save_settings_to(&p, ch).unwrap();
        assert_eq!(load_settings_from(&p)["legacy"]["k"], 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod signing_tests {
    use super::*;

    /// The shipped public key must accept a signature made by `rsign sign` with the owner's secret key
    /// (testdata/sample.txt.minisig) and reject any change to the data.
    #[test]
    fn real_public_key_verifies_a_real_rsign_signature() {
        assert!(!UPDATE_PUBLIC_KEY.trim().is_empty(), "release builds must ship an update key");
        let data = include_bytes!("../testdata/sample.txt");
        let sig = include_str!("../testdata/sample.txt.minisig");
        verify_signature(data, sig, UPDATE_PUBLIC_KEY).expect("a signature made by rsign must verify");
        let mut tampered = data.to_vec();
        tampered[0] ^= 1;
        assert!(verify_signature(&tampered, sig, UPDATE_PUBLIC_KEY).is_err(), "tampered data must be rejected");
        assert!(verify_signature(data, "garbage", UPDATE_PUBLIC_KEY).is_err(), "a garbage signature must be rejected");
    }
}
