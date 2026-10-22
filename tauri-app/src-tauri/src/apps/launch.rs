// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Starting an uninstaller from its registry command line - WITHOUT a shell.
//!
//! The 1.x app ran `cmd /c <string>`: a value such as `"C:\x\u.exe" & calc` (HKCU is writable without elevation)
//! then ran the extra command with this app's rights. Here the string is split into program + arguments, the program
//! must be an absolute path (or msiexec / rundll32, resolved to System32) and is started directly; the argument text
//! is passed through verbatim (uninstallers expect their own quoting) but is never interpreted by a shell.
//! A hung uninstaller is ended (with its child processes) after the timeout.
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::parse::{exe_of, expand_env, is_abs};
use super::system::{descendants, kill_pid, snap_processes};
use crate::util::{wide, CREATE_NO_WINDOW};

const ERROR_ELEVATION_REQUIRED: i32 = 740;

/// (program, argument text) from an uninstall command line. `None` when no program can be identified.
pub fn split_command(cmd: &str) -> Option<(String, String)> {
    let c = expand_env(cmd);
    let t = c.trim_start();
    if let Some(r) = t.strip_prefix('"') {
        if let Some(e) = r.find('"') {
            if e > 0 {
                return Some((r[..e].to_string(), r[e + 1..].to_string()));
            }
        }
    }
    let exe = exe_of(t);
    if !exe.is_empty() && t.starts_with(exe.as_str()) {
        return Some((exe.clone(), t[exe.len()..].to_string()));
    }
    // "msiexec /x {GUID}" / "rundll32 shell32.dll,..." (no extension)
    let tok = t.split_whitespace().next().unwrap_or("");
    let low = tok.to_ascii_lowercase();
    if low == "msiexec" || low == "rundll32" {
        return Some((format!("{low}.exe"), t[tok.len()..].to_string()));
    }
    None
}

fn system32(name: &str) -> String {
    let win = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    format!(r"{}\System32\{}", win.trim_end_matches('\\'), name)
}

/// The program to start and the verbatim argument text.
pub fn plan(cmd: &str) -> Result<(String, String), String> {
    let bad = || "Could not understand the uninstall command.".to_string();
    if cmd.contains('\0') {
        return Err(bad());
    }
    let (exe, args) = split_command(cmd).ok_or_else(bad)?;
    let low = exe.to_ascii_lowercase();
    if is_abs(&exe) {
        return Ok((exe, args));
    }
    if matches!(low.as_str(), "msiexec.exe" | "rundll32.exe") {
        return Ok((system32(&low), args));
    }
    Err(bad())
}

enum Running {
    Child(Child),
    Handle(windows::Win32::Foundation::HANDLE),
}

impl Running {
    fn pid(&self) -> u32 {
        match self {
            Running::Child(c) => c.id(),
            Running::Handle(h) => unsafe { windows::Win32::System::Threading::GetProcessId(*h) },
        }
    }
    /// Some(exit code) once finished.
    fn poll(&mut self) -> Result<Option<i32>, String> {
        match self {
            Running::Child(c) => c.try_wait().map(|o| o.map(|s| s.code().unwrap_or(-1))).map_err(|e| format!("Lost track of the uninstaller: {e}")),
            Running::Handle(h) => unsafe {
                use windows::Win32::Foundation::WAIT_OBJECT_0;
                use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
                if WaitForSingleObject(*h, 0) == WAIT_OBJECT_0 {
                    let mut code = 0u32;
                    let _ = GetExitCodeProcess(*h, &mut code);
                    Ok(Some(code as i32))
                } else {
                    Ok(None)
                }
            },
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        if let Running::Handle(h) = self {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(*h);
            }
        }
    }
}

/// Needed when the uninstaller's manifest demands elevation and this process is not elevated (CreateProcess says 740).
fn shell_execute(exe: &str, args: &str) -> Result<Running, String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    let (file, params) = (wide(exe), wide(args.trim()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: 1,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(|_| "The uninstaller could not be started (administrator approval was declined?).".to_string())?;
    if info.hProcess.is_invalid() {
        return Err("The uninstaller could not be tracked after it was started.".into());
    }
    Ok(Running::Handle(info.hProcess))
}

/// End a process and everything it started (children first). Never touches protected processes or ourselves.
fn kill_tree(pid: u32) {
    let all = snap_processes();
    let mut order = descendants(&all, pid);
    order.reverse(); // deepest first, root last
    for p in order {
        if p == std::process::id() {
            continue;
        }
        if let Some(pr) = all.iter().find(|x| x.pid == p) {
            if crate::protect::is_protected_process(&pr.name, &pr.exe) {
                continue;
            }
        }
        let _ = kill_pid(p);
    }
}

/// Start the uninstall command and wait for it. Ok(exit code). `Err` = could not start / timed out (the tree is ended).
pub fn run(cmd: &str, timeout: Duration) -> Result<i32, String> {
    let (exe, args) = plan(cmd)?;
    let mut c = Command::new(&exe);
    if !args.trim().is_empty() {
        c.raw_arg(args.trim());
    }
    let mut running = match c.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).spawn() {
        Ok(ch) => Running::Child(ch),
        Err(e) if e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) => shell_execute(&exe, &args)?,
        Err(e) => return Err(format!("Could not start the uninstaller: {e}")),
    };
    let start = Instant::now();
    loop {
        if let Some(code) = running.poll()? {
            return Ok(code);
        }
        if start.elapsed() > timeout {
            kill_tree(running.pid());
            return Err(format!("The uninstaller did not finish within {} minutes and was ended.", timeout.as_secs() / 60));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting() {
        assert_eq!(split_command(r#""C:\Program Files\A\unins000.exe" /SILENT"#), Some((r"C:\Program Files\A\unins000.exe".into(), " /SILENT".into())));
        assert_eq!(split_command(r"C:\Program Files (x86)\My App\unins000.exe /VERYSILENT"), Some((r"C:\Program Files (x86)\My App\unins000.exe".into(), " /VERYSILENT".into())));
        assert_eq!(split_command("MsiExec.exe /X{ABC}"), Some(("MsiExec.exe".into(), " /X{ABC}".into())));
        assert_eq!(split_command("msiexec /x {ABC}"), Some(("msiexec.exe".into(), " /x {ABC}".into())));
        assert_eq!(split_command("rundll32 shell32.dll,Foo"), Some(("rundll32.exe".into(), " shell32.dll,Foo".into())));
        assert_eq!(split_command("uninstall-me now"), None);
        assert_eq!(split_command(""), None);
    }

    #[test]
    fn plan_never_uses_a_shell() {
        // the metacharacters stay in the argument text; the program is what the first token says
        let (exe, args) = plan(r#""C:\x\u.exe" /S & calc.exe"#).unwrap();
        assert_eq!(exe, r"C:\x\u.exe");
        assert!(args.contains("& calc.exe"));
        let (exe, _) = plan("msiexec /x {1}").unwrap();
        assert!(exe.to_lowercase().ends_with(r"\system32\msiexec.exe"));
        // relative / bare programs other than msiexec & rundll32 are refused (no PATH / current-directory search)
        assert!(plan("evil.exe /S").is_err());
        assert!(plan(r"..\evil.exe /S").is_err());
        assert!(plan(r#""\\server\share\u.exe" /S"#).is_err());
        assert!(plan("").is_err());
        assert!(plan("C:\\x\\u.exe\0 /S").is_err());
        // "& calc" in front is not a program either
        assert!(plan("& calc.exe").is_err());
    }

    #[test]
    fn metacharacters_are_not_executed() {
        // ping.exe does not interpret `&`; with a shell in between the second command would create the marker.
        let marker = std::env::temp_dir().join(format!("pk_launch_marker_{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let cmd = format!(r#""{}" -n 1 127.0.0.1 & cmd /c echo hi > "{}""#, system32("ping.exe"), marker.display());
        let _ = run(&cmd, Duration::from_secs(30)).unwrap();
        assert!(!marker.exists(), "text after & must not be run as a second command");
    }

    #[test]
    fn exit_code_and_timeout_kill() {
        let rc = run(&format!(r#""{}" /c exit 5"#, system32("cmd.exe")), Duration::from_secs(30)).unwrap();
        assert_eq!(rc, 5);
        let t = Instant::now();
        let r = run(&format!(r#""{}" -n 60 127.0.0.1"#, system32("ping.exe")), Duration::from_secs(1));
        assert!(r.unwrap_err().contains("did not finish"));
        assert!(t.elapsed() < Duration::from_secs(20));
        assert!(run(r"C:\definitely\missing\u.exe /S", Duration::from_secs(5)).is_err());
    }
}
