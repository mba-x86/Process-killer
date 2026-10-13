"""Process Killer - RAM boost, junk cleaning and disk analysis (no UI)."""
import ctypes
import glob
import heapq
import os
import time
from ctypes import wintypes

import psutil

import core

MB = 1024 * 1024


# ------------------------------------------------------------------ RAM

def ram_info():
    vm = psutil.virtual_memory()
    return {"total": vm.total, "available": vm.available, "used": vm.total - vm.available,
            "percent": vm.percent}


def _enable_privilege(name):
    try:
        adv, k32 = ctypes.WinDLL("advapi32", use_last_error=True), ctypes.WinDLL("kernel32")
        k32.GetCurrentProcess.restype = wintypes.HANDLE
        token = wintypes.HANDLE()
        if not adv.OpenProcessToken(k32.GetCurrentProcess(), 0x28, ctypes.byref(token)):
            return False

        class LUID(ctypes.Structure):
            _fields_ = [("lo", wintypes.DWORD), ("hi", wintypes.LONG)]

        class TP(ctypes.Structure):
            _fields_ = [("n", wintypes.DWORD), ("luid", LUID), ("attr", wintypes.DWORD)]
        luid = LUID()
        if not adv.LookupPrivilegeValueW(None, name, ctypes.byref(luid)):
            return False
        tp = TP(1, luid, 2)
        adv.AdjustTokenPrivileges(token, False, ctypes.byref(tp), 0, None, None)
        return ctypes.get_last_error() == 0
    except Exception:  # noqa: BLE001
        return False


def trim_working_sets():
    """Ask Windows to move idle pages of every accessible process to the standby list."""
    k32, psapi = ctypes.WinDLL("kernel32"), ctypes.WinDLL("psapi")
    k32.OpenProcess.restype = wintypes.HANDLE
    k32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    k32.CloseHandle.argtypes = [wintypes.HANDLE]
    psapi.EmptyWorkingSet.argtypes = [wintypes.HANDLE]
    n = 0
    for pid in psutil.pids():
        if pid in (0, 4):
            continue
        h = k32.OpenProcess(0x0400 | 0x0100, False, pid)  # QUERY_INFORMATION | SET_QUOTA
        if h:
            if psapi.EmptyWorkingSet(h):
                n += 1
            k32.CloseHandle(h)
    return n


def purge_standby():
    """Flush the standby cache (admin only). Returns True on success."""
    if not core.is_admin() or not _enable_privilege("SeProfileSingleProcessPrivilege"):
        return False
    cmd = ctypes.c_int(4)  # MemoryPurgeStandbyList
    return ctypes.WinDLL("ntdll").NtSetSystemInformation(80, ctypes.byref(cmd), ctypes.sizeof(cmd)) == 0


def boost_ram():
    before = ram_info()["available"]
    trimmed = trim_working_sets()
    purged = purge_standby()
    time.sleep(0.8)
    after = ram_info()
    return {"freed": max(0, after["available"] - before), "trimmed": trimmed, "purged": purged,
            "percent": after["percent"]}


# ------------------------------------------------------------------ junk cleaner

def _env(n):
    return os.environ.get(n, "")


def _browser_caches():
    paths = []
    la = _env("LOCALAPPDATA")
    for base in (r"Google\Chrome", r"Microsoft\Edge", r"BraveSoftware\Brave-Browser"):
        ud = os.path.join(la, base, "User Data")
        for prof in glob.glob(os.path.join(ud, "Default")) + glob.glob(os.path.join(ud, "Profile *")):
            for sub in ("Cache", "Code Cache", "GPUCache"):
                paths.append(os.path.join(prof, sub))
    paths += glob.glob(os.path.join(la, "Mozilla", "Firefox", "Profiles", "*", "cache2"))
    return [p for p in paths if os.path.isdir(p)]


def junk_categories():
    """id, title, description, default-ticked, folders (None = Recycle Bin)."""
    user_temp = {os.path.normcase(p): p for p in (_env("TEMP"), os.path.join(_env("LOCALAPPDATA"), "Temp")) if p}
    win = _env("SystemRoot") or r"C:\Windows"
    return [
        {"id": "user_temp", "title": "User temp files", "desc": "Leftovers from apps and installers",
         "on": True, "paths": list(user_temp.values())},
        {"id": "win_temp", "title": "Windows temp files", "desc": "System temp folder (needs Administrator)",
         "on": True, "paths": [os.path.join(win, "Temp")]},
        {"id": "dumps", "title": "Crash dumps & error reports", "desc": "Memory dumps and Windows error reports",
         "on": True, "paths": [os.path.join(_env("LOCALAPPDATA"), "CrashDumps"),
                               os.path.join(_env("ProgramData"), "Microsoft", "Windows", "WER", "ReportArchive"),
                               os.path.join(_env("ProgramData"), "Microsoft", "Windows", "WER", "ReportQueue"),
                               os.path.join(win, "Minidump")]},
        {"id": "recycle", "title": "Recycle Bin", "desc": "Permanently empties the bin", "on": True, "paths": None},
        {"id": "browser", "title": "Browser caches", "desc": "Chrome, Edge, Brave, Firefox (you stay logged in)",
         "on": False, "paths": _browser_caches()},
    ]


class _RBInfo(ctypes.Structure):
    _fields_ = [("cbSize", wintypes.DWORD), ("size", ctypes.c_int64), ("items", ctypes.c_int64)]


def recycle_size():
    info = _RBInfo(ctypes.sizeof(_RBInfo), 0, 0)
    if ctypes.windll.shell32.SHQueryRecycleBinW(None, ctypes.byref(info)) == 0:
        return info.size, info.items
    return 0, 0


def scan_junk(cat):
    """-> (bytes, files)"""
    if cat["paths"] is None:
        return recycle_size()
    total = files = 0
    for root in cat["paths"]:
        for dp, _dn, fn in os.walk(root, onerror=lambda e: None):
            for f in fn:
                try:
                    total += os.lstat(os.path.join(dp, f)).st_size
                    files += 1
                except OSError:
                    pass
    return total, files


class Progress:
    def __init__(self, total=0):
        self.total, self.done, self.failed = total, 0, 0


def clean_junk(cat, prog):
    """Delete the category's files. Files in use are skipped. -> bytes freed"""
    if cat["paths"] is None:
        size, _ = recycle_size()
        ctypes.windll.shell32.SHEmptyRecycleBinW(None, None, 0x7)  # no confirm/progress/sound
        prog.done = prog.total or size
        return size
    freed = 0
    for root in cat["paths"]:
        for dp, dn, fn in os.walk(root, topdown=False, onerror=lambda e: None):
            for f in fn:
                p = os.path.join(dp, f)
                try:
                    sz = os.lstat(p).st_size
                    os.remove(p)
                    freed += sz
                    prog.done += sz
                except OSError:
                    prog.failed += 1
            for d in dn:
                try:
                    os.rmdir(os.path.join(dp, d))
                except OSError:
                    pass
    return freed


# ------------------------------------------------------------------ partitions & disk scan

def list_partitions():
    out = []
    for p in psutil.disk_partitions(all=False):
        if "cdrom" in p.opts or not p.fstype:
            continue
        try:
            u = psutil.disk_usage(p.mountpoint)
        except OSError:
            continue
        buf = ctypes.create_unicode_buffer(261)
        ctypes.windll.kernel32.GetVolumeInformationW(p.mountpoint, buf, 261, None, None, None, None, 0)
        out.append({"mount": p.mountpoint, "fstype": p.fstype, "label": buf.value, "total": u.total,
                    "used": u.used, "free": u.free, "percent": u.percent,
                    "removable": "removable" in p.opts})
    return out


CATEGORIES = {
    "Videos": {"mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "ts"},
    "Audio": {"mp3", "wav", "flac", "aac", "ogg", "m4a", "wma"},
    "Images": {"jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "psd", "heic", "webp", "raw", "cr2", "svg"},
    "Archives": {"zip", "rar", "7z", "tar", "gz", "bz2", "xz", "cab"},
    "Disk images": {"iso", "img", "vhd", "vhdx", "vmdk", "wim", "esd", "bin", "dmg"},
    "Documents": {"pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv", "log"},
    "Programs": {"exe", "msi", "dll", "sys", "apk"},
}
EXT_TO_CAT = {e: c for c, exts in CATEGORIES.items() for e in exts}
UNDELETABLE = {"pagefile.sys", "hiberfil.sys", "swapfile.sys"}


def file_category(path):
    return EXT_TO_CAT.get(os.path.splitext(path)[1].lower().lstrip("."), "Other")


class ScanState:
    def __init__(self, total_bytes):
        self.total_bytes = max(total_bytes, 1)
        self.files = self.bytes = self.errors = 0
        self.current = ""
        self.cancel = False
        self.heap = []      # (size, path, mtime) - the N largest files
        self.folders = {}   # first-level folder -> bytes


def scan_disk(roots, state, top_n=3000, min_size=1 * MB):
    for root in roots:
        root = os.path.normpath(root)
        stack = [root]
        while stack and not state.cancel:
            d = stack.pop()
            state.current = d
            try:
                it = os.scandir(d)
            except OSError:
                state.errors += 1
                continue
            with it:
                for e in it:
                    try:
                        if e.is_symlink() or (hasattr(e, "is_junction") and e.is_junction()):
                            continue
                        if e.is_dir(follow_symlinks=False):
                            stack.append(e.path)
                        elif e.is_file(follow_symlinks=False):
                            st = e.stat(follow_symlinks=False)
                            sz = st.st_size
                            state.files += 1
                            state.bytes += sz
                            rel = e.path[len(root):].lstrip("\\")
                            first = rel.split("\\", 1)
                            key = os.path.join(root, first[0]) if len(first) > 1 else root + "  (files in this folder)"
                            state.folders[key] = state.folders.get(key, 0) + sz
                            if sz >= min_size:
                                item = (sz, e.path, st.st_mtime)
                                if len(state.heap) < top_n:
                                    heapq.heappush(state.heap, item)
                                elif sz > state.heap[0][0]:
                                    heapq.heappushpop(state.heap, item)
                    except OSError:
                        state.errors += 1
    return state


def delete_to_recycle(path):
    """Move one file to the Recycle Bin. Refuses Windows files."""
    low = os.path.normpath(path).lower()
    if os.path.basename(low) in UNDELETABLE or low.startswith(core.WIN_DIR + "\\"):
        return False, "protected system file"
    if not os.path.isfile(path):
        return True, "already gone"
    ps = ("Add-Type -AssemblyName Microsoft.VisualBasic;"
          "[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile("
          f"'{path.replace(chr(39), chr(39) * 2)}','OnlyErrorDialogs','SendToRecycleBin')")
    ok, out = core.run(["powershell", "-NoProfile", "-Command", ps])
    return ok, "moved to Recycle Bin" if ok else core._last(out)
