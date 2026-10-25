// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! perf - Memory, Network and Graphics pages: get_mem_detail, get_mem_hw, get_net_detail, get_gpu_detail.
//! Direct Win32 APIs only (GlobalMemoryStatusEx, GetPerformanceInfo, SMBIOS, IP Helper, DXGI, PDH).
#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::util::{CmdError, CmdResult};

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| CmdError("The background task failed. Please try again.".into()))?
}

// ======================================================================================================== memory

#[tauri::command]
pub async fn get_mem_detail() -> CmdResult<Value> {
    blocking(mem_detail).await
}

#[tauri::command]
pub async fn get_mem_hw() -> CmdResult<Value> {
    blocking(|| Ok(mem_hw())).await
}

fn mem_detail() -> CmdResult<Value> {
    use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    unsafe {
        let mut ms = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        GlobalMemoryStatusEx(&mut ms).map_err(|_| CmdError("Could not read memory status.".into()))?;
        let mut pi = PERFORMANCE_INFORMATION { cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32, ..Default::default() };
        GetPerformanceInfo(&mut pi, pi.cb).map_err(|_| CmdError("Could not read performance information.".into()))?;
        let page = pi.PageSize as u64;
        let (pf_total, pf_used) = page_file_totals(page, &ms);
        Ok(json!({
            "total": ms.ullTotalPhys,
            "used": ms.ullTotalPhys.saturating_sub(ms.ullAvailPhys),
            "available": ms.ullAvailPhys,
            "cached": pi.SystemCache as u64 * page,
            "committed": pi.CommitTotal as u64 * page,
            "commit_limit": pi.CommitLimit as u64 * page,
            "paged_pool": pi.KernelPaged as u64 * page,
            "nonpaged_pool": pi.KernelNonpaged as u64 * page,
            "compressed": compressed_bytes(),
            "page_file_total": pf_total,
            "page_file_used": pf_used,
        }))
    }
}

/// Sum of all page files via EnumPageFilesW; falls back to (commit limit - RAM) when that fails.
fn page_file_totals(page: u64, ms: &windows::Win32::System::SystemInformation::MEMORYSTATUSEX) -> (u64, u64) {
    use windows::core::BOOL;
    use windows::Win32::System::ProcessStatus::{EnumPageFilesW, ENUM_PAGE_FILE_INFORMATION};
    use windows::core::PCWSTR;
    unsafe extern "system" fn cb(ctx: *mut c_void, info: *mut ENUM_PAGE_FILE_INFORMATION, _name: PCWSTR) -> BOOL {
        let acc = &mut *(ctx as *mut (u64, u64));
        acc.0 += (*info).TotalSize as u64;
        acc.1 += (*info).TotalInUse as u64;
        BOOL(1)
    }
    let mut acc: (u64, u64) = (0, 0);
    let ok = unsafe { EnumPageFilesW(Some(cb), &mut acc as *mut _ as *mut c_void) }.is_ok();
    if ok {
        (acc.0 * page, acc.1 * page)
    } else {
        let total = ms.ullTotalPageFile.saturating_sub(ms.ullTotalPhys);
        let free = ms.ullAvailPageFile.saturating_sub(ms.ullAvailPhys.min(ms.ullAvailPageFile));
        (total, total.saturating_sub(free))
    }
}

struct CompCache {
    pid: Option<u32>,
    at: Option<Instant>,
    value: Option<u64>,
}
static COMP: Mutex<CompCache> = Mutex::new(CompCache { pid: None, at: None, value: None });

/// Working set of the "Memory Compression" process, cached for 5 s (finding the pid needs a process snapshot).
fn compressed_bytes() -> Option<u64> {
    let mut c = COMP.lock().ok()?;
    if let Some(at) = c.at {
        if at.elapsed() < Duration::from_secs(5) {
            return c.value;
        }
    }
    if c.pid.is_none() {
        c.pid = find_pid("Memory Compression");
    }
    let mut v = c.pid.and_then(working_set_of);
    if v.is_none() && c.pid.is_some() {
        // pid may be stale (reboot is impossible within a session, but be safe) - search once more, then give up
        c.pid = find_pid("Memory Compression");
        v = c.pid.and_then(working_set_of);
    }
    c.at = Some(Instant::now());
    c.value = v;
    v
}

fn find_pid(name: &str) -> Option<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = None;
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let n = String::from_utf16_lossy(&e.szExeFile);
                if n.trim_end_matches('\0') == name {
                    found = Some(e.th32ProcessID);
                    break;
                }
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        found
    }
}

fn working_set_of(pid: u32) -> Option<u64> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut pmc = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
            let r = GetProcessMemoryInfo(h, &mut pmc, pmc.cb);
            let _ = CloseHandle(h);
            if r.is_ok() && pmc.WorkingSetSize > 0 {
                return Some(pmc.WorkingSetSize as u64);
            }
        }
    }
    // Not openable without elevation: let sysinfo read just this pid (uses the process snapshot API).
    let mut sys = sysinfo::System::new();
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[p]), true);
    sys.process(p).map(|p| p.memory()).filter(|m| *m > 0)
}

// ---- SMBIOS ----

#[derive(Debug, Default, PartialEq)]
pub struct Module {
    pub slot: Option<String>,
    pub size: u64,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    pub speed_mhz: Option<u32>,
    pub form_factor: Option<&'static str>,
    pub mem_type: Option<&'static str>,
}

#[derive(Debug, Default, PartialEq)]
pub struct MemHw {
    pub modules: Vec<Module>,
    pub slots_total: Option<u32>,
    pub entries: u32, // number of type-17 structures (physical slots described)
}

fn smbios_str(strings: &[u8], idx: u8) -> Option<String> {
    if idx == 0 {
        return None;
    }
    let s = strings.split(|b| *b == 0).nth(idx as usize - 1)?;
    let t = String::from_utf8_lossy(s).trim().to_string();
    let junk = ["", "Unknown", "Not Specified", "To Be Filled By O.E.M.", "None", "[Empty]", "Undefined", "N/A"];
    if junk.iter().any(|j| j.eq_ignore_ascii_case(&t)) || t.chars().all(|c| c == '0') {
        None
    } else {
        Some(t)
    }
}

fn mem_type_name(t: u8) -> Option<&'static str> {
    Some(match t {
        0x12 => "DDR",
        0x13 => "DDR2",
        0x14 => "DDR2 FB-DIMM",
        0x18 => "DDR3",
        0x1A => "DDR4",
        0x1B => "LPDDR",
        0x1C => "LPDDR2",
        0x1D => "LPDDR3",
        0x1E => "LPDDR4",
        0x20 => "HBM",
        0x21 => "HBM2",
        0x22 => "DDR5",
        0x23 => "LPDDR5",
        0x24 => "HBM3",
        0x0F => "SDRAM",
        _ => return None,
    })
}

fn form_factor_name(f: u8) -> Option<&'static str> {
    Some(match f {
        0x03 => "SIMM",
        0x09 => "DIMM",
        0x0C => "RIMM",
        0x0D => "SODIMM",
        0x0F => "FB-DIMM",
        0x0B => "Row of chips",
        0x05 => "Chip",
        _ => return None,
    })
}

/// Parse a raw SMBIOS buffer as returned by GetSystemFirmwareTable('RSMB') (8-byte header + structure table).
pub fn parse_smbios(raw: &[u8]) -> MemHw {
    let mut hw = MemHw::default();
    if raw.len() < 8 {
        return hw;
    }
    let tlen = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
    let table = &raw[8..raw.len().min(8 + tlen)];
    let mut total_devices: u32 = 0;
    let mut have16 = false;
    let mut pos = 0usize;
    while pos + 4 <= table.len() {
        let ty = table[pos];
        let len = table[pos + 1] as usize;
        if len < 4 || pos + len > table.len() {
            break;
        }
        let f = &table[pos..pos + len];
        // string-set: ends with a double NUL
        let mut end = pos + len;
        while end + 1 < table.len() && !(table[end] == 0 && table[end + 1] == 0) {
            end += 1;
        }
        let strings = if end > pos + len { &table[pos + len..end] } else { &[][..] };
        let next = (end + 2).min(table.len() + 2);
        let u16at = |o: usize| if o + 2 <= f.len() { Some(u16::from_le_bytes([f[o], f[o + 1]])) } else { None };
        let u32at = |o: usize| if o + 4 <= f.len() { Some(u32::from_le_bytes([f[o], f[o + 1], f[o + 2], f[o + 3]])) } else { None };
        match ty {
            16 => {
                // Physical Memory Array; Use (0x05) == 3 means system memory
                let usage = f.get(5).copied().unwrap_or(3);
                if usage == 3 {
                    if let Some(n) = u16at(0x0D) {
                        total_devices = total_devices.saturating_add(n as u32);
                        have16 = true;
                    }
                }
            }
            17 => {
                hw.entries += 1;
                let size_mb: u64 = match u16at(0x0C) {
                    Some(0) | Some(0xFFFF) | None => 0,
                    Some(0x7FFF) => u32at(0x1C).map(|v| (v & 0x7FFF_FFFF) as u64).unwrap_or(0),
                    Some(v) if v & 0x8000 != 0 => 0, // KB units: treat tiny sizes as MB-fraction below
                    Some(v) => v as u64,
                };
                let size_kb: u64 = match u16at(0x0C) {
                    Some(v) if v != 0xFFFF && v != 0x7FFF && v & 0x8000 != 0 => (v & 0x7FFF) as u64,
                    _ => 0,
                };
                let size = size_mb * 1024 * 1024 + size_kb * 1024;
                let b = |o: usize| f.get(o).copied().unwrap_or(0);
                let cfg = u16at(0x20).filter(|v| *v != 0 && *v != 0xFFFF);
                let rated = u16at(0x15).filter(|v| *v != 0 && *v != 0xFFFF);
                hw.modules.push(Module {
                    slot: smbios_str(strings, b(0x10)),
                    size,
                    manufacturer: smbios_str(strings, b(0x17)),
                    part_number: smbios_str(strings, b(0x1A)),
                    speed_mhz: cfg.or(rated).map(|v| v as u32),
                    form_factor: form_factor_name(b(0x0E)),
                    mem_type: mem_type_name(b(0x12)),
                });
            }
            127 => break,
            _ => {}
        }
        pos = next;
    }
    hw.slots_total = if have16 && total_devices > 0 { Some(total_devices) } else if hw.entries > 0 { Some(hw.entries) } else { None };
    hw
}

fn hw_to_json(hw: &MemHw) -> Value {
    let populated: Vec<&Module> = hw.modules.iter().filter(|m| m.size > 0).collect();
    let first = populated.first();
    let mem_type = first.and_then(|m| m.mem_type);
    let speed = populated.iter().filter_map(|m| m.speed_mhz).max();
    let ff = first.and_then(|m| m.form_factor);
    let modules: Vec<Value> = populated
        .iter()
        .map(|m| {
            json!({ "slot": m.slot, "size": m.size, "manufacturer": m.manufacturer,
                    "part_number": m.part_number, "speed_mhz": m.speed_mhz })
        })
        .collect();
    json!({
        "type": mem_type, "speed_mhz": speed,
        "slots_used": if hw.entries > 0 { json!(populated.len()) } else { Value::Null },
        "slots_total": hw.slots_total, "form_factor": ff, "modules": modules,
    })
}

fn read_smbios() -> Option<Vec<u8>> {
    use windows::Win32::System::SystemInformation::{GetSystemFirmwareTable, FIRMWARE_TABLE_PROVIDER};
    let sig = FIRMWARE_TABLE_PROVIDER(0x52534D42); // 'RSMB'
    unsafe {
        let n = GetSystemFirmwareTable(sig, 0, None);
        if n == 0 {
            return None;
        }
        let mut buf = vec![0u8; n as usize];
        let got = GetSystemFirmwareTable(sig, 0, Some(&mut buf));
        if got == 0 {
            return None;
        }
        buf.truncate(got as usize);
        Some(buf)
    }
}

fn mem_hw() -> Value {
    match read_smbios() {
        Some(raw) => hw_to_json(&parse_smbios(&raw)),
        None => json!({ "type": null, "speed_mhz": null, "slots_used": null, "slots_total": null,
                        "form_factor": null, "modules": [] }),
    }
}

// ====================================================================================================== network

#[tauri::command]
pub async fn get_net_detail() -> CmdResult<Value> {
    blocking(net_detail).await
}

struct StaticAdapter {
    id: String,
    name: String,
    kind: &'static str,
    luid: u64,
    if_index: u32,
    ip: Option<String>,
    gateway: Option<String>,
    dns: Vec<String>,
    mac: Option<String>,
    ssid: Option<String>,
}

static NET_STATIC: Mutex<Option<(Instant, std::sync::Arc<Vec<StaticAdapter>>)>> = Mutex::new(None);
static NET_PREV: Mutex<Option<HashMap<u64, (u64, u64, Instant)>>> = Mutex::new(None);

unsafe fn sockaddr_ip(sa: *const windows::Win32::Networking::WinSock::SOCKADDR) -> Option<std::net::IpAddr> {
    if sa.is_null() {
        return None;
    }
    let p = sa as *const u8;
    let fam = u16::from_le_bytes([*p, *p.add(1)]);
    match fam {
        2 => Some(std::net::IpAddr::V4(std::net::Ipv4Addr::new(*p.add(4), *p.add(5), *p.add(6), *p.add(7)))),
        23 => {
            let mut o = [0u8; 16];
            for (i, b) in o.iter_mut().enumerate() {
                *b = *p.add(8 + i);
            }
            Some(std::net::IpAddr::V6(std::net::Ipv6Addr::from(o)))
        }
        _ => None,
    }
}

unsafe fn pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        String::new()
    } else {
        p.to_string().unwrap_or_default()
    }
}

fn load_adapters() -> Vec<StaticAdapter> {
    use windows::Win32::NetworkManagement::IpHelper::{GetAdaptersAddresses, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH};
    let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let mut size: u32 = 16 * 1024;
    let mut buf: Vec<u64> = Vec::new();
    let mut ok = false;
    for _ in 0..4 {
        buf = vec![0u64; (size as usize + 7) / 8];
        let r = unsafe { GetAdaptersAddresses(0, flags, None, Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH), &mut size) };
        if r == 0 {
            ok = true;
            break;
        }
        if r != 111 {
            break;
        } // 111 = ERROR_BUFFER_OVERFLOW -> retry with the size it asked for
    }
    let mut out = Vec::new();
    if !ok {
        return out;
    }
    let noise = ["Teredo", "ISATAP", "6to4", "Wi-Fi Direct Virtual", "Loopback", "Pseudo-Interface"];
    unsafe {
        let mut p = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !p.is_null() {
            let a = &*p;
            p = a.Next;
            let ift = a.IfType;
            if ift == 24 || ift == 131 || ift == 1 {
                continue; // loopback, tunnel, other
            }
            let desc = pwstr_to_string(a.Description);
            let friendly = pwstr_to_string(a.FriendlyName);
            if noise.iter().any(|n| desc.contains(n) || friendly.contains(n)) {
                continue;
            }
            let up = a.OperStatus.0 == 1;
            if !up && a.PhysicalAddressLength == 0 {
                continue;
            }
            let guid = a.AdapterName.to_string().unwrap_or_default();
            let mut ip = None;
            let mut u = a.FirstUnicastAddress;
            while !u.is_null() {
                if let Some(std::net::IpAddr::V4(v4)) = sockaddr_ip((*u).Address.lpSockaddr) {
                    if !v4.is_link_local() {
                        ip = Some(v4.to_string());
                        break;
                    }
                }
                u = (*u).Next;
            }
            let mut gateway = None;
            let mut g = a.FirstGatewayAddress;
            while !g.is_null() {
                if let Some(std::net::IpAddr::V4(v4)) = sockaddr_ip((*g).Address.lpSockaddr) {
                    gateway = Some(v4.to_string());
                    break;
                }
                g = (*g).Next;
            }
            let mut dns = Vec::new();
            let mut d = a.FirstDnsServerAddress;
            while !d.is_null() {
                match sockaddr_ip((*d).Address.lpSockaddr) {
                    Some(std::net::IpAddr::V4(v4)) => dns.push(v4.to_string()),
                    Some(std::net::IpAddr::V6(v6)) if (v6.segments()[0] & 0xFFC0) != 0xFEC0 => dns.push(v6.to_string()),
                    _ => {}
                }
                d = (*d).Next;
            }
            let n = a.PhysicalAddressLength as usize;
            let mac = if n > 0 {
                Some(a.PhysicalAddress[..n.min(8)].iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join("-"))
            } else {
                None
            };
            let kind = match ift {
                6 => "Ethernet",
                71 => "Wi-Fi",
                _ => "Other",
            };
            out.push(StaticAdapter {
                id: guid,
                name: if desc.is_empty() { friendly } else { desc },
                kind,
                luid: a.Luid.Value,
                if_index: a.Anonymous1.Anonymous.IfIndex,
                ip,
                gateway,
                dns,
                mac,
                ssid: None,
            });
        }
    }
    let rank = |k: &str| match k {
        "Ethernet" => 0,
        "Wi-Fi" => 1,
        _ => 2,
    };
    out.sort_by_key(|a| rank(a.kind)); // stable: keeps OS order inside a kind
    let ssids = wifi_ssids();
    for a in out.iter_mut().filter(|a| a.kind == "Wi-Fi") {
        let key = a.id.trim_matches(|c| c == '{' || c == '}').to_ascii_uppercase();
        a.ssid = ssids.get(&key).cloned();
    }
    out
}

/// Connected SSIDs by interface GUID (upper-case, no braces). wlanapi.dll is loaded dynamically so the app
/// still starts on systems without the WLAN feature.
fn wifi_ssids() -> HashMap<String, String> {
    use windows::core::{s, w};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    type Open = unsafe extern "system" fn(u32, *mut c_void, *mut u32, *mut HANDLE) -> u32;
    type Close = unsafe extern "system" fn(HANDLE, *mut c_void) -> u32;
    type Enum = unsafe extern "system" fn(HANDLE, *mut c_void, *mut *mut u8) -> u32;
    type Query = unsafe extern "system" fn(HANDLE, *const c_void, i32, *mut c_void, *mut u32, *mut *mut u8, *mut i32) -> u32;
    type Free = unsafe extern "system" fn(*mut c_void);
    let mut res = HashMap::new();
    unsafe {
        // System32 only: never a same-named DLL from the application folder or the current directory
        let Ok(lib) = LoadLibraryExW(w!("wlanapi.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) else { return res };
        let (o, c, e, q, f) = (
            GetProcAddress(lib, s!("WlanOpenHandle")),
            GetProcAddress(lib, s!("WlanCloseHandle")),
            GetProcAddress(lib, s!("WlanEnumInterfaces")),
            GetProcAddress(lib, s!("WlanQueryInterface")),
            GetProcAddress(lib, s!("WlanFreeMemory")),
        );
        if let (Some(o), Some(c), Some(e), Some(q), Some(f)) = (o, c, e, q, f) {
            let open: Open = std::mem::transmute(o);
            let close: Close = std::mem::transmute(c);
            let enumi: Enum = std::mem::transmute(e);
            let query: Query = std::mem::transmute(q);
            let free: Free = std::mem::transmute(f);
            let mut ver = 0u32;
            let mut h = HANDLE::default();
            if open(2, std::ptr::null_mut(), &mut ver, &mut h) == 0 {
                let mut list: *mut u8 = std::ptr::null_mut();
                if enumi(h, std::ptr::null_mut(), &mut list) == 0 && !list.is_null() {
                    let count = *(list as *const u32);
                    for i in 0..count as usize {
                        let item = list.add(8 + i * 532); // WLAN_INTERFACE_INFO: GUID(16) + desc(512) + state(4)
                        let g = std::slice::from_raw_parts(item, 16);
                        let guid = format!(
                            "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                            u32::from_le_bytes([g[0], g[1], g[2], g[3]]), u16::from_le_bytes([g[4], g[5]]), u16::from_le_bytes([g[6], g[7]]),
                            g[8], g[9], g[10], g[11], g[12], g[13], g[14], g[15]
                        );
                        let mut sz = 0u32;
                        let mut data: *mut u8 = std::ptr::null_mut();
                        let mut vt = 0i32;
                        // opcode 7 = wlan_intf_opcode_current_connection
                        if query(h, item as *const c_void, 7, std::ptr::null_mut(), &mut sz, &mut data, &mut vt) == 0 && !data.is_null() {
                            // WLAN_CONNECTION_ATTRIBUTES: state(4) mode(4) profile(512) then DOT11_SSID {len u32, bytes[32]}
                            let state = *(data as *const u32);
                            let len = *(data.add(520) as *const u32) as usize;
                            if state == 1 && len > 0 && len <= 32 {
                                let b = std::slice::from_raw_parts(data.add(524), len);
                                res.insert(guid, String::from_utf8_lossy(b).to_string());
                            }
                            free(data as *mut c_void);
                        }
                    }
                    free(list as *mut c_void);
                }
                close(h, std::ptr::null_mut());
            }
        }
        let _ = FreeLibrary(lib);
    }
    res
}

fn net_detail() -> CmdResult<Value> {
    use windows::Win32::NetworkManagement::IpHelper::{GetIfEntry2, MIB_IF_ROW2};
    use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;
    let list = {
        let mut g = NET_STATIC.lock().map_err(|_| CmdError("Network state is unavailable.".into()))?;
        match &*g {
            Some((at, l)) if at.elapsed() < Duration::from_secs(10) => l.clone(),
            _ => {
                let l = std::sync::Arc::new(load_adapters());
                *g = Some((Instant::now(), l.clone()));
                l
            }
        }
    };
    let now = Instant::now();
    let mut prev_guard = NET_PREV.lock().map_err(|_| CmdError("Network state is unavailable.".into()))?;
    let prev = prev_guard.get_or_insert_with(HashMap::new);
    let mut next: HashMap<u64, (u64, u64, Instant)> = HashMap::new();
    let mut adapters = Vec::new();
    for a in list.iter() {
        let mut row = MIB_IF_ROW2 { InterfaceLuid: NET_LUID_LH { Value: a.luid }, InterfaceIndex: a.if_index, ..Default::default() };
        if unsafe { GetIfEntry2(&mut row) }.0 != 0 {
            continue;
        }
        let up = row.OperStatus.0 == 1;
        let (rx, tx) = (row.InOctets, row.OutOctets);
        let (mut down, mut upb) = (0u64, 0u64);
        if let Some((prx, ptx, at)) = prev.get(&a.luid) {
            let dt = now.duration_since(*at).as_secs_f64();
            if dt > 0.05 && up {
                down = (rx.saturating_sub(*prx) as f64 / dt) as u64;
                upb = (tx.saturating_sub(*ptx) as f64 / dt) as u64;
            }
        }
        next.insert(a.luid, (rx, tx, now));
        let speed = row.ReceiveLinkSpeed.max(row.TransmitLinkSpeed);
        let link = if up && speed != 0 && speed != u64::MAX { json!(speed / 1_000_000) } else { Value::Null };
        adapters.push(json!({
            "id": a.id, "name": a.name, "kind": a.kind, "up": up,
            "ip": if up { json!(a.ip) } else { Value::Null },
            "gateway": if up { json!(a.gateway) } else { Value::Null },
            "dns": if up { json!(a.dns) } else { json!([]) },
            "mac": a.mac, "link_mbps": link, "down_bps": down, "up_bps": upb,
            "total_down": rx, "total_up": tx,
            "ssid": if up { json!(a.ssid) } else { Value::Null },
        }));
    }
    *prev = next;
    Ok(json!({ "adapters": adapters }))
}

// ===================================================================================================== graphics

#[tauri::command]
pub async fn get_gpu_detail() -> CmdResult<Value> {
    blocking(gpu_detail).await
}

#[derive(Clone)]
struct GpuStatic {
    id: String,
    name: String,
    vendor: &'static str,
    luid: (u32, u32), // high, low
    vram_total: Option<u64>,
    driver: Option<String>,
    driver_date: Option<String>,
    directx: Option<String>,
}

static GPU_STATIC: OnceLock<Vec<GpuStatic>> = OnceLock::new();

fn vendor_name(id: u32) -> &'static str {
    match id {
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x5143 => "Qualcomm",
        0x1414 => "Microsoft",
        _ => "Unknown",
    }
}

struct RegGpu {
    desc: String,
    match_id: String,
    version: Option<String>,
    date: Option<String>,
    vram: Option<u64>,
}

/// Display-class registry key: driver version/date and the real (64-bit) VRAM size.
fn reg_gpus() -> Vec<RegGpu> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;
    let mut out = Vec::new();
    let base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(base) else { return out };
    for sub in k.enum_keys().flatten() {
        if !sub.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(sk) = k.open_subkey(&sub) else { continue };
        let desc: String = sk.get_value("DriverDesc").unwrap_or_default();
        if desc.is_empty() {
            continue;
        }
        let match_id: String = sk.get_value("MatchingDeviceId").unwrap_or_default();
        let version: Option<String> = sk.get_value("DriverVersion").ok();
        let date: Option<String> = sk.get_value::<String, _>("DriverDate").ok().and_then(|d| convert_driver_date(&d));
        let vram = sk.get_raw_value("HardwareInformation.qwMemorySize").ok().and_then(|v| {
            if v.bytes.len() >= 8 {
                Some(u64::from_le_bytes(v.bytes[..8].try_into().unwrap()))
            } else if v.bytes.len() >= 4 {
                Some(u32::from_le_bytes(v.bytes[..4].try_into().unwrap()) as u64)
            } else {
                None
            }
        });
        out.push(RegGpu { desc, match_id: match_id.to_ascii_lowercase(), version, date, vram });
    }
    out
}

/// "3-18-2026" (M-D-YYYY, registry format) -> "2026-03-18".
fn convert_driver_date(s: &str) -> Option<String> {
    let p: Vec<&str> = s.trim().split(['-', '/']).collect();
    if p.len() != 3 {
        return None;
    }
    let (m, d, y): (u32, u32, u32) = (p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1990 {
        return None;
    }
    Some(format!("{:04}-{:02}-{:02}", y, m, d))
}

fn directx_level(adapter: &windows::Win32::Graphics::Dxgi::IDXGIAdapter1) -> Option<String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::*;
    use windows::Win32::Graphics::Direct3D12::*;
    unsafe {
        let unk: windows::core::IUnknown = adapter.cast().ok()?;
        let mut dev: Option<ID3D12Device> = None;
        D3D12CreateDevice(&unk, D3D_FEATURE_LEVEL_11_0, &mut dev).ok()?;
        let dev = dev?;
        let levels = [D3D_FEATURE_LEVEL_12_2, D3D_FEATURE_LEVEL_12_1, D3D_FEATURE_LEVEL_12_0, D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
        let mut data = D3D12_FEATURE_DATA_FEATURE_LEVELS {
            NumFeatureLevels: levels.len() as u32,
            pFeatureLevelsRequested: levels.as_ptr(),
            MaxSupportedFeatureLevel: D3D_FEATURE_LEVEL_11_0,
        };
        dev.CheckFeatureSupport(D3D12_FEATURE_FEATURE_LEVELS, &mut data as *mut _ as *mut c_void, std::mem::size_of_val(&data) as u32).ok()?;
        let fl = match data.MaxSupportedFeatureLevel {
            x if x == D3D_FEATURE_LEVEL_12_2 => "12_2",
            x if x == D3D_FEATURE_LEVEL_12_1 => "12_1",
            x if x == D3D_FEATURE_LEVEL_12_0 => "12_0",
            x if x == D3D_FEATURE_LEVEL_11_1 => "11_1",
            _ => "11_0",
        };
        Some(if fl.starts_with("11") { format!("12 (feature level {fl})") } else { format!("12 (feature level {fl})") })
    }
}

fn load_gpus() -> Vec<GpuStatic> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    let mut out = Vec::new();
    let regs = reg_gpus();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        let mut i = 0;
        while let Ok(ad) = factory.EnumAdapters1(i) {
            i += 1;
            let Ok(d) = ad.GetDesc1() else { continue };
            if d.Flags & 1 != 0 || d.VendorId == 0x1414 {
                continue; // software / Microsoft Basic Render / remote display
            }
            let name = String::from_utf16_lossy(&d.Description).trim_end_matches('\0').trim().to_string();
            let tag = format!("ven_{:04x}&dev_{:04x}", d.VendorId, d.DeviceId);
            let reg = regs.iter().find(|r| r.match_id.contains(&tag)).or_else(|| regs.iter().find(|r| r.desc.eq_ignore_ascii_case(&name)));
            let dxgi_vram = d.DedicatedVideoMemory as u64;
            let vram = [Some(dxgi_vram), reg.and_then(|r| r.vram)].into_iter().flatten().max().filter(|v| *v > 0);
            out.push(GpuStatic {
                id: format!("PCI\\VEN_{:04X}&DEV_{:04X}", d.VendorId, d.DeviceId),
                name,
                vendor: vendor_name(d.VendorId),
                luid: (d.AdapterLuid.HighPart as u32, d.AdapterLuid.LowPart),
                vram_total: vram,
                driver: reg.and_then(|r| r.version.clone()),
                driver_date: reg.and_then(|r| r.date.clone()),
                directx: directx_level(&ad),
            });
        }
    }
    // keep ids unique and stable when identical cards are installed
    let mut seen: HashMap<String, u32> = HashMap::new();
    for g in out.iter_mut() {
        let n = seen.entry(g.id.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            g.id = format!("{}#{}", g.id, n);
        }
    }
    out
}

// ---- PDH ----

struct Pdh {
    query: windows::Win32::System::Performance::PDH_HQUERY,
    eng: windows::Win32::System::Performance::PDH_HCOUNTER,
    mem: windows::Win32::System::Performance::PDH_HCOUNTER,
    primed: bool,
}
unsafe impl Send for Pdh {}
static PDH: Mutex<Option<Option<Pdh>>> = Mutex::new(None);

fn pdh_open() -> Option<Pdh> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Performance::*;
    unsafe {
        let mut q = PDH_HQUERY::default();
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut q) != 0 {
            return None;
        }
        let mut eng = PDH_HCOUNTER::default();
        let mut mem = PDH_HCOUNTER::default();
        if PdhAddEnglishCounterW(q, w!("\\GPU Engine(*)\\Utilization Percentage"), 0, &mut eng) != 0 {
            let _ = PdhCloseQuery(q);
            return None;
        }
        // memory counter is optional
        if PdhAddEnglishCounterW(q, w!("\\GPU Adapter Memory(*)\\Dedicated Usage"), 0, &mut mem) != 0 {
            mem = PDH_HCOUNTER::default();
        }
        Some(Pdh { query: q, eng, mem, primed: false })
    }
}

/// All (instance, value) pairs for a wildcard counter.
fn pdh_array(counter: windows::Win32::System::Performance::PDH_HCOUNTER) -> Vec<(String, f64)> {
    use windows::Win32::System::Performance::*;
    let mut out = Vec::new();
    unsafe {
        let mut size = 0u32;
        let mut count = 0u32;
        let r = PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
        if r != 0x800007D2 || size == 0 {
            return out; // PDH_MORE_DATA expected
        }
        let mut buf = vec![0u64; (size as usize + 7) / 8];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
            return out;
        }
        for i in 0..count as usize {
            let it = &*items.add(i);
            if it.FmtValue.CStatus != 0 && it.FmtValue.CStatus != 0x00000001 {
                continue; // not PDH_CSTATUS_VALID_DATA / NEW_DATA
            }
            let name = it.szName.to_string().unwrap_or_default();
            out.push((name, it.FmtValue.Anonymous.doubleValue));
        }
    }
    out
}

/// "...luid_0x00000000_0x0000d3c4_phys_0..." -> (high, low)
fn parse_luid(inst: &str) -> Option<(u32, u32)> {
    let i = inst.find("luid_0x")?;
    let rest = &inst[i + 5..];
    let mut parts = rest.split('_');
    let hi = parts.next()?.trim_start_matches("0x");
    let lo = parts.next()?.trim_start_matches("0x");
    Some((u32::from_str_radix(hi, 16).ok()?, u32::from_str_radix(lo, 16).ok()?))
}

fn engine_type(inst: &str) -> Option<&str> {
    inst.split("_engtype_").nth(1)
}

#[derive(Default, Clone)]
struct Live {
    engines: HashMap<String, f64>, // raw engine type -> summed percent
    vram_used: Option<f64>,
}

fn pdh_sample() -> Option<HashMap<(u32, u32), Live>> {
    use windows::Win32::System::Performance::PdhCollectQueryData;
    let mut g = PDH.lock().ok()?;
    if g.is_none() {
        *g = Some(pdh_open());
    }
    let p = g.as_mut()?.as_mut()?;
    unsafe {
        if PdhCollectQueryData(p.query) != 0 {
            return None;
        }
        if !p.primed {
            std::thread::sleep(Duration::from_millis(250)); // rates need two samples
            if PdhCollectQueryData(p.query) != 0 {
                return None;
            }
            p.primed = true;
        }
    }
    let mut map: HashMap<(u32, u32), Live> = HashMap::new();
    for (inst, v) in pdh_array(p.eng) {
        if let (Some(l), Some(t)) = (parse_luid(&inst), engine_type(&inst)) {
            *map.entry(l).or_default().engines.entry(t.to_string()).or_insert(0.0) += v.max(0.0);
        }
    }
    if !p.mem.0.is_null() {
        for (inst, v) in pdh_array(p.mem) {
            if let Some(l) = parse_luid(&inst) {
                let e = map.entry(l).or_default();
                e.vram_used = Some(e.vram_used.unwrap_or(0.0) + v.max(0.0));
            }
        }
    }
    Some(map)
}

fn engine_label(raw: &str) -> Option<&'static str> {
    let l = raw.to_ascii_lowercase();
    if l.starts_with("3d") {
        Some("3D")
    } else if l.starts_with("copy") {
        Some("Copy")
    } else if l.starts_with("videodecode") {
        Some("Video decode")
    } else if l.starts_with("videoencode") {
        Some("Video encode")
    } else if l.starts_with("compute") || l.starts_with("cuda") {
        Some("Compute")
    } else {
        None
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn gpu_detail() -> CmdResult<Value> {
    let gpus = GPU_STATIC.get_or_init(load_gpus);
    let live = pdh_sample();
    let mut adapters = Vec::new();
    let mut any_live = false;
    for g in gpus {
        let l = live.as_ref().and_then(|m| m.get(&g.luid)).filter(|l| !l.engines.is_empty());
        let (util, engines, vram_used) = match l {
            Some(l) => {
                any_live = true;
                let util = l.engines.values().cloned().fold(0.0f64, f64::max).min(100.0);
                let mut per: Vec<(&str, f64)> = ["3D", "Copy", "Video decode", "Video encode", "Compute"].iter().map(|n| (*n, 0.0)).collect();
                for (raw, v) in &l.engines {
                    if let Some(lab) = engine_label(raw) {
                        if let Some(e) = per.iter_mut().find(|e| e.0 == lab) {
                            e.1 += v;
                        }
                    }
                }
                let eng: Vec<Value> = per.iter().map(|(n, v)| json!({ "name": n, "percent": round1(v.min(100.0)) })).collect();
                (json!(round1(util)), json!(eng), l.vram_used.map(|v| json!(v as u64)).unwrap_or(Value::Null))
            }
            None => (Value::Null, Value::Null, Value::Null),
        };
        adapters.push(json!({
            "id": g.id, "name": g.name, "vendor": g.vendor, "vram_total": g.vram_total,
            "driver": g.driver, "driver_date": g.driver_date, "directx": g.directx,
            "util_percent": util, "vram_used": vram_used, "temperature_c": null,
            "clock_mhz": null, "power_w": null, "engines": engines,
        }));
    }
    Ok(json!({ "live_supported": any_live, "adapters": adapters }))
}

// ========================================================================================================= tests

#[cfg(test)]
mod tests {
    use super::*;

    fn s17(slot: &str, size: u16, ty: u8, speed: u16, cfg: u16, manu: &str, part: &str) -> Vec<u8> {
        let mut f = vec![0u8; 0x22];
        f[0] = 17;
        f[1] = 0x22;
        f[0x0C..0x0E].copy_from_slice(&size.to_le_bytes());
        f[0x0E] = 0x09; // DIMM
        f[0x10] = 1;
        f[0x12] = ty;
        f[0x15..0x17].copy_from_slice(&speed.to_le_bytes());
        f[0x17] = 2;
        f[0x1A] = 3;
        f[0x20..0x22].copy_from_slice(&cfg.to_le_bytes());
        if size == 0 {
            f[0x10] = 0;
            f[0x17] = 0;
            f[0x1A] = 0;
            f.extend([0, 0]);
            return f;
        }
        for s in [slot, manu, part] {
            f.extend_from_slice(s.as_bytes());
            f.push(0);
        }
        f.push(0);
        f
    }

    fn build() -> Vec<u8> {
        let mut t = Vec::new();
        let mut s16 = vec![0u8; 0x17];
        s16[0] = 16;
        s16[1] = 0x17;
        s16[5] = 3;
        s16[0x0D..0x0F].copy_from_slice(&4u16.to_le_bytes());
        t.extend(s16);
        t.extend([0, 0]);
        t.extend(s17("DIMM_A1", 8192, 0x1A, 3200, 3200, "Kingston", "KF432C16BB/8"));
        t.extend(s17("DIMM_A2", 0, 0x02, 0, 0, "", ""));
        t.extend(s17("DIMM_B1", 8192, 0x1A, 3200, 2933, "Kingston", "KF432C16BB/8"));
        t.extend(s17("DIMM_B2", 0, 0x02, 0, 0, "", ""));
        t.extend([127, 4, 0, 0, 0, 0]);
        let mut raw = vec![0u8, 3, 3, 0];
        raw.extend((t.len() as u32).to_le_bytes());
        raw.extend(t);
        raw
    }

    #[test]
    fn smbios_parses_modules() {
        let hw = parse_smbios(&build());
        assert_eq!(hw.slots_total, Some(4));
        assert_eq!(hw.entries, 4);
        let j = hw_to_json(&hw);
        assert_eq!(j["type"], "DDR4");
        assert_eq!(j["speed_mhz"], 3200);
        assert_eq!(j["slots_used"], 2);
        assert_eq!(j["form_factor"], "DIMM");
        assert_eq!(j["modules"][0]["slot"], "DIMM_A1");
        assert_eq!(j["modules"][0]["size"], 8u64 * 1024 * 1024 * 1024);
        assert_eq!(j["modules"][0]["manufacturer"], "Kingston");
        assert_eq!(j["modules"][1]["speed_mhz"], 2933);
    }

    #[test]
    fn smbios_extended_size_and_garbage() {
        let mut t = s17("A", 0x7FFF, 0x22, 5600, 4800, "Hynix", "X");
        t[0x1C..0x20].copy_from_slice(&32768u32.to_le_bytes());
        t.extend([127, 4, 0, 0, 0, 0]);
        let mut raw = vec![0u8, 3, 3, 0];
        raw.extend((t.len() as u32).to_le_bytes());
        raw.extend(t);
        let j = hw_to_json(&parse_smbios(&raw));
        assert_eq!(j["type"], "DDR5");
        assert_eq!(j["modules"][0]["size"], 32u64 * 1024 * 1024 * 1024);
        assert_eq!(j["speed_mhz"], 4800);
        // truncated / empty input never panics
        assert_eq!(parse_smbios(&[]), MemHw::default());
        let _ = parse_smbios(&raw[..20]);
        assert!(hw_to_json(&MemHw::default())["slots_used"].is_null());
    }

    #[test]
    fn luid_and_date_parsing() {
        assert_eq!(parse_luid("pid_4_luid_0x00000000_0x0000d3c4_phys_0_eng_0_engtype_3D"), Some((0, 0xd3c4)));
        assert_eq!(engine_type("pid_4_luid_0x0_0x1_phys_0_eng_0_engtype_VideoDecode"), Some("VideoDecode"));
        assert_eq!(convert_driver_date("3-18-2026").as_deref(), Some("2026-03-18"));
        assert_eq!(convert_driver_date("junk"), None);
        assert_eq!(engine_label("Compute_0"), Some("Compute"));
    }

    #[test]
    #[ignore]
    fn smoke_mem() {
        println!("{}", serde_json::to_string_pretty(&mem_detail().unwrap()).unwrap());
        println!("{}", serde_json::to_string_pretty(&mem_hw()).unwrap());
    }

    #[test]
    #[ignore]
    fn smoke_net() {
        net_detail().unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        println!("{}", serde_json::to_string_pretty(&net_detail().unwrap()).unwrap());
    }

    #[test]
    #[ignore]
    fn smoke_gpu() {
        gpu_detail().unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        println!("{}", serde_json::to_string_pretty(&gpu_detail().unwrap()).unwrap());
    }
}
