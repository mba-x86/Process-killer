# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Process Killer - core logic (no UI): discovery of apps and everything
related to them, plus the actions that stop / remove those things."""
import csv
import ctypes
import io
import os
import re
import subprocess
import time
import winreg
from dataclasses import dataclass, field

import psutil

NO_WINDOW = subprocess.CREATE_NO_WINDOW
WIN_DIR = os.environ.get("SystemRoot", r"C:\Windows").lower()

PROTECTED_PROCS = {
    "system", "system idle process", "registry", "smss.exe", "csrss.exe",
    "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "svchost.exe",
    "dwm.exe", "explorer.exe", "fontdrvhost.exe", "memory compression",
}
PROTECTED_DIRS = {
    "microsoft", "windows", "windowsapps", "common files", "windows nt",
    "internet explorer", "windows defender", "modifiablewindowsapps",
    "packages", "temp", "microsoft shared", "dotnet", "reference assemblies",
}
UNINSTALL_ROOTS = [
    (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    (winreg.HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
]
RUN_KEYS = [
    (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "HKLM"),
    (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run", "HKLM (32-bit)"),
    (winreg.HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "HKCU"),
]


# ------------------------------------------------------------------ models

@dataclass
class AppInfo:
    name: str
    publisher: str = ""
    version: str = ""
    size_kb: int = 0
    date: str = ""
    location: str = ""
    uninstall: str = ""
    quiet: str = ""
    hive: int = 0
    regpath: str = ""
    no_remove: bool = False


@dataclass
class Related:
    kind: str            # process | service | startup | task | folder
    name: str
    detail: str
    data: dict = field(default_factory=dict)
    reason: str = "name"  # "path" = lives in the app's install folder, "name" = name looks similar
    protected: bool = False
    state: str = ""


# ------------------------------------------------------------------ helpers

def is_admin():
    try:
        return bool(ctypes.windll.shell32.IsUserAnAdmin())
    except Exception:
        return False


def run(cmd, timeout=120):
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout,
                           creationflags=NO_WINDOW, errors="replace")
        return p.returncode == 0, (p.stdout + p.stderr).strip()
    except Exception as e:  # noqa: BLE001
        return False, str(e)


def fmt_kb(kb):
    if not kb:
        return ""
    mb = kb / 1024
    return f"{mb / 1024:.1f} GB" if mb >= 1024 else f"{mb:.0f} MB" if mb >= 1 else f"{kb} KB"


def fmt_bytes(b):
    return fmt_kb(b // 1024) if b else ""


def roots():
    e = os.environ.get
    out, seen = [], set()
    for c in (e("ProgramFiles"), e("ProgramFiles(x86)"), e("ProgramData"), e("LOCALAPPDATA"),
              e("APPDATA"), os.path.join(e("LOCALAPPDATA", ""), "Programs")):
        if c and os.path.isdir(c) and c.lower() not in seen:
            seen.add(c.lower())
            out.append(c)
    return out


def safe_location(loc):
    """Normalised lower-case install folder, or None if it's too broad to trust."""
    if not loc:
        return None
    p = os.path.normpath(loc.strip().strip('"')).lower().rstrip("\\")
    if not os.path.isabs(p) or p.startswith(WIN_DIR):
        return None
    if p in {r.lower() for r in roots()} or len(p.split("\\")) < 3:
        return None
    if p == os.environ.get("USERPROFILE", "").lower():
        return None
    return p


def core_name(name):
    """'Python 3.14.7 (64-bit)' -> 'Python'. '' if too short to match safely."""
    n = re.sub(r"\([^)]*\)", " ", name)
    n = re.sub(r"\bv?\d+(\.\d+)+\S*", " ", n)
    n = re.sub(r"\b(x64|x86|64-bit|32-bit|version)\b", " ", n, flags=re.I)
    n = re.sub(r"\s+", " ", n).strip(" -_")
    return n if len(n) >= 4 else ""


def _exe_of(cmd):
    """Executable named at the start of an uninstall command. Handles quoted paths and the common
    unquoted 'C:\\Program Files (x86)\\App\\unins000.exe /SILENT' form (path contains spaces)."""
    cmd = os.path.expandvars(cmd or "")
    m = (re.match(r'\s*"([^"]+)"', cmd) or re.match(r"\s*([A-Za-z]:\\.+?\.exe)(?=\s|$)", cmd, re.I)
         or re.match(r"\s*(\S+?\.exe)", cmd, re.I))
    return m.group(1) if m else ""


def _infer_location(v):
    loc = (v.get("InstallLocation") or "").strip().strip('"')
    if loc:
        return loc
    icon = (v.get("DisplayIcon") or "").split(",")[0].strip().strip('"')
    if icon and os.path.splitext(icon)[1].lower() in (".exe", ".ico"):
        d = os.path.dirname(icon)
        if safe_location(d):
            return d
    exe = _exe_of(v.get("UninstallString") or "")
    if exe and not re.search(r"msiexec|rundll32", exe, re.I):
        d = os.path.dirname(exe)
        if safe_location(d):
            return d
    return ""


# ------------------------------------------------------------------ applications

def list_apps():
    apps, seen = [], set()
    for hive, root in UNINSTALL_ROOTS:
        try:
            k = winreg.OpenKey(hive, root)
        except OSError:
            continue
        with k:
            for i in range(winreg.QueryInfoKey(k)[0]):
                try:
                    sub = winreg.EnumKey(k, i)
                    v = {}
                    with winreg.OpenKey(k, sub) as sk:
                        for n in ("DisplayName", "DisplayVersion", "Publisher", "InstallLocation",
                                  "InstallDate", "EstimatedSize", "UninstallString",
                                  "QuietUninstallString", "DisplayIcon", "SystemComponent",
                                  "ParentKeyName", "ReleaseType", "NoRemove"):
                            try:
                                v[n] = winreg.QueryValueEx(sk, n)[0]
                            except OSError:
                                pass
                except OSError:
                    continue
                # some publishers (Microsoft Edge) prefix the name with invisible direction marks
                name = "".join(ch for ch in (v.get("DisplayName") or "") if ch.isprintable()).strip()
                cmd = v.get("UninstallString") or v.get("QuietUninstallString")
                # Windows marks Microsoft Edge "NoRemove"; we still list it (guarded flow) because people ask
                # to remove it. WebView2 is SystemComponent and stays hidden: other apps depend on it.
                edge = name.lower() == "microsoft edge" and "--msedge" in (cmd or "").lower()
                if (not name or not cmd or v.get("SystemComponent") == 1 or (v.get("NoRemove") == 1 and not edge)
                        or "ParentKeyName" in v or v.get("ReleaseType") in ("Update", "Hotfix", "Security Update")
                        or re.match(r"^(KB\d+|Update for|Security Update)", name, re.I)):
                    continue
                ver = str(v.get("DisplayVersion") or "")
                key = (name.lower(), ver)
                if key in seen:
                    continue
                seen.add(key)
                d = str(v.get("InstallDate") or "")
                if re.fullmatch(r"\d{8}", d):
                    d = f"{d[:4]}-{d[4:6]}-{d[6:]}"
                size = v.get("EstimatedSize")
                apps.append(AppInfo(
                    name=name, publisher=str(v.get("Publisher") or ""), version=ver,
                    size_kb=size if isinstance(size, int) else 0, date=d,
                    location=_infer_location(v), uninstall=v.get("UninstallString") or cmd,
                    quiet=v.get("QuietUninstallString") or "", hive=hive, regpath=f"{root}\\{sub}",
                    no_remove=v.get("NoRemove") == 1))
    apps.sort(key=lambda a: a.name.lower())
    return apps


def app_installed(app):
    try:
        winreg.CloseKey(winreg.OpenKey(app.hive, app.regpath))
        return True
    except OSError:
        return False


# ------------------------------------------------------------------ Microsoft Edge (guarded removal)

EDGE_STABLE_GUID = "{56EB18F8-B008-4CBD-B6D2-8C97FE7E9062}"
EDGE_POLICY_KEY = r"SOFTWARE\Policies\Microsoft\EdgeUpdate"
BROWSER_WORDS = ("chrome", "brave", "firefox", "opera", "vivaldi")


def is_edge(app):
    return app.name.strip().lower() == "microsoft edge" and "--msedge" in app.uninstall.lower()


def other_browsers(apps=None):
    """Names of other installed browsers, so removing Edge never leaves the PC without one."""
    names = []
    for a in apps if apps is not None else list_apps():
        n = a.name.lower()
        if any(b in n for b in BROWSER_WORDS) and "webview" not in n and a.name not in names:
            names.append(a.name)
    return names


def block_edge_reinstall(hive=winreg.HKEY_LOCAL_MACHINE, path=EDGE_POLICY_KEY):
    """Tell Edge's updater (policy 'Allow installation' = 0 for Edge Stable) not to install Edge again.
    Undo: delete that value, or set it to 1. WebView2 is not affected. -> (ok, message)"""
    try:
        with winreg.CreateKeyEx(hive, path, 0, winreg.KEY_SET_VALUE | winreg.KEY_WOW64_64KEY) as k:
            winreg.SetValueEx(k, f"Install{EDGE_STABLE_GUID}", 0, winreg.REG_DWORD, 0)
        return True, f"reinstall blocked (HKLM\\{path} \\ Install{EDGE_STABLE_GUID} = 0)"
    except PermissionError:
        return False, "access denied - run Process Killer as Administrator"
    except OSError as e:
        return False, str(e)


def uninstaller_missing(app):
    """True when the app's uninstall command points at a file that no longer exists (a stale entry:
    the program was deleted or moved by hand). Windows' own Uninstall button fails for these."""
    exe = _exe_of(app.uninstall)
    if not exe or re.search(r"msiexec|rundll32|cmd\.exe|powershell|explorer\.exe|ms-settings", exe, re.I):
        return False
    return os.path.isabs(exe) and not os.path.exists(exe)


def _delete_reg_tree(hive, path):
    flags = winreg.KEY_ALL_ACCESS | winreg.KEY_WOW64_64KEY
    with winreg.OpenKey(hive, path, 0, flags) as k:
        while True:
            try:
                sub = winreg.EnumKey(k, 0)
            except OSError:
                break
            _delete_reg_tree(hive, path + "\\" + sub)
    winreg.DeleteKeyEx(hive, path, flags)


def remove_uninstall_entry(app):
    """Delete a stale 'Installed apps' entry. A .reg backup is saved first so it can be restored by
    double-clicking it. -> (ok, message)"""
    hive_name = {winreg.HKEY_LOCAL_MACHINE: "HKLM", winreg.HKEY_CURRENT_USER: "HKCU"}.get(app.hive, "")
    if not hive_name:
        return False, "unsupported registry location"
    folder = os.path.join(os.environ.get("APPDATA", "."), "ProcessKiller", "backups")
    backup = ""
    try:
        os.makedirs(folder, exist_ok=True)
        safe = re.sub(r"[^\w.-]+", "_", app.name)[:60]
        backup = os.path.join(folder, f"{safe}-{time.strftime('%Y%m%d-%H%M%S')}.reg")
        ok, out = run(["reg", "export", f"{hive_name}\\{app.regpath}", backup, "/y"])
        if not ok:
            return False, f"could not save a registry backup first ({_last(out)}) - nothing was changed"
    except OSError as e:
        return False, f"could not save a registry backup first ({e}) - nothing was changed"
    try:
        _delete_reg_tree(app.hive, app.regpath)
    except PermissionError:
        return False, "access denied - run Process Killer as Administrator"
    except OSError as e:
        return False, str(e)
    return True, f"entry removed (backup: {backup})"


# ------------------------------------------------------------------ snapshots

def snap_processes():
    me, parent = os.getpid(), os.getppid()
    out = []
    for p in psutil.process_iter(["pid", "name", "exe"]):
        i = p.info
        if i["pid"] not in (me, parent, 0):
            out.append({"pid": i["pid"], "name": i["name"] or "?", "exe": i["exe"] or ""})
    return out


def snap_services():
    """All services. Uses psutil's two low-level queries per service instead of as_dict(), which makes
    ~8 Service Control Manager calls per service (slow on PCs with many vendor services)."""
    out = []
    for s in psutil.win_service_iter():
        try:
            try:
                cfg, st = s._query_config(), s._query_status()
                row = {"name": s.name(), "display": cfg["display_name"], "status": st["status"],
                       "start": cfg["start_type"], "path": cfg["binpath"] or "", "pid": st.get("pid")}
            except AttributeError:  # psutil internals changed: fall back to the public API
                row = {"name": s.name(), "display": s.display_name(), "status": s.status(),
                       "start": s.start_type(), "path": s.binpath() or "", "pid": s.pid()}
            out.append(row)
        except Exception:  # noqa: BLE001
            continue
    return out


_svc_cache = (0.0, [])


def cached_services(max_age=20):
    global _svc_cache
    if time.time() - _svc_cache[0] > max_age:
        _svc_cache = (time.time(), snap_services())
    return _svc_cache[1]


def invalidate_caches():
    """Call after anything that changes services or tasks."""
    global _svc_cache, _task_cache
    _svc_cache, _task_cache = (0.0, []), (0.0, [])


def list_startup():
    out = []
    for hive, path, label in RUN_KEYS:
        try:
            with winreg.OpenKey(hive, path) as k:
                for i in range(winreg.QueryInfoKey(k)[1]):
                    name, val, _ = winreg.EnumValue(k, i)
                    out.append({"name": name, "cmd": str(val), "hive": hive, "path": path,
                                "label": label})
        except OSError:
            continue
    return out


_task_cache = (0.0, [])


TASKS_DIR = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32", "Tasks")


def _tasks_from_files():
    """Read the task definitions straight from System32\\Tasks (a few ms) instead of `schtasks /query /v`,
    which takes many seconds on PCs with lots of vendor tasks. Returns None if the folder is unreadable."""
    import xml.etree.ElementTree as ET
    try:
        entries = list(os.scandir(TASKS_DIR))
    except OSError:
        return None
    rows = []
    stack = [e for e in entries if not (e.is_dir() and e.name.lower() == "microsoft")]
    while stack:
        e = stack.pop()
        try:
            if e.is_dir(follow_symlinks=False):
                stack.extend(os.scandir(e.path))
                continue
            root = ET.parse(e.path).getroot()
        except (OSError, ET.ParseError):
            continue
        ns = {"t": root.tag.split("}")[0].strip("{")} if root.tag.startswith("{") else {}
        q = (lambda p: p.replace("t:", "")) if not ns else (lambda p: p)
        cmds = []
        for ex in root.iterfind(q(".//t:Actions/t:Exec"), ns):
            c = (ex.findtext(q("t:Command"), "", ns) or "").strip()
            a = (ex.findtext(q("t:Arguments"), "", ns) or "").strip()
            cmds.append(f"{c} {a}".strip())
        enabled = (root.findtext(q(".//t:Settings/t:Enabled"), "true", ns) or "true").lower() != "false"
        name = "\\" + os.path.relpath(e.path, TASKS_DIR).replace("/", "\\")
        rows.append({"name": name, "cmd": " | ".join(cmds), "status": "Ready" if enabled else "Disabled"})
    return rows


def _tasks_from_schtasks():
    ok, text = run(["schtasks", "/query", "/fo", "csv", "/v"], 90)
    rows, seen = [], set()
    if ok:
        for r in csv.reader(io.StringIO(text)):
            if len(r) < 9 or not r[1].startswith("\\") or r[1] in seen:
                continue
            if r[1].lower().startswith("\\microsoft\\"):
                continue
            seen.add(r[1])
            rows.append({"name": r[1], "cmd": r[8], "status": r[3]})
    return rows


def list_tasks(force=False):
    global _task_cache
    if not force and time.time() - _task_cache[0] < 120:
        return _task_cache[1]
    rows = _tasks_from_files()
    if rows is None:                       # not allowed to read the folder (not elevated)
        rows = _tasks_from_schtasks()
    _task_cache = (time.time(), rows)
    return rows


def snapshot(fresh=False):
    """Processes are always live. Services (20 s) and tasks (2 min) are cached unless fresh=True."""
    return {"procs": snap_processes(), "services": snap_services() if fresh else cached_services(),
            "startup": list_startup(), "tasks": list_tasks(force=fresh)}


# ------------------------------------------------------------------ relation finder

def _is_protected_proc(name, exe):
    return name.lower() in PROTECTED_PROCS or exe.lower().startswith(WIN_DIR + "\\")


def find_related(app, snap=None):
    snap = snap or snapshot()
    loc = safe_location(app.location)
    core = core_name(app.name).lower()

    def by_path(text):
        return bool(loc) and loc in text.lower().replace('"', "")

    def by_name(*texts):
        return bool(core) and any(core in (t or "").lower() for t in texts)

    out = []
    for p in snap["procs"]:
        exe = p["exe"].lower()
        if loc and exe.startswith(loc + "\\"):
            reason = "path"
        elif by_name(p["name"], p["exe"]):
            reason = "name"
        else:
            continue
        out.append(Related("process", f"{p['name']} (PID {p['pid']})", p["exe"], {"pid": p["pid"]},
                           reason, _is_protected_proc(p["name"], p["exe"]), "Running"))
    for s in snap["services"]:
        if by_path(s["path"]):
            reason = "path"
        elif by_name(s["name"], s["display"]):
            reason = "name"
        else:
            continue
        out.append(Related("service", s["display"] or s["name"], s["path"], {"name": s["name"]},
                           reason, False, s["status"].capitalize()))
    for s in snap["startup"]:
        if by_path(s["cmd"]):
            reason = "path"
        elif by_name(s["name"], s["cmd"]):
            reason = "name"
        else:
            continue
        out.append(Related("startup", s["name"], f"{s['label']}  {s['cmd']}", s, reason, False, "Enabled"))
    for t in snap["tasks"]:
        if by_path(t["cmd"]):
            reason = "path"
        elif by_name(t["name"], t["cmd"]):
            reason = "name"
        else:
            continue
        out.append(Related("task", t["name"], t["cmd"], {"name": t["name"]}, reason, False, t["status"]))
    order = {"process": 0, "service": 1, "startup": 2, "task": 3, "folder": 4}
    out.sort(key=lambda r: (r.reason != "path", order[r.kind], r.name.lower()))
    return out


def find_leftovers(app):
    """After an uninstall: whatever related things are still on the machine."""
    items = [r for r in find_related(app, snapshot(fresh=True)) if r.kind != "process" or not r.protected]
    core = core_name(app.name).lower()
    loc = safe_location(app.location)
    seen = set()
    if loc and os.path.isdir(loc):
        seen.add(loc)
        items.append(Related("folder", app.location, "Install folder", {"path": app.location}, "path"))
    if is_edge(app):
        # Edge's own folder, and (unticked by default) the user's profile data. EdgeUpdate / EdgeWebView are
        # deliberately NOT offered: WebView2 needs them.
        pf = os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")
        edge_dir = os.path.join(pf, "Microsoft", "Edge").lower()      # the parent folder covers Application\
        items[:] = [r for r in items if not (r.kind == "folder" and r.name.lower().startswith(edge_dir))]
        seen.discard(loc)
        for path, label, reason in (
                (os.path.join(pf, "Microsoft", "Edge"), "Edge program files", "path"),
                (os.path.expandvars(r"%LOCALAPPDATA%\Microsoft\Edge"),
                 "Edge profile data: bookmarks, saved passwords, history", "name")):
            if os.path.isdir(path) and path.lower() not in seen:
                seen.add(path.lower())
                items.append(Related("folder", path, label, {"path": path}, reason))
    for root in roots():
        try:
            for e in os.scandir(root):
                if not e.is_dir(follow_symlinks=False) or e.path.lower() in seen:
                    continue
                if core and core in e.name.lower():
                    prot = e.name.lower() in PROTECTED_DIRS
                    seen.add(e.path.lower())
                    items.append(Related("folder", e.path, f"Leftover in {os.path.basename(root) or root}",
                                         {"path": e.path}, "path" if e.name.lower() == core else "name",
                                         prot))
        except OSError:
            continue
    return items


# ------------------------------------------------------------------ actions -> (ok, message)

def is_protected_proc_pid(pid):
    try:
        p = psutil.Process(pid)
        return _is_protected_proc(p.name(), p.exe() if p.exe() else "") or pid == os.getpid()
    except psutil.Error:
        return False


def act_kill(data, tree=True):
    try:
        p = psutil.Process(data["pid"])
        if tree:
            for c in p.children(recursive=True):
                try:
                    c.kill()
                except psutil.Error:
                    pass
        p.kill()
        p.wait(5)
        return True, "process ended"
    except psutil.NoSuchProcess:
        return True, "already gone"
    except psutil.AccessDenied:
        return False, "access denied (run as Administrator)"
    except Exception as e:  # noqa: BLE001
        return False, str(e)


def svc_status(name):
    try:
        return psutil.win_service_get(name).status()
    except Exception:  # noqa: BLE001
        return None


def svc_stop(data, timeout=20):
    invalidate_caches()
    name = data["name"]
    if svc_status(name) in (None, "stopped"):
        return True, "already stopped"
    ok, out = run(["sc", "stop", name])
    end = time.time() + timeout
    while time.time() < end:
        if svc_status(name) == "stopped":
            return True, "service stopped"
        time.sleep(0.4)
    return False, out.splitlines()[-1] if out else "timed out waiting for it to stop"


def svc_start(data):
    invalidate_caches()
    ok, out = run(["sc", "start", data["name"]])
    return (ok or "1056" in out), "service started" if (ok or "1056" in out) else _last(out)


def svc_config(data, mode):
    invalidate_caches()
    ok, out = run(["sc", "config", data["name"], "start=", mode])
    return ok, {"auto": "startup: automatic", "demand": "startup: manual",
                "disabled": "startup: disabled"}[mode] if ok else _last(out)


def svc_disable(data):
    ok, msg = svc_stop(data)
    ok2, msg2 = svc_config(data, "disabled")
    return ok2, ("stopped and disabled" if ok and ok2 else msg2 if not ok2 else f"disabled ({msg})")


def svc_enable(data):
    ok, msg = svc_config(data, "auto")
    if not ok:
        return ok, msg
    ok2, msg2 = svc_start(data)
    return ok2, "enabled and started" if ok2 else f"enabled, but could not start: {msg2}"


def svc_restart(data):
    ok, msg = svc_stop(data)
    return svc_start(data) if ok else (ok, msg)


def svc_delete(data):
    invalidate_caches()
    svc_stop(data, timeout=8)
    ok, out = run(["sc", "delete", data["name"]])
    return ok, "service deleted" if ok else _last(out)


def act_startup_remove(data):
    try:
        with winreg.OpenKey(data["hive"], data["path"], 0, winreg.KEY_SET_VALUE) as k:
            winreg.DeleteValue(k, data["name"])
        return True, "startup entry removed"
    except FileNotFoundError:
        return True, "already gone"
    except OSError as e:
        return False, str(e)


def act_task_delete(data):
    invalidate_caches()
    ok, out = run(["schtasks", "/delete", "/tn", data["name"], "/f"])
    return ok, "task deleted" if ok else _last(out)


def act_folder_recycle(data):
    path = data["path"]
    if not os.path.isdir(path):
        return True, "already gone"
    if os.path.basename(path.rstrip("\\")).lower() in PROTECTED_DIRS:
        return False, "protected folder"
    ps = ("Add-Type -AssemblyName Microsoft.VisualBasic;"
          "[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory("
          f"'{path.replace(chr(39), chr(39) * 2)}','OnlyErrorDialogs','SendToRecycleBin')")
    ok, out = run(["powershell", "-NoProfile", "-Command", ps])
    return ok, "moved to Recycle Bin" if ok else _last(out)


def run_uninstall(app, silent=False):
    if uninstaller_missing(app):
        return False, f"the uninstaller is missing: {_exe_of(app.uninstall)}"
    cmd = app.quiet if (silent and app.quiet) else app.uninstall
    if is_edge(app):
        if not is_admin():
            return False, "removing Microsoft Edge needs Administrator rights"
        if "--force-uninstall" not in cmd.lower():   # Windows blocks the plain command for Edge
            cmd += " --force-uninstall"
    if re.search(r"msiexec", cmd, re.I):
        cmd = re.sub(r"/i(?=\s*\{)", "/X", cmd, flags=re.I)
        if silent and "/q" not in cmd.lower():
            cmd += " /qn /norestart"
    try:
        rc = subprocess.run(cmd, shell=True, timeout=3600).returncode
    except Exception as e:  # noqa: BLE001
        return False, str(e)
    time.sleep(2)
    if rc in (1602, 1223):
        return False, "uninstall was cancelled"
    if not app_installed(app):
        return True, "application removed" + (" (restart may be required)" if rc in (1641, 3010) else "")
    if rc in (0, 3010, 1641):
        return True, "uninstaller finished (app may still be closing)"
    return False, f"uninstaller exited with code {rc}"


def _last(out):
    lines = [l for l in (out or "").splitlines() if l.strip()]
    return lines[-1].strip() if lines else "failed"


# actions used by the wizard
PRE_ACTIONS = {"process": act_kill, "service": svc_stop, "startup": act_startup_remove,
               "task": act_task_delete}
PRE_LABELS = {"process": "End process", "service": "Stop service", "startup": "Remove entry",
              "task": "Delete task"}
CLEAN_ACTIONS = {"process": act_kill, "service": svc_delete, "startup": act_startup_remove,
                 "task": act_task_delete, "folder": act_folder_recycle}
CLEAN_LABELS = {"process": "End process", "service": "Delete service", "startup": "Remove entry",
                "task": "Delete task", "folder": "Recycle Bin"}
