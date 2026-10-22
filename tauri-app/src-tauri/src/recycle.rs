// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Move one file or folder to the Recycle Bin, and FAIL (leaving it untouched) when it cannot be recycled.
//!
//! Why not `SHFileOperationW(FO_DELETE, FOF_ALLOWUNDO)`: with no UI allowed, Windows silently deletes items that are
//! too large for the Recycle Bin (or that live on a volume without one) while the app promises "restorable".
//! `IFileOperation` with `FOFX_RECYCLEONDELETE` makes such a delete fail instead.
//!
//! API: `recycle(path) -> Result<(), String>`. The caller must have validated the path (protection rules) first.
use windows::core::PCWSTR;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName, FILEOPERATION_FLAGS, FOFX_RECYCLEONDELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION,
    FOF_NOERRORUI, FOF_SILENT,
};

pub const TOO_LARGE_MSG: &str = "too large for the Recycle Bin (or this drive has none) - not deleted";

/// Balances CoInitializeEx on this (possibly pooled) thread.
struct ComGuard(bool);
impl ComGuard {
    fn new() -> Result<Self, String> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_ok() {
            Ok(ComGuard(true)) // S_OK or S_FALSE: both need a matching CoUninitialize
        } else if hr == RPC_E_CHANGED_MODE {
            Ok(ComGuard(false)) // thread already runs in another apartment model: usable, nothing to undo
        } else {
            Err("could not start the Windows shell".into())
        }
    }
}
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

fn exists(path: &str) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

// ---------------------------------------------------------------- pre-flight (can this item be recycled at all?)

/// `\?\Volume{GUID}\` -> `{GUID}`.
fn volume_guid(name: &str) -> Option<String> {
    let s = name.find('{')?;
    let e = name[s..].find('}')? + s;
    Some(name[s..=e].to_string())
}

/// Drive root of an absolute `X:\...` path.
fn drive_root(path: &str) -> Option<String> {
    let b = path.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == 92 || b[2] == b'/') {
        Some(format!("{}:\\", (b[0] as char).to_ascii_uppercase()))
    } else {
        None
    }
}

/// Total size of `path`, stopping early once it exceeds `limit` or after `max_secs`. (size so far, finished)
fn size_upto(path: &str, limit: u64, max_secs: u64) -> (u64, bool) {
    let start = std::time::Instant::now();
    let mut total = 0u64;
    for (n, e) in walkdir::WalkDir::new(path).follow_links(false).into_iter().flatten().enumerate() {
        if e.file_type().is_file() {
            total = total.saturating_add(e.metadata().map(|m| m.len()).unwrap_or(0));
            if total > limit {
                return (total, false);
            }
        }
        if n % 256 == 255 && start.elapsed().as_secs() >= max_secs {
            return (total, false);
        }
    }
    (total, true)
}

/// Refuse up front what Windows would permanently delete: drives without a Recycle Bin, a bin that is switched off,
/// and items bigger than the bin's maximum size.
fn preflight(path: &str) -> Result<String, String> {
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeNameForVolumeMountPointW};
    use winreg::enums::*;
    use winreg::RegKey;
    let root = drive_root(path).ok_or_else(|| "this location has no Recycle Bin - not deleted".to_string())?;
    let wroot: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    // DRIVE_FIXED = 3: removable and network drives have no Recycle Bin
    if unsafe { GetDriveTypeW(PCWSTR(wroot.as_ptr())) } != 3 {
        return Err("this drive has no Recycle Bin - not deleted".into());
    }
    let mut name = [0u16; 64];
    let guid = unsafe { GetVolumeNameForVolumeMountPointW(PCWSTR(wroot.as_ptr()), &mut name) }
        .ok()
        .and_then(|_| volume_guid(&String::from_utf16_lossy(&name[..name.iter().position(|&c| c == 0).unwrap_or(name.len())])))
        .ok_or_else(|| "this drive has no Recycle Bin - not deleted".to_string())?;
    let cu = RegKey::predef(HKEY_CURRENT_USER);
    let nuke = |p: &str, v: &str| cu.open_subkey_with_flags(p, KEY_READ).ok().and_then(|k| k.get_value::<u32, _>(v).ok()).unwrap_or(0) != 0;
    let bb = r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket";
    let vol = format!(r"{bb}\Volume\{guid}");
    if nuke(&vol, "NukeOnDelete") || nuke(bb, "NukeOnDelete") || nuke(r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer", "NoRecycleFiles") {
        return Err("the Recycle Bin is turned off for this drive - not deleted".into());
    }
    let mut total = 0u64;
    let _ = unsafe { GetDiskFreeSpaceExW(PCWSTR(wroot.as_ptr()), None, Some(&mut total), None) };
    let max_bytes = match cu.open_subkey_with_flags(&vol, KEY_READ).ok().and_then(|k| k.get_value::<u32, _>("MaxCapacity").ok()) {
        Some(mb) if mb > 0 => (mb as u64) * 1024 * 1024,
        _ => total / 20, // no explicit setting: assume a conservative 5% of the drive
    };
    if max_bytes > 0 {
        let (size, _) = size_upto(path, max_bytes, 4);
        if size > max_bytes {
            return Err(TOO_LARGE_MSG.into());
        }
    }
    Ok(root)
}

fn bin_items(root: &str) -> Option<i64> {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let w: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    unsafe { SHQueryRecycleBinW(PCWSTR(w.as_ptr()), &mut info) }.ok().map(|_| info.i64NumItems)
}

/// Move `path` (file or folder) to the Recycle Bin. `Ok(())` means it is in the bin (restorable).
/// `Err` means it was NOT deleted (too large / no bin on that drive / in use / cancelled).
pub fn recycle(path: &str) -> Result<(), String> {
    let path = path.trim_end_matches(['\\', '/']);
    if path.is_empty() || path.contains('\0') {
        return Err("invalid path".into());
    }
    if !exists(path) {
        return Err("not found".into());
    }
    let root = preflight(path)?;
    let before = bin_items(&root);
    let _com = ComGuard::new()?;
    let wpath: Vec<u16> = path.replace('/', "\\").encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        (|| -> windows::core::Result<bool> {
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wpath.as_ptr()), None)?;
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            op.SetOperationFlags(FILEOPERATION_FLAGS(
                FOF_ALLOWUNDO.0 | FOFX_RECYCLEONDELETE.0 | FOF_NOCONFIRMATION.0 | FOF_SILENT.0 | FOF_NOERRORUI.0,
            ))?;
            op.DeleteItem(&item, None)?;
            op.PerformOperations()?;
            Ok(op.GetAnyOperationsAborted()?.as_bool())
        })()
    };
    let gone = !exists(path);
    match result {
        Ok(false) if gone => match (before, bin_items(&root)) {
            (Some(b), Some(a)) if a <= b => {
                Err("it was removed, but Windows did not put it in the Recycle Bin (it may have been deleted permanently)".into())
            }
            _ => Ok(()),
        },
        Ok(false) => Err("Windows did not move it to the Recycle Bin (it may be in use)".into()),
        Ok(true) => Err(if gone { "cancelled part-way".into() } else { "cancelled".into() }),
        Err(e) => {
            if gone {
                Err("it was removed, but Windows did not confirm the Recycle Bin (it may have been deleted permanently)".into())
            } else if e.code().0 as u32 == 0x8027_0020 {
                Err(TOO_LARGE_MSG.into())
            } else {
                Err(format!("could not move it to the Recycle Bin ({})", e.message().trim()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pk_recycle_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn small_folder_is_recycled() {
        let d = tmp("small");
        let sub = d.join("RecycleMe");
        std::fs::create_dir_all(sub.join("inner")).unwrap();
        std::fs::write(sub.join("inner").join("a.txt"), b"hello pk_recycle_test").unwrap();
        let r = recycle(&sub.to_string_lossy());
        assert_eq!(r, Ok(()), "{r:?}");
        assert!(!sub.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn helpers() {
        assert_eq!(volume_guid(r"\?\Volume{1234-abcd}\").as_deref(), Some("{1234-abcd}"));
        assert_eq!(volume_guid("nope"), None);
        assert_eq!(drive_root(r"c:\x\y").as_deref(), Some(r"C:\"));
        assert_eq!(drive_root(r"\server\share\x"), None);
        assert_eq!(drive_root("relative"), None);
    }

    #[test]
    fn size_cap_stops_early() {
        let d = tmp("size");
        std::fs::write(d.join("a.bin"), vec![0u8; 5000]).unwrap();
        std::fs::write(d.join("b.bin"), vec![0u8; 5000]).unwrap();
        let (s, done) = size_upto(&d.to_string_lossy(), 100, 4);
        assert!(s > 100 && !done);
        let (s, done) = size_upto(&d.to_string_lossy(), 1 << 30, 4);
        assert!(s == 10000 && done);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_path_is_an_error_not_a_panic() {
        assert!(recycle(r"C:\definitely\not\here\pk").is_err());
        assert!(recycle("").is_err());
    }

    /// A drive with no Recycle Bin must FAIL and keep the file. `subst` maps a temp dir to a free drive letter.
    #[test]
    fn unrecyclable_item_is_kept() {
        let letter = ('P'..='Z').rev().find(|c| !std::path::Path::new(&format!("{c}:\\")).exists());
        let Some(letter) = letter else { return };
        let d = tmp("subst");
        let ok = std::process::Command::new("subst").arg(format!("{letter}:")).arg(&d).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            let _ = std::fs::remove_dir_all(&d);
            return;
        }
        let f = format!("{letter}:\\keep_me.txt");
        std::fs::write(&f, b"precious").unwrap();
        let r = recycle(&f);
        let still = std::path::Path::new(&f).exists();
        let kept = d.join("keep_me.txt").exists();
        let _ = std::process::Command::new("subst").arg(format!("{letter}:")).arg("/d").status();
        println!("subst result: {r:?}, file still there: {still}, on disk: {kept}");
        let _ = std::fs::remove_dir_all(&d);
        match r {
            Ok(()) => assert!(!still),
            Err(_) => assert!(still && kept, "an item reported as not deleted must still exist"),
        }
    }
}
