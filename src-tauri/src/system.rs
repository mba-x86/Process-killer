// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! system: live gauges, drives, top processes/apps, clipboard (SHARED.md), dashboard specs (dashboard.md)
//! and CPU detail (processor.md). Behaviour ported from the 1.x `sysinfo.py`.
#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::State;
use windows::core::PCWSTR;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

use crate::state::AppState;
use crate::util::{self, CmdResult};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ------------------------------------------------------------------ small helpers

/// Port of sysinfo.fmt_size: "15.8 GB", "512 MB", "2 TB".
pub fn fmt_size(bytes: f64) -> String {
    let mut b = bytes.max(0.0);
    for unit in ["B", "KB", "MB", "GB"] {
        if b < 1024.0 {
            return if unit == "B" { format!("{b:.0} B") } else { format!("{b:.1} {unit}").replace(".0 ", " ") };
        }
        b /= 1024.0;
    }
    format!("{b:.2} TB").replace(".00 ", " ")
}

/// Strip invisible direction marks / BOMs / control characters that WMI sometimes prepends.
pub fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(*c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn reg_open(path: &str) -> Option<RegKey> {
    RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(path, KEY_READ).ok()
}
fn reg_str(path: &str, name: &str) -> String {
    reg_open(path).and_then(|k| k.get_value::<String, _>(name).ok()).map(|s| clean(&s)).unwrap_or_default()
}
fn reg_u32(path: &str, name: &str) -> Option<u32> {
    reg_open(path).and_then(|k| k.get_value::<u32, _>(name).ok())
}

fn spawn_blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = CmdResult<T>> {
    let h = tauri::async_runtime::spawn_blocking(f);
    async move { h.await.map_err(|_| util::CmdError("The background task failed.".into())) }
}

// ------------------------------------------------------------------ CPU topology (P/E split, caches)

#[derive(Clone, Debug, Default)]
pub struct Topology {
    pub physical: u32,
    /// (logical id, 'P' | 'E'), sorted by id.
    pub cores: Vec<(u32, char)>,
    pub cores_p: u32,
    pub cores_e: u32,
    pub l2: u64,
    pub l3: u64,
}

/// Pure part: `cores` = one entry per physical core (efficiency class, logical ids). The highest class is P; if all
/// classes are equal the CPU is not hybrid and everything is P.
pub fn classify(phys: &[(u8, Vec<u32>)]) -> (Vec<(u32, char)>, u32, u32) {
    let max = phys.iter().map(|p| p.0).max().unwrap_or(0);
    let min = phys.iter().map(|p| p.0).min().unwrap_or(0);
    let hybrid = max != min;
    let (mut cp, mut ce) = (0u32, 0u32);
    let mut logical = Vec::new();
    for (class, ids) in phys {
        let kind = if hybrid && *class < max { 'E' } else { 'P' };
        if kind == 'P' {
            cp += 1
        } else {
            ce += 1
        }
        for id in ids {
            logical.push((*id, kind));
        }
    }
    logical.sort_by_key(|x| x.0);
    (logical, cp, ce)
}

fn read_topology() -> Topology {
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationAll, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };
    let mut len = 0u32;
    unsafe {
        let _ = GetLogicalProcessorInformationEx(RelationAll, None, &mut len);
    }
    if len == 0 {
        return fallback_topology();
    }
    // u64-backed so the records are 8-byte aligned (a Vec<u8> only guarantees 1)
    let mut buf = vec![0u64; (len as usize + 16).div_ceil(8)];
    let ok = unsafe {
        GetLogicalProcessorInformationEx(RelationAll, Some(buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX), &mut len)
    };
    if ok.is_err() {
        return fallback_topology();
    }
    let mut phys: Vec<(u8, Vec<u32>)> = Vec::new();
    let (mut l2, mut l3) = (0u64, 0u64);
    let mut off = 0usize;
    while off + 8 <= len as usize {
        let rec = unsafe { &*((buf.as_ptr() as *const u8).add(off) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX) };
        let size = rec.Size as usize;
        if size == 0 || off + size > len as usize {
            break;
        }
        match rec.Relationship.0 {
            0 => {
                // RelationProcessorCore
                let p = unsafe { &rec.Anonymous.Processor };
                let mut ids = Vec::new();
                for g in 0..p.GroupCount as usize {
                    let ga = unsafe { &*p.GroupMask.as_ptr().add(g) };
                    for bit in 0..usize::BITS {
                        if ga.Mask & (1usize << bit) != 0 {
                            ids.push(ga.Group as u32 * 64 + bit);
                        }
                    }
                }
                phys.push((p.EfficiencyClass, ids));
            }
            2 => {
                // RelationCache (skip instruction caches)
                let c = unsafe { &rec.Anonymous.Cache };
                if c.Type.0 != 1 {
                    match c.Level {
                        2 => l2 += c.CacheSize as u64,
                        3 => l3 += c.CacheSize as u64,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        off += size;
    }
    if phys.is_empty() {
        return fallback_topology();
    }
    let (cores, cores_p, cores_e) = classify(&phys);
    Topology { physical: phys.len() as u32, cores, cores_p, cores_e, l2, l3 }
}

fn fallback_topology() -> Topology {
    let n = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
    Topology { physical: n, cores: (0..n).map(|i| (i, 'P')).collect(), cores_p: n, cores_e: 0, l2: 0, l3: 0 }
}

pub fn topology() -> &'static Topology {
    static T: OnceLock<Topology> = OnceLock::new();
    T.get_or_init(read_topology)
}

const CPU_KEY: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";

/// Nominal (base) clock from the registry; honest: this is not the live boost frequency.
pub fn base_mhz() -> u32 {
    static M: OnceLock<u32> = OnceLock::new();
    *M.get_or_init(|| reg_u32(CPU_KEY, "~MHz").unwrap_or(0))
}

fn virtualization_enabled() -> bool {
    use windows::Win32::System::Threading::{IsProcessorFeaturePresent, PROCESSOR_FEATURE_ID};
    let fw = unsafe { IsProcessorFeaturePresent(PROCESSOR_FEATURE_ID(21)) }.as_bool(); // PF_VIRT_FIRMWARE_ENABLED
    fw || hypervisor_present()
}
fn hypervisor_present() -> bool {
    // CPUID.1:ECX bit 31
    unsafe { std::arch::x86_64::__cpuid(1).ecx & (1 << 31) != 0 }
}

// ------------------------------------------------------------------ memory / perf info

struct MemExtra {
    commit_total: u64,
    commit_limit: u64,
    cache: u64,
}
fn perf_info() -> MemExtra {
    use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    let mut pi = PERFORMANCE_INFORMATION { cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32, ..Default::default() };
    let ok = unsafe { GetPerformanceInfo(&mut pi, pi.cb) };
    let _ = ok;
    let ps = pi.PageSize as u64;
    MemExtra { commit_total: pi.CommitTotal as u64 * ps, commit_limit: pi.CommitLimit as u64 * ps, cache: pi.SystemCache as u64 * ps }
}

fn total_phys() -> u64 {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok() {
        m.ullTotalPhys
    } else {
        0
    }
}

// ------------------------------------------------------------------ get_live

struct NetRates {
    down: f64,
    up: f64,
}
static NET_RATES: Mutex<NetRates> = Mutex::new(NetRates { down: 0.0, up: 0.0 });
static DISK_SYS: Mutex<Option<(Instant, Value)>> = Mutex::new(None);

fn net_rates(state: &AppState) -> (f64, f64) {
    let mut g = lock(&state.nets);
    let elapsed = g.1.elapsed().as_secs_f64();
    if elapsed < 0.4 {
        let r = lock(&NET_RATES);
        return (r.down, r.up);
    }
    g.0.refresh(true);
    let (mut rx, mut tx) = (0u64, 0u64);
    for (name, data) in g.0.iter() {
        let n = name.to_lowercase();
        if n.contains("loopback") || n.contains("isatap") || n.contains("teredo") {
            continue;
        }
        rx += data.received();
        tx += data.transmitted();
    }
    g.1 = Instant::now();
    let r = (rx as f64 / elapsed, tx as f64 / elapsed);
    let mut c = lock(&NET_RATES);
    c.down = r.0;
    c.up = r.1;
    r
}

fn disk_free(path: &str) -> Option<(u64, u64)> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let w = util::wide(path);
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut avail), Some(&mut total), Some(&mut free)) }.ok()?;
    if total == 0 {
        return None;
    }
    Some((total, avail))
}

fn disk_json(mount: &str, total: u64, free: u64) -> Value {
    let used = total.saturating_sub(free);
    let pct = if total > 0 { used as f64 * 100.0 / total as f64 } else { 0.0 };
    json!({ "mount": mount, "total": total, "used": used, "free": free, "percent": round1(pct) })
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn disk_sys() -> Value {
    let mut c = lock(&DISK_SYS);
    if let Some((t, v)) = c.as_ref() {
        if t.elapsed() < Duration::from_secs(10) {
            return v.clone();
        }
    }
    let mut mount = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    mount.push('\\');
    let v = match disk_free(&mount) {
        Some((total, free)) => disk_json(&mount, total, free),
        None => json!({ "mount": mount, "total": 0, "used": 0, "free": 0, "percent": 0.0 }),
    };
    *c = Some((Instant::now(), v.clone()));
    v
}

#[tauri::command]
pub fn get_live(state: State<'_, AppState>) -> Value {
    live(&state)
}

pub fn live(state: &AppState) -> Value {
    let (cpu_pct, per_core, threads, total, avail) = {
        let mut sys = lock(&state.sys);
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let per: Vec<f64> = sys.cpus().iter().map(|c| round1(c.cpu_usage() as f64)).collect();
        (round1(sys.global_cpu_usage() as f64), per.clone(), per.len(), sys.total_memory(), sys.available_memory())
    };
    let used = total.saturating_sub(avail);
    let pct = if total > 0 { used as f64 * 100.0 / total as f64 } else { 0.0 };
    let pi = perf_info();
    let (down, up) = net_rates(state);
    json!({
        "ts": util::unix_now(),
        "cpu": { "percent": cpu_pct, "per_core": per_core, "freq_mhz": base_mhz(), "threads": threads, "cores": topology().physical },
        "mem": { "total": total, "used": used, "available": avail, "percent": round1(pct),
                 "committed": pi.commit_total, "cached": pi.cache },
        "net": { "down_bps": down.round(), "up_bps": up.round() },
        "disk_sys": disk_sys(),
    })
}

// ------------------------------------------------------------------ get_drives

extern "system" {
    fn SetErrorMode(mode: u32) -> u32;
}

pub fn list_drives() -> Vec<Value> {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
    // no "insert a disk" dialogs for empty card readers
    let old = unsafe { SetErrorMode(0x0001) }; // SEM_FAILCRITICALERRORS
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let mount = format!("{}:\\", (b'A' + i as u8) as char);
        let w = util::wide(&mount);
        let kind = unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) };
        // 2 removable, 3 fixed; skip remote (4), cd-rom (5), ram disk (6), unknown/no root (0,1)
        if kind != 2 && kind != 3 {
            continue;
        }
        let Some((total, free)) = disk_free(&mount) else { continue };
        let mut label = [0u16; 261];
        let mut fs = [0u16; 261];
        let ok = unsafe { GetVolumeInformationW(PCWSTR(w.as_ptr()), Some(&mut label), None, None, None, Some(&mut fs)) }.is_ok();
        let to_s = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|c| *c == 0).unwrap_or(b.len())]);
        let (mut label, fstype) = if ok { (to_s(&label), to_s(&fs)) } else { (String::new(), String::new()) };
        if label.is_empty() {
            label = if kind == 2 { "Removable disk".into() } else { "Local disk".into() };
        }
        let mut v = disk_json(&mount, total, free);
        v["label"] = json!(label);
        v["fstype"] = json!(fstype);
        v["removable"] = json!(kind == 2);
        out.push(v);
    }
    unsafe { SetErrorMode(old) };
    out
}

#[tauri::command]
pub async fn get_drives() -> CmdResult<Vec<Value>> {
    spawn_blocking(list_drives).await
}

// ------------------------------------------------------------------ top processes / apps

#[derive(Clone, Debug)]
pub struct ProcRow {
    pub pid: u32,
    pub name: String,
    pub mem: u64,
    pub cpu: f64,
}

struct TopCache {
    sys: System,
    at: Option<Instant>,
    rows: Vec<ProcRow>,
}
static TOP: OnceLock<Mutex<TopCache>> = OnceLock::new();

fn proc_snapshot() -> Vec<ProcRow> {
    let m = TOP.get_or_init(|| Mutex::new(TopCache { sys: System::new(), at: None, rows: Vec::new() }));
    let mut c = lock(m);
    if let Some(t) = c.at {
        if t.elapsed() < Duration::from_secs(2) {
            return c.rows.clone();
        }
    }
    let kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
    if c.at.is_none() {
        // first sample has no CPU delta yet: take a priming sample
        c.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        std::thread::sleep(Duration::from_millis(250));
    }
    c.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
    let rows: Vec<ProcRow> = c
        .sys
        .processes()
        .iter()
        .filter(|(pid, _)| pid.as_u32() != 0)
        .map(|(pid, p)| ProcRow {
            pid: pid.as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            mem: p.memory(),
            cpu: (p.cpu_usage() as f64 / ncpu * 100.0).round() / 100.0,
        })
        .collect();
    c.rows = rows.clone();
    c.at = Some(Instant::now());
    rows
}

fn sort_by(by: &str, a_mem: u64, a_cpu: f64, b_mem: u64, b_cpu: f64) -> std::cmp::Ordering {
    if by == "cpu" {
        b_cpu.partial_cmp(&a_cpu).unwrap_or(std::cmp::Ordering::Equal).then(b_mem.cmp(&a_mem))
    } else {
        b_mem.cmp(&a_mem).then(b_cpu.partial_cmp(&a_cpu).unwrap_or(std::cmp::Ordering::Equal))
    }
}

pub fn top_processes_from(mut rows: Vec<ProcRow>, by: &str, limit: usize) -> Vec<Value> {
    rows.sort_by(|a, b| sort_by(by, a.mem, a.cpu, b.mem, b.cpu));
    rows.into_iter().take(limit).map(|r| json!({ "pid": r.pid, "name": r.name, "mem": r.mem, "cpu": r.cpu })).collect()
}

/// Group by executable name (case-insensitive); first-seen spelling is displayed.
pub fn top_apps_from(rows: Vec<ProcRow>, by: &str, limit: usize) -> Vec<Value> {
    let mut map: HashMap<String, (String, u64, u64, f64)> = HashMap::new();
    for r in rows {
        let e = map.entry(r.name.to_lowercase()).or_insert_with(|| (r.name.clone(), 0, 0, 0.0));
        e.1 += 1;
        e.2 += r.mem;
        e.3 += r.cpu;
    }
    let mut v: Vec<_> = map.into_values().collect();
    v.sort_by(|a, b| sort_by(by, a.2, a.3, b.2, b.3));
    v.into_iter()
        .take(limit)
        .map(|(name, count, mem, cpu)| json!({ "name": name, "count": count, "mem": mem, "cpu": (cpu * 100.0).round() / 100.0 }))
        .collect()
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_top_processes(by: Option<String>, limit: Option<usize>) -> CmdResult<Vec<Value>> {
    let by = by.unwrap_or_else(|| "memory".into());
    let limit = limit.unwrap_or(6).min(200);
    spawn_blocking(move || top_processes_from(proc_snapshot(), &by, limit)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_top_apps(by: Option<String>, limit: Option<usize>) -> CmdResult<Vec<Value>> {
    let by = by.unwrap_or_else(|| "memory".into());
    let limit = limit.unwrap_or(6).min(200);
    spawn_blocking(move || top_apps_from(proc_snapshot(), &by, limit)).await
}

// ------------------------------------------------------------------ copy_text

#[tauri::command(rename_all = "snake_case")]
pub async fn copy_text(text: String) -> CmdResult<()> {
    spawn_blocking(move || clip(&text)).await?
}

extern "system" {
    fn OpenClipboard(hwnd: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, mem: isize) -> isize;
}

/// Win32 clipboard (CF_UNICODETEXT), so any Unicode text round-trips.
fn clip(text: &str) -> CmdResult<()> {
    use windows::Win32::Foundation::GlobalFree;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    let fail = || util::CmdError("Could not copy to the clipboard.".into());
    let w = util::wide(text);
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2).map_err(|_| fail())?;
        let p = GlobalLock(h) as *mut u16;
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(fail());
        }
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        let _ = GlobalUnlock(h);
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(0) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        if !opened {
            let _ = GlobalFree(Some(h));
            return Err(fail());
        }
        EmptyClipboard();
        let ok = SetClipboardData(13, h.0 as isize) != 0; // CF_UNICODETEXT; the system owns h on success
        CloseClipboard();
        if !ok {
            let _ = GlobalFree(Some(h));
            return Err(fail());
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ specs: model

pub type Rows = Vec<(String, String)>;
pub type Sections = Vec<(String, Rows)>;

fn tone(section: &str, label: &str, value: &str) -> Option<&'static str> {
    if section != "Security & firmware" {
        return None;
    }
    let v = value.to_lowercase();
    match label {
        "Secure Boot" => match v.as_str() {
            "enabled" => Some("ok"),
            "disabled" => Some("warn"),
            _ => None,
        },
        "TPM" => {
            if v.starts_with("version") && v.ends_with("enabled") {
                Some("ok")
            } else if v.starts_with("not detected") || v.ends_with("disabled") {
                Some("warn")
            } else {
                None
            }
        }
        "Virtualization" => (v == "enabled").then_some("ok"),
        _ => None,
    }
}

pub fn sections_json(sections: &Sections) -> Value {
    let secs: Vec<Value> = sections
        .iter()
        .map(|(t, rows)| {
            let rows: Vec<Value> = rows
                .iter()
                .map(|(k, v)| match tone(t, k, v) {
                    Some(tn) => json!([k, v, tn]),
                    None => json!([k, v]),
                })
                .collect();
            json!({ "title": t, "rows": rows })
        })
        .collect();
    json!({ "sections": secs })
}

// 1.x-compatible cache: {"saved": <unix float>, "sections": [[title, [[label, value], ...]], ...]}
fn specs_path() -> std::path::PathBuf {
    util::data_dir().join("specs.json")
}

pub fn parse_cached_specs(text: &str) -> Option<Sections> {
    let v: Value = serde_json::from_str(text).ok()?;
    let mut out = Sections::new();
    for s in v.get("sections")?.as_array()? {
        let a = s.as_array()?;
        let title = a.first()?.as_str()?.to_string();
        let mut rows = Rows::new();
        for r in a.get(1)?.as_array()? {
            let r = r.as_array()?;
            rows.push((r.first()?.as_str()?.to_string(), r.get(1)?.as_str()?.to_string()));
        }
        out.push((title, rows));
    }
    Some(out)
}

pub fn cached_specs() -> Option<Sections> {
    parse_cached_specs(&std::fs::read_to_string(specs_path()).ok()?)
}

pub fn specs_cache_text(sections: &Sections) -> String {
    let secs: Vec<Value> = sections.iter().map(|(t, rows)| json!([t, rows.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>()])).collect();
    json!({ "saved": util::unix_now() as f64, "sections": secs }).to_string()
}

fn save_cached_specs(sections: &Sections) {
    let _ = std::fs::write(specs_path(), specs_cache_text(sections));
}

fn cached_row(c: &Option<Sections>, section: &str, label: &str) -> Option<String> {
    c.as_ref()?.iter().find(|s| s.0 == section)?.1.iter().find(|r| r.0 == label).map(|r| r.1.clone())
}

// ------------------------------------------------------------------ specs: direct readers

pub fn firmware_mode() -> &'static str {
    use windows::Win32::System::SystemInformation::{GetFirmwareType, FIRMWARE_TYPE};
    let mut t = FIRMWARE_TYPE(0);
    if unsafe { GetFirmwareType(&mut t) }.is_ok() {
        return match t.0 {
            1 => "Legacy BIOS",
            2 => "UEFI",
            _ => "Unknown",
        };
    }
    "Unknown"
}

pub fn secure_boot() -> &'static str {
    match reg_u32(r"SYSTEM\CurrentControlSet\Control\SecureBoot\State", "UEFISecureBootEnabled") {
        None => {
            if firmware_mode() == "Legacy BIOS" {
                "Not supported (Legacy BIOS)"
            } else {
                "Unavailable"
            }
        }
        Some(1) => "Enabled",
        Some(_) => "Disabled",
    }
}

/// GPU name -> dedicated VRAM bytes (Win32_VideoController caps at 4 GB, the registry does not).
pub fn gpu_vram() -> Vec<(String, u64)> {
    let base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let mut out = Vec::new();
    let Some(k) = reg_open(base) else { return out };
    for sub in k.enum_keys().flatten() {
        if sub.is_empty() || !sub.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(sk) = k.open_subkey_with_flags(&sub, KEY_READ) else { continue };
        let Ok(name) = sk.get_value::<String, _>("DriverDesc") else { continue };
        let Ok(raw) = sk.get_raw_value("HardwareInformation.qwMemorySize") else { continue };
        let mut b = [0u8; 8];
        let n = raw.bytes.len().min(8);
        b[..n].copy_from_slice(&raw.bytes[..n]);
        out.push((clean(&name), u64::from_le_bytes(b)));
    }
    out
}

/// Days since 1970-01-01 -> (y, m, d) (Howard Hinnant's civil_from_days).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn fmt_date(unix: i64) -> String {
    let (y, m, d) = civil_from_days(unix.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

fn pagefile_total() -> u64 {
    perf_info().commit_limit.saturating_sub(total_phys())
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "-".into())
}
fn username() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "-".into())
}

fn dash(s: String) -> String {
    if s.is_empty() {
        "-".into()
    } else {
        s
    }
}

pub fn collect_fast() -> Sections {
    let cached = cached_specs();
    let bios = r"HARDWARE\DESCRIPTION\System\BIOS";
    let cur = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let mut s: Sections = Vec::new();

    s.push((
        "Device".into(),
        vec![
            ("Device name".into(), hostname()),
            ("Manufacturer".into(), dash(reg_str(bios, "SystemManufacturer"))),
            ("Model".into(), dash(reg_str(bios, "SystemProductName"))),
            ("User".into(), username()),
        ],
    ));

    let build = reg_str(cur, "CurrentBuild");
    let mut name = reg_str(cur, "ProductName");
    if build.parse::<u32>().unwrap_or(0) >= 22000 {
        name = name.replace("Windows 10", "Windows 11");
    }
    let inst = reg_u32(cur, "InstallDate").unwrap_or(0);
    s.push((
        "Operating system".into(),
        vec![
            ("Edition".into(), if name.is_empty() { "Windows".into() } else { name }),
            ("Version".into(), format!("{} (build {}.{})", reg_str(cur, "DisplayVersion"), build, reg_u32(cur, "UBR").unwrap_or(0)).trim().to_string()),
            ("Architecture".into(), "64-bit".into()),
            ("Installed on".into(), if inst > 0 { fmt_date(inst as i64) } else { "-".into() }),
        ],
    ));

    s.push((
        "Security & firmware".into(),
        vec![
            ("Firmware mode".into(), firmware_mode().into()),
            ("Secure Boot".into(), secure_boot().into()),
            ("TPM".into(), cached_row(&cached, "Security & firmware", "TPM").unwrap_or_else(|| "Detecting...".into())),
            ("Virtualization".into(), if virtualization_enabled() { "Enabled" } else { "Disabled" }.into()),
        ],
    ));

    let t = topology();
    let mhz = base_mhz();
    s.push((
        "Processor".into(),
        vec![
            ("Model".into(), dash(reg_str(CPU_KEY, "ProcessorNameString"))),
            ("Cores / threads".into(), format!("{} cores / {} threads", t.physical, t.cores.len())),
            ("Base speed".into(), if mhz > 0 { format!("{:.2} GHz", mhz as f64 / 1000.0) } else { "-".into() }),
        ],
    ));

    s.push((
        "Memory".into(),
        vec![
            ("Total".into(), fmt_size(total_phys() as f64)),
            ("Modules".into(), cached_row(&cached, "Memory", "Modules").unwrap_or_else(|| "Detecting...".into())),
            ("Page file".into(), fmt_size(pagefile_total() as f64)),
        ],
    ));

    let g: Rows = gpu_vram().into_iter().filter(|(_, m)| *m > 0).map(|(n, m)| (n, format!("{} VRAM", fmt_size(m as f64)))).collect();
    if !g.is_empty() {
        s.push(("Graphics".into(), g));
    }

    s.push((
        "Motherboard & BIOS".into(),
        vec![
            ("Motherboard".into(), dash(format!("{} {}", reg_str(bios, "BaseBoardManufacturer"), reg_str(bios, "BaseBoardProduct")).trim().to_string())),
            ("BIOS".into(), dash(format!("{} {}", reg_str(bios, "BIOSVendor"), reg_str(bios, "BIOSVersion")).trim().to_string())),
            ("BIOS date".into(), dash(reg_str(bios, "BIOSReleaseDate"))),
        ],
    ));
    if let Some(n) = cached.as_ref().and_then(|c| c.iter().find(|x| x.0 == "Network")) {
        s.push(n.clone());
    }
    s
}

#[tauri::command]
pub fn get_specs_fast() -> Value {
    sections_json(&collect_fast())
}

// ------------------------------------------------------------------ specs: full (PowerShell / WMI)

const PS_SCRIPT: &str = r#"
$d = @{}
$d.cs = Get-CimInstance Win32_ComputerSystem | Select-Object Manufacturer,Model,Name,PCSystemType,Domain,PartOfDomain,HypervisorPresent
$d.os = Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture,@{n='Installed';e={$_.InstallDate.ToString('yyyy-MM-dd')}}
$d.cpu = Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed,L2CacheSize,L3CacheSize,SocketDesignation,VirtualizationFirmwareEnabled
$d.mem = Get-CimInstance Win32_PhysicalMemory | Select-Object Capacity,Speed,ConfiguredClockSpeed,Manufacturer,PartNumber,SMBIOSMemoryType
$d.gpu = Get-CimInstance Win32_VideoController | Select-Object Name,AdapterRAM,DriverVersion,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate
$d.board = Get-CimInstance Win32_BaseBoard | Select-Object Manufacturer,Product
$d.bios = Get-CimInstance Win32_BIOS | Select-Object Manufacturer,SMBIOSBIOSVersion,@{n='Date';e={$_.ReleaseDate.ToString('yyyy-MM-dd')}}
$d.disk = Get-CimInstance Win32_DiskDrive | Select-Object Model,Size,InterfaceType,MediaType
$d.tpm = Get-CimInstance -Namespace root/cimv2/security/microsofttpm -ClassName Win32_Tpm | Select-Object SpecVersion,IsEnabled_InitialValue,IsActivated_InitialValue,ManufacturerVersion
$d.net = Get-CimInstance Win32_NetworkAdapterConfiguration -Filter 'IPEnabled=true' | Select-Object Description,IPAddress,MACAddress
$d | ConvertTo-Json -Depth 4 -Compress
"#;

/// Pull the JSON object out of the PowerShell output (tolerates stray lines before/after).
pub fn extract_json(out: &str) -> Option<Value> {
    let a = out.find('{')?;
    let b = out.rfind('}')?;
    if b < a {
        return None;
    }
    serde_json::from_str(&out[a..=b]).ok()
}

fn clean_value(v: &mut Value) {
    match v {
        Value::String(s) => *s = clean(s),
        Value::Array(a) => a.iter_mut().for_each(clean_value),
        Value::Object(o) => o.values_mut().for_each(clean_value),
        _ => {}
    }
}

fn list(v: Option<&Value>) -> Vec<&Value> {
    match v {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(a)) => a.iter().collect(),
        Some(x) => vec![x],
    }
}
fn gs(v: &Value, k: &str) -> Option<String> {
    match v.get(k)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
fn gf(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0)
}
fn truthy(v: &Value, k: &str) -> bool {
    match v.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        _ => false,
    }
}

fn ddr(t: i64) -> &'static str {
    match t {
        20 => "DDR",
        21 => "DDR2",
        24 => "DDR3",
        26 => "DDR4",
        34 => "DDR5",
        _ => "",
    }
}

/// Port of sysinfo.collect(): WMI JSON + direct readers -> sections (titles/labels identical to 1.x).
pub fn build_full(d: &Value, is_admin: bool) -> Sections {
    let none = json!({});
    let first = |k: &str| list(d.get(k)).into_iter().next().unwrap_or(&none).clone();
    let cs = first("cs");
    let osd = first("os");
    let cpus = list(d.get("cpu"));
    let mut s: Sections = Vec::new();

    let kind = match cs.get("PCSystemType").and_then(|x| x.as_i64()) {
        Some(1) => "Desktop",
        Some(2) => "Laptop",
        Some(3) => "Workstation",
        Some(4) => "Enterprise server",
        _ => "",
    };
    let mut dev: Rows = vec![
        ("Device name".into(), gs(&cs, "Name").unwrap_or_else(hostname)),
        ("Manufacturer".into(), gs(&cs, "Manufacturer").unwrap_or_else(|| "-".into())),
        ("Model".into(), gs(&cs, "Model").unwrap_or_else(|| "-".into())),
    ];
    if !kind.is_empty() {
        dev.push(("Type".into(), kind.into()));
    }
    dev.push(("User".into(), username()));
    dev.push(("Domain / workgroup".into(), gs(&cs, "Domain").unwrap_or_else(|| "-".into())));
    s.push(("Device".into(), dev));

    s.push((
        "Operating system".into(),
        vec![
            ("Edition".into(), gs(&osd, "Caption").unwrap_or_else(|| "Windows".into())),
            ("Version".into(), format!("{} (build {})", gs(&osd, "Version").unwrap_or_else(|| "-".into()), gs(&osd, "BuildNumber").unwrap_or_else(|| "-".into()))),
            ("Architecture".into(), gs(&osd, "OSArchitecture").unwrap_or_else(|| "-".into())),
            ("Installed on".into(), gs(&osd, "Installed").unwrap_or_else(|| "-".into())),
        ],
    ));

    let tpm = list(d.get("tpm")).into_iter().next();
    let tpm_txt = match tpm {
        Some(t) => format!(
            "Version {} · {}",
            gs(t, "SpecVersion").unwrap_or_else(|| "?".into()).split(',').next().unwrap_or("?").trim(),
            if truthy(t, "IsEnabled_InitialValue") { "enabled" } else { "disabled" }
        ),
        None if is_admin => "Not detected".into(),
        None => "Unavailable (run as Administrator)".into(),
    };
    let virt = cpus.first().map(|c| truthy(c, "VirtualizationFirmwareEnabled")).unwrap_or(false) || truthy(&cs, "HypervisorPresent") || virtualization_enabled();
    s.push((
        "Security & firmware".into(),
        vec![
            ("Firmware mode".into(), firmware_mode().into()),
            ("Secure Boot".into(), secure_boot().into()),
            ("TPM".into(), tpm_txt),
            ("Virtualization".into(), if virt { "Enabled" } else { "Disabled" }.into()),
        ],
    ));

    if let Some(c) = cpus.first() {
        let mut rows: Rows = vec![
            ("Model".into(), gs(c, "Name").unwrap_or_else(|| "-".into()).trim().to_string()),
            (
                "Cores / threads".into(),
                format!("{} cores / {} threads", gs(c, "NumberOfCores").unwrap_or_else(|| "-".into()), gs(c, "NumberOfLogicalProcessors").unwrap_or_else(|| "-".into())),
            ),
            ("Base speed".into(), format!("{:.2} GHz", gf(c, "MaxClockSpeed") / 1000.0)),
        ];
        let mut caches = vec![];
        if gf(c, "L2CacheSize") > 0.0 {
            caches.push(format!("L2 {:.0} MB", gf(c, "L2CacheSize") / 1024.0));
        }
        if gf(c, "L3CacheSize") > 0.0 {
            caches.push(format!("L3 {:.0} MB", gf(c, "L3CacheSize") / 1024.0));
        }
        if !caches.is_empty() {
            rows.push(("Cache".into(), caches.join(" · ")));
        }
        if let Some(sock) = gs(c, "SocketDesignation") {
            rows.push(("Socket".into(), sock));
        }
        if cpus.len() > 1 {
            rows.insert(0, ("Sockets".into(), cpus.len().to_string()));
        }
        s.push(("Processor".into(), rows));
    }

    let mems = list(d.get("mem"));
    let mut rows: Rows = vec![("Total".into(), fmt_size(total_phys() as f64))];
    if !mems.is_empty() {
        let speed = mems.iter().map(|m| { let c = gf(m, "ConfiguredClockSpeed"); if c > 0.0 { c } else { gf(m, "Speed") } }).fold(0.0, f64::max);
        let mut kinds: Vec<&str> = mems.iter().map(|m| ddr(m.get("SMBIOSMemoryType").and_then(|x| x.as_i64()).unwrap_or(0))).filter(|k| !k.is_empty()).collect();
        kinds.sort();
        kinds.dedup();
        let mut sizes: Vec<u64> = mems.iter().map(|m| gf(m, "Capacity") as u64).collect();
        sizes.sort();
        sizes.dedup();
        let module = if sizes.len() == 1 { format!("{} × {}", mems.len(), fmt_size(sizes[0] as f64)) } else { format!("{} modules", mems.len()) };
        let speed_txt = if speed > 0.0 { format!("-{}", speed as u64) } else { String::new() };
        rows.push(("Modules".into(), format!("{module} {}{speed_txt}", kinds.join(" / ")).trim().to_string()));
        let uniq = |key: &str, skip_unknown: bool| {
            let mut v: Vec<String> = mems.iter().filter_map(|m| gs(m, key)).map(|x| x.trim().to_string()).filter(|x| !x.is_empty() && !(skip_unknown && x == "Unknown")).collect();
            v.sort();
            v.dedup();
            v
        };
        let brands = uniq("Manufacturer", true);
        if !brands.is_empty() {
            rows.push(("Manufacturer".into(), brands.join(", ")));
        }
        let parts = uniq("PartNumber", false);
        if !parts.is_empty() {
            rows.push(("Part number".into(), parts.join(", ")));
        }
    }
    rows.push(("Page file".into(), fmt_size(pagefile_total() as f64)));
    s.push(("Memory".into(), rows));

    let gpus = list(d.get("gpu"));
    if !gpus.is_empty() {
        let vram = gpu_vram();
        let mut rows: Rows = vec![];
        for g in &gpus {
            let name = gs(g, "Name").unwrap_or_else(|| "Graphics adapter".into());
            let mem = vram.iter().find(|(n, _)| *n == name).map(|x| x.1 as f64).filter(|m| *m > 0.0).unwrap_or_else(|| gf(g, "AdapterRAM"));
            rows.push((name, if mem > 0.0 { format!("{} VRAM", fmt_size(mem)) } else { "shared memory".into() }));
            if let Some(dv) = gs(g, "DriverVersion") {
                rows.push(("   Driver".into(), dv));
            }
        }
        if let Some(g) = gpus.iter().find(|g| gf(g, "CurrentHorizontalResolution") > 0.0) {
            rows.push((
                "Display".into(),
                format!("{} × {} @ {} Hz", gf(g, "CurrentHorizontalResolution") as u64, gf(g, "CurrentVerticalResolution") as u64, gs(g, "CurrentRefreshRate").unwrap_or_else(|| "-".into())),
            ));
        }
        s.push(("Graphics".into(), rows));
    }

    let board = first("board");
    let bios = first("bios");
    s.push((
        "Motherboard & BIOS".into(),
        vec![
            ("Motherboard".into(), dash(format!("{} {}", gs(&board, "Manufacturer").unwrap_or_default(), gs(&board, "Product").unwrap_or_default()).trim().to_string())),
            ("BIOS".into(), dash(format!("{} {}", gs(&bios, "Manufacturer").unwrap_or_default(), gs(&bios, "SMBIOSBIOSVersion").unwrap_or_default()).trim().to_string())),
            ("BIOS date".into(), gs(&bios, "Date").unwrap_or_else(|| "-".into())),
        ],
    ));

    let disks = list(d.get("disk"));
    if !disks.is_empty() {
        let rows: Rows = disks
            .iter()
            .map(|x| {
                let iface = gs(x, "InterfaceType").unwrap_or_default();
                (gs(x, "Model").unwrap_or_else(|| "Disk".into()).trim().to_string(), format!("{} · {}", fmt_size(gf(x, "Size")), iface).trim_matches(|c| c == ' ' || c == '·').to_string())
            })
            .collect();
        s.push(("Storage devices".into(), rows));
    }

    let nets = list(d.get("net"));
    if !nets.is_empty() {
        let mut rows: Rows = vec![];
        for n in nets.iter().take(3) {
            let ips = list(n.get("IPAddress"));
            rows.push((gs(n, "Description").unwrap_or_else(|| "Adapter".into()), ips.first().and_then(|v| v.as_str()).unwrap_or("-").to_string()));
            if let Some(m) = gs(n, "MACAddress") {
                rows.push(("   MAC address".into(), m));
            }
        }
        s.push(("Network".into(), rows));
    }

    if let Some(b) = battery() {
        s.push(("Battery".into(), vec![("Charge".into(), b)]));
    }
    s
}

fn battery() -> Option<String> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut p = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut p) }.ok()?;
    if p.BatteryFlag == 128 || p.BatteryFlag == 255 || p.BatteryLifePercent == 255 {
        return None;
    }
    Some(format!("{}% ({})", p.BatteryLifePercent, if p.ACLineStatus == 1 { "charging" } else { "on battery" }))
}

pub fn read_full() -> CmdResult<Sections> {
    let (ok, out) = util::powershell(PS_SCRIPT);
    let Some(mut raw) = extract_json(&out) else {
        let _ = ok;
        return util::err("Could not read the hardware details. Try Refresh.");
    };
    clean_value(&mut raw);
    Ok(build_full(&raw, util::is_admin()))
}

#[tauri::command]
pub async fn get_specs_full() -> CmdResult<Value> {
    spawn_blocking(|| -> CmdResult<Value> {
        let s = read_full()?;
        save_cached_specs(&s);
        Ok(sections_json(&s))
    })
    .await?
}

// ------------------------------------------------------------------ DirectX (dxdiag)

/// Parsed dxdiag report.
#[derive(Debug, Default, PartialEq)]
pub struct DxInfo {
    pub version: Option<String>,
    pub feature: Option<String>,
    pub model: Option<String>,
}

fn after<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.trim_start().strip_prefix(key).map(|r| r.trim())
}

pub fn parse_dxdiag(text: &str) -> DxInfo {
    let mut info = DxInfo::default();
    let mut best: Option<(Vec<u32>, String)> = None;
    for line in text.lines() {
        if info.version.is_none() {
            if let Some(v) = after(line, "DirectX Version:") {
                if !v.is_empty() {
                    info.version = Some(v.to_string());
                }
            }
        }
        if let Some(v) = after(line, "Feature Levels:") {
            let first = v.split(',').next().unwrap_or("").trim().to_string();
            if !first.is_empty() {
                let key: Vec<u32> = first.split('_').map(|p| p.parse().unwrap_or(0)).collect();
                if best.as_ref().map_or(true, |b| key > b.0) {
                    best = Some((key, first));
                }
            }
        }
        if info.model.is_none() {
            if let Some(v) = after(line, "Driver Model:") {
                if !v.is_empty() {
                    info.model = Some(v.to_string());
                }
            }
        }
    }
    info.feature = best.map(|b| b.1);
    info
}

pub fn dx_text(info: &DxInfo) -> Option<String> {
    let mut t = info.version.clone()?;
    if let Some(f) = &info.feature {
        t.push_str(&format!(" (feature level {f})"));
    }
    if let Some(m) = &info.model {
        t.push_str(&format!(" · {m}"));
    }
    Some(t)
}

fn decode_text(raw: &[u8]) -> String {
    if raw.starts_with(&[0xFF, 0xFE]) || raw.starts_with(&[0xFE, 0xFF]) {
        let le = raw[0] == 0xFF;
        let u: Vec<u16> = raw[2..].chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    }
}

fn dx_cache_path() -> std::path::PathBuf {
    util::data_dir().join("directx.txt")
}

static DX_SESSION: Mutex<Option<String>> = Mutex::new(None);

fn run_dxdiag() -> Option<String> {
    use std::os::windows::process::CommandExt;
    // dxdiag runs elevated with us and WRITES this file: use a folder created fresh (create_dir fails if it already
    // exists), so a pre-planted link at a predictable name cannot redirect the write.
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmpdir = std::env::temp_dir().join(format!("pk_dxdiag_{}_{}", std::process::id(), nanos));
    std::fs::create_dir(&tmpdir).ok()?;
    let path = tmpdir.join("dxdiag.txt");
    std::process::Command::new("dxdiag").args(["/whql:off", "/t"]).arg(&path).creation_flags(util::CREATE_NO_WINDOW).spawn().ok()?;
    let end = Instant::now() + Duration::from_secs(45);
    let mut text = String::new();
    while Instant::now() < end {
        if path.exists() {
            std::thread::sleep(Duration::from_millis(600));
            if let Ok(raw) = std::fs::read(&path) {
                text = decode_text(&raw);
                if text.contains("DirectX Version") {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = std::fs::remove_dir_all(&tmpdir);
    dx_text(&parse_dxdiag(&text))
}

#[tauri::command]
pub async fn get_directx() -> CmdResult<Value> {
    spawn_blocking(|| {
        if let Some(t) = lock(&DX_SESSION).clone() {
            return json!({ "text": t });
        }
        let cache = dx_cache_path();
        let cached = std::fs::read_to_string(&cache).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let fresh = std::fs::metadata(&cache)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map_or(false, |a| a < Duration::from_secs(7 * 86400));
        let text = match (&cached, fresh) {
            (Some(c), true) => c.clone(),
            _ => match run_dxdiag() {
                Some(t) => {
                    let _ = std::fs::write(&cache, &t);
                    t
                }
                None => cached.unwrap_or_else(|| "Unavailable".into()),
            },
        };
        *lock(&DX_SESSION) = Some(text.clone());
        json!({ "text": text })
    })
    .await
}

// ------------------------------------------------------------------ get_cpu_detail

#[derive(Serialize)]
pub struct CoreInfo {
    id: u32,
    kind: String,
}

static SOCKET: OnceLock<Option<String>> = OnceLock::new();

fn cpu_socket() -> Option<String> {
    SOCKET
        .get_or_init(|| {
            if let Some(c) = cached_row(&cached_specs(), "Processor", "Socket") {
                return Some(c);
            }
            let (_, out) = util::powershell("(Get-CimInstance Win32_Processor | Select-Object -First 1).SocketDesignation");
            let t = clean(out.lines().last().unwrap_or(""));
            (!t.is_empty() && t.len() < 40).then_some(t)
        })
        .clone()
}

fn process_count() -> usize {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    let Ok(h) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else { return 0 };
    let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut n = 0;
    if unsafe { Process32FirstW(h, &mut e) }.is_ok() {
        n = 1;
        while unsafe { Process32NextW(h, &mut e) }.is_ok() {
            n += 1;
        }
    }
    unsafe {
        let _ = CloseHandle(h);
    }
    n
}

pub fn cpu_detail() -> Value {
    let t = topology();
    let nz = |x: u64| if x > 0 { json!(x) } else { Value::Null };
    let model = reg_str(CPU_KEY, "ProcessorNameString");
    let mhz = base_mhz();
    json!({
        "model": if model.is_empty() { Value::Null } else { json!(model) },
        "socket": cpu_socket(),
        "architecture": "64-bit",
        "cores_p": t.cores_p,
        "cores_e": t.cores_e,
        "threads": t.cores.len(),
        "base_mhz": if mhz > 0 { json!(mhz) } else { Value::Null },
        "max_mhz": Value::Null,
        "l2_bytes": nz(t.l2),
        "l3_bytes": nz(t.l3),
        "virtualization": virtualization_enabled(),
        "cores": t.cores.iter().map(|(id, k)| CoreInfo { id: *id, kind: k.to_string() }).collect::<Vec<_>>(),
        "process_count": process_count(),
    })
}

#[tauri::command]
pub async fn get_cpu_detail() -> CmdResult<Value> {
    spawn_blocking(cpu_detail).await
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_format_matches_python() {
        assert_eq!(fmt_size(0.0), "0 B");
        assert_eq!(fmt_size(512.0 * 1048576.0), "512 MB");
        assert_eq!(fmt_size(16_966_000_000.0), "15.8 GB");
        assert_eq!(fmt_size(2.0 * 1024f64.powi(4)), "2 TB");
        assert_eq!(fmt_size(1536.0), "1.5 KB");
    }

    #[test]
    fn clean_strips_marks() {
        assert_eq!(clean("\u{200E} Intel\u{FEFF}\r\n"), "Intel");
    }

    #[test]
    fn hybrid_classification() {
        let mut phys = vec![];
        for i in 0..6u32 {
            phys.push((1u8, vec![i * 2, i * 2 + 1]));
        }
        for i in 12..16u32 {
            phys.push((0u8, vec![i]));
        }
        let (cores, p, e) = classify(&phys);
        assert_eq!((p, e, cores.len()), (6, 4, 16));
        assert_eq!(cores[0], (0, 'P'));
        assert_eq!(cores[12], (12, 'E'));
        let flat = vec![(0u8, vec![0, 1]), (0, vec![2, 3])];
        let (c, p, e) = classify(&flat);
        assert_eq!((p, e), (2, 0));
        assert!(c.iter().all(|x| x.1 == 'P'));
    }

    #[test]
    fn dates() {
        assert_eq!(fmt_date(0), "1970-01-01");
        assert_eq!(fmt_date(1_771_027_200), "2026-02-14");
    }

    #[test]
    fn dxdiag_parse() {
        let t = "   DirectX Version: DirectX 12\n Feature Levels: 12_2,12_1,12_0,11_1\n Feature Levels: 12_1,11_0\n Driver Model: WDDM 3.2\n Driver Model: WDDM 2.0\n";
        let i = parse_dxdiag(t);
        assert_eq!(i.feature.as_deref(), Some("12_2"));
        assert_eq!(dx_text(&i).unwrap(), "DirectX 12 (feature level 12_2) · WDDM 3.2");
        assert!(dx_text(&parse_dxdiag("nothing")).is_none());
    }

    #[test]
    fn utf16_decode() {
        let mut b = vec![0xFF, 0xFE];
        for u in "DirectX Version: x".encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        assert!(decode_text(&b).contains("DirectX Version"));
    }

    #[test]
    fn tones() {
        assert_eq!(tone("Security & firmware", "Secure Boot", "Enabled"), Some("ok"));
        assert_eq!(tone("Security & firmware", "TPM", "Version 2.0 · enabled"), Some("ok"));
        assert_eq!(tone("Security & firmware", "TPM", "Unavailable (run as Administrator)"), None);
        assert_eq!(tone("Device", "Secure Boot", "Enabled"), None);
    }

    #[test]
    fn specs_cache_roundtrip_is_1x_compatible() {
        let s: Sections = vec![("Device".into(), vec![("Device name".into(), "PC".into())])];
        let t = specs_cache_text(&s);
        let v: Value = serde_json::from_str(&t).unwrap();
        assert!(v["saved"].is_number());
        assert_eq!(v["sections"][0][1][0], json!(["Device name", "PC"]));
        assert_eq!(parse_cached_specs(&t).unwrap(), s);
        // a 1.x file with 3-element rows is not produced, but 2-element ones must load
        assert!(parse_cached_specs("{\"saved\":1.0,\"sections\":[[\"A\",[[\"k\",\"v\"]]]]}").is_some());
        assert!(parse_cached_specs("garbage").is_none());
    }

    #[test]
    fn full_report_from_sample_json() {
        let raw = r#"noise {"cs":{"Manufacturer":"Gigabyte","Model":"B760M","Name":"PC","PCSystemType":1,"Domain":"WORKGROUP","HypervisorPresent":false},
          "os":{"Caption":"Microsoft Windows 11 Pro","Version":"10.0.26300","BuildNumber":"26300","OSArchitecture":"64-bit","Installed":"2026-02-14"},
          "cpu":{"Name":"Intel(R) Core(TM) i5-14400F","NumberOfCores":10,"NumberOfLogicalProcessors":16,"MaxClockSpeed":2500,"L2CacheSize":10240,"L3CacheSize":20480,"SocketDesignation":"U3E1","VirtualizationFirmwareEnabled":true},
          "mem":[{"Capacity":8589934592,"Speed":3200,"ConfiguredClockSpeed":3200,"Manufacturer":"Kingston","PartNumber":"X","SMBIOSMemoryType":26},{"Capacity":8589934592,"Speed":3200,"ConfiguredClockSpeed":3200,"Manufacturer":"Kingston","PartNumber":"X","SMBIOSMemoryType":26}],
          "gpu":{"Name":"NVIDIA GeForce","AdapterRAM":4293918720,"DriverVersion":"32.0","CurrentHorizontalResolution":1920,"CurrentVerticalResolution":1080,"CurrentRefreshRate":144},
          "board":{"Manufacturer":"Gigabyte","Product":"B760M"},"bios":{"Manufacturer":"AMI","SMBIOSBIOSVersion":"F8","Date":"2024-05-02"},
          "disk":[{"Model":"Samsung","Size":500107862016,"InterfaceType":"SCSI"}],"tpm":null,
          "net":{"Description":"Realtek","IPAddress":["192.168.1.23","fe80::1"],"MACAddress":"AA:BB"}} trailing"#;
        let v = extract_json(raw).unwrap();
        let s = build_full(&v, false);
        let titles: Vec<&str> = s.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(&titles[..9], ["Device", "Operating system", "Security & firmware", "Processor", "Memory", "Graphics", "Motherboard & BIOS", "Storage devices", "Network"]);
        let sec = s.iter().find(|x| x.0 == "Security & firmware").unwrap();
        assert!(sec.1.iter().any(|r| r.0 == "TPM" && r.1 == "Unavailable (run as Administrator)"));
        let mem = s.iter().find(|x| x.0 == "Memory").unwrap();
        assert!(mem.1.iter().any(|r| r.0 == "Modules" && r.1 == "2 × 8 GB DDR4-3200"));
        let proc = s.iter().find(|x| x.0 == "Processor").unwrap();
        assert!(proc.1.iter().any(|r| r.0 == "Cache" && r.1 == "L2 10 MB · L3 20 MB"));
        let net = s.iter().find(|x| x.0 == "Network").unwrap();
        assert_eq!(net.1[0].1, "192.168.1.23");
        let st = s.iter().find(|x| x.0 == "Storage devices").unwrap();
        assert_eq!(st.1[0], ("Samsung".to_string(), "465.8 GB · SCSI".to_string()));
    }

    fn rows() -> Vec<ProcRow> {
        let r = |pid, n: &str, mem, cpu| ProcRow { pid, name: n.into(), mem, cpu };
        vec![r(1, "brave.exe", 100, 1.0), r(2, "Brave.exe", 300, 0.5), r(3, "a.exe", 200, 2.0)]
    }

    #[test]
    fn grouping_and_sorting() {
        let apps = top_apps_from(rows(), "memory", 6);
        assert_eq!(apps[0]["name"], "brave.exe");
        assert_eq!(apps[0]["count"], 2);
        assert_eq!(apps[0]["mem"], 400);
        let by_cpu = top_apps_from(rows(), "cpu", 1);
        assert_eq!(by_cpu[0]["name"], "a.exe");
        let p = top_processes_from(rows(), "memory", 2);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0]["pid"], 2);
    }

    // ---- read-only smoke tests against the real machine: cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn smoke_live() {
        let st = AppState::new();
        std::thread::sleep(Duration::from_millis(600));
        live(&st);
        std::thread::sleep(Duration::from_millis(1000));
        let t = Instant::now();
        let v = live(&st);
        println!("get_live took {:?}\n{}", t.elapsed(), serde_json::to_string_pretty(&v).unwrap());
        let total = v["mem"]["total"].as_f64().unwrap() / 1073741824.0;
        println!("RAM {total:.2} GiB");
        assert!(total > 4.0);
    }

    #[test]
    #[ignore]
    fn smoke_drives_top() {
        println!("{}", serde_json::to_string_pretty(&list_drives()).unwrap());
        let t = Instant::now();
        let a = top_apps_from(proc_snapshot(), "memory", 6);
        println!("top apps ({:?}): {}", t.elapsed(), serde_json::to_string_pretty(&a).unwrap());
        println!("top cpu procs: {}", serde_json::to_string(&top_processes_from(proc_snapshot(), "cpu", 6)).unwrap());
    }

    #[test]
    #[ignore]
    fn smoke_cpu_detail() {
        println!("{}", serde_json::to_string_pretty(&cpu_detail()).unwrap());
    }

    #[test]
    #[ignore]
    fn smoke_specs() {
        let t = Instant::now();
        println!("fast {:?}: {}", t.elapsed(), serde_json::to_string_pretty(&sections_json(&collect_fast())).unwrap());
        let t = Instant::now();
        let full = read_full().unwrap();
        println!("full {:?}: {}", t.elapsed(), serde_json::to_string_pretty(&sections_json(&full)).unwrap());
    }

    #[test]
    #[ignore]
    fn smoke_clip_unicode() {
        clip("Process Killer \u{2713} \u{00e9}\u{4e2d}").unwrap();
        let (_, out) = util::powershell("Get-Clipboard");
        println!("clipboard: {out}");
        assert!(out.contains('\u{2713}') && out.contains('\u{4e2d}'));
    }
}
