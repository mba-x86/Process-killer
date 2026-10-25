// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Shared commands (docs/api/SHARED.md): window, overview, elevation.
use serde::Serialize;
use tauri::{AppHandle, Manager};
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::util::{self, CmdResult};

#[tauri::command]
pub fn app_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[derive(Serialize)]
pub struct Overview {
    hostname: String,
    os: String,
    model: String,
    uptime_secs: u64,
    is_admin: bool,
    version: String,
    copyright: String,
}

fn reg_string(path: &str, name: &str) -> String {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(path)
        .and_then(|k| k.get_value::<String, _>(name))
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_overview() -> Overview {
    let cur = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let mut os = reg_string(cur, "ProductName");
    let build: u32 = reg_string(cur, "CurrentBuildNumber").parse().unwrap_or(0);
    if build >= 22000 {
        os = os.replace("Windows 10", "Windows 11"); // the registry keeps saying "10" on Windows 11
    }
    Overview {
        hostname: sysinfo::System::host_name().unwrap_or_default(),
        os: format!("Microsoft {os}"),
        model: reg_string(r"HARDWARE\DESCRIPTION\System\BIOS", "SystemProductName"),
        uptime_secs: sysinfo::System::uptime(),
        is_admin: util::is_admin(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        copyright: "© 2026 mba-x86".to_string(),
    }
}

/// Re-launch this exe elevated (UAC prompt), then quit.
#[tauri::command]
pub fn relaunch_as_admin(app: AppHandle) -> CmdResult<()> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let exe = std::env::current_exe()?;
    let verb = util::wide("runas");
    let file = util::wide(&exe.to_string_lossy());
    let r = unsafe { ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(file.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    if (r.0 as isize) > 32 {
        app.exit(0);
        Ok(())
    } else {
        util::err("The elevation request was cancelled or failed.")
    }
}
