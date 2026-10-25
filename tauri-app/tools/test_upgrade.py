# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Prove that installed copies upgrade IN PLACE to the current installer.

Two scenarios, both per-user (an all-users install needs a UAC prompt, so that layout is checked by hand):
  A. 1.x (Python/Tkinter, installer.iss in the repo root)  ->  current   (the original 1.x -> 2.x migration)
  B. 2.0.0 "Process Killer" (installer script of commit 3555d87) -> current "Task Reaper"  (the rename)
Each runs the new installer EXACTLY like the in-app updater does:
        /SILENT /SUPPRESSMSGBOXES /CLOSEAPPLICATIONS /RESTARTAPPLICATIONS /NORESTART /RELAUNCH=1

NOTHING REAL IS TOUCHED: a throw-away AppId, a temp install folder, and every product / shortcut / data-folder name in the test
copies of the scripts is prefixed "PKTest" (so the real Start menu, desktop shortcuts and %APPDATA%\\ProcessKiller are never deleted).
Run:  py tauri-app/tools/test_upgrade.py
"""
import os, re, shutil, subprocess, sys, tempfile, time, winreg

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))     # tauri-app
REPO = os.path.dirname(ROOT)
TEST_GUID = "{{5A1D7C0E-0B7B-4C1B-9F11-AAAA00000001}"                   # Inno syntax ({{ = literal {)
KEY = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{5A1D7C0E-0B7B-4C1B-9F11-AAAA00000001}_is1"
OLD_20_COMMIT = "3555d87"                                               # last commit with the 2.0.0 installer script
results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("PASS " if ok else "FAIL ") + name + (f"  ({detail})" if detail else ""))


def iscc():
    for p in (os.path.expandvars(r"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe"), r"C:\Program Files (x86)\Inno Setup 6\ISCC.exe"):
        if os.path.exists(p):
            return p
    sys.exit("Inno Setup not found")


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def ps(script):
    return run(["powershell", "-NoProfile", "-Command", script]).stdout.strip()


def build_stub(work):
    d = os.path.join(work, "stub")
    os.makedirs(os.path.join(d, "src"))
    open(os.path.join(d, "Cargo.toml"), "w").write('[package]\nname="ProcessKiller"\nversion="0.0.1"\nedition="2021"\n[profile.release]\nopt-level="s"\n')
    open(os.path.join(d, "src", "main.rs"), "w").write(
        '#![windows_subsystem = "windows"]\nfn main(){ if let Ok(p)=std::env::var("PK_TEST_MARKER"){ let _=std::fs::write(p, std::env::current_exe().unwrap().to_string_lossy().as_bytes()); } }\n')
    r = run(["cargo", "build", "--release", "-q"], cwd=d)
    exe = os.path.join(d, "target", "release", "ProcessKiller.exe")
    if not os.path.exists(exe):
        sys.exit("stub build failed:\n" + r.stderr[-800:])
    return exe


def sandbox(text):
    """Make a test copy of an installer script that can only touch test-named things."""
    text = text.replace("{{8F2C6E1B-5B1E-4E0C-9C55-3D7A6B1E9A42}", TEST_GUID)          # throw-away product id
    text = text.replace("Process Killer", "PKTest Process Killer")                       # old product name (shortcuts, folders, AppName)
    text = text.replace('#define AppName "Task Reaper"', '#define AppName "PKTest Task Reaper"')
    text = text.replace("{userappdata}\\ProcessKiller", "{userappdata}\\PKTestData")    # never delete the real settings folder
    text = re.sub(r"(?m)^(WizardSmallImageFile|WizardImageFile|SetupIconFile)=assets\\", lambda m: f"{m.group(1)}={REPO}\\assets\\", text)
    text = text.replace("LicenseFile=LICENSE", f"LicenseFile={REPO}\\LICENSE")
    text = text.replace("SetupIconFile=..\\src-tauri\\icons\\icon.ico", f"SetupIconFile={ROOT}\\src-tauri\\icons\\icon.ico")
    return text


def compile_iss(src_text, defines, out_iss_path):
    open(out_iss_path, "w", encoding="utf8").write(sandbox(src_text))
    r = run([iscc(), *[f"/D{k}={v}" for k, v in defines.items()], out_iss_path], cwd=os.path.dirname(out_iss_path))
    if r.returncode != 0:
        sys.exit("ISCC failed:\n" + r.stdout[-1500:] + r.stderr[-500:])


def reg(name):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY) as k:
            return winreg.QueryValueEx(k, name)[0]
    except OSError:
        return None


def folders():
    out = ps("[Environment]::GetFolderPath('Programs'); [Environment]::GetFolderPath('Desktop')").splitlines()
    return out[0], out[1]


def lnk_target(path):
    return ps(f"(New-Object -ComObject WScript.Shell).CreateShortcut('{path}').TargetPath")


def uninstall(app):
    unins = os.path.join(app, "unins000.exe")
    if os.path.exists(unins):
        run([unins, "/VERYSILENT", "/SUPPRESSMSGBOXES"])
        time.sleep(2)


def upgrade(setup, marker):
    env = dict(os.environ, PK_TEST_MARKER=marker)
    return subprocess.run([setup, "/SILENT", "/SUPPRESSMSGBOXES", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS", "/NORESTART", "/RELAUNCH=1"],
                          env=env, capture_output=True, text=True, timeout=180)


def wait_marker(marker):
    for _ in range(40):
        if os.path.exists(marker):
            return True
        time.sleep(0.25)
    return False


def new_payload(work, stub):
    src = os.path.join(work, "new_src"); os.makedirs(src)
    shutil.copy(stub, os.path.join(src, "ProcessKiller.exe"))
    with open(os.path.join(src, "ProcessKiller.exe"), "ab") as f:
        f.write(b"\0" * 2048)                                                  # different size/hash than the old exe
    return src


def version_now():
    return re.search(r'^version\s*=\s*"([^"]+)"', open(os.path.join(ROOT, "src-tauri", "Cargo.toml"), encoding="utf8").read(), re.M).group(1)


def scenario_a(work, stub, new_iss, new_ver, new_setup_dir):
    print("\n== A: 1.x (Python) -> current ==")
    app = os.path.join(work, "app_a"); marker = os.path.join(work, "relaunched_a.txt")
    old_src = os.path.join(work, "old_src"); os.makedirs(os.path.join(old_src, "_internal", "lib"))
    shutil.copy(stub, os.path.join(old_src, "ProcessKiller.exe"))
    for n in ("python314.dll", "tcl86t.dll", os.path.join("lib", "_ssl.pyd")):
        open(os.path.join(old_src, "_internal", n), "wb").write(os.urandom(4096))
    out_old = os.path.join(work, "out_old"); os.makedirs(out_old)
    old_iss = os.path.join(work, "old.iss")
    compile_iss(open(os.path.join(REPO, "installer.iss"), encoding="utf8").read(), {"AppVersion": "1.0.3", "SourceDir": old_src, "OutDir": out_old}, old_iss)
    run([os.path.join(out_old, "ProcessKiller-Setup-1.0.3.exe"), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER", "/TASKS=", f"/DIR={app}"])
    check("A: 1.x install registered", reg("DisplayVersion") == "1.0.3", f"version={reg('DisplayVersion')}")
    check("A: 1.x has Python runtime folder", os.path.isdir(os.path.join(app, "_internal")))
    old_size = os.path.getsize(os.path.join(app, "ProcessKiller.exe"))
    r = upgrade(os.path.join(new_setup_dir, f"TaskReaper-Setup-{new_ver}.exe"), marker)
    check("A: installer exit code 0", r.returncode == 0, f"code={r.returncode}")
    check(f"A: version updated to {new_ver}", reg("DisplayVersion") == new_ver, f"version={reg('DisplayVersion')}")
    loc = reg("InstallLocation") or ""
    check("A: same install folder reused", os.path.normcase(os.path.normpath(loc)) == os.path.normcase(os.path.normpath(app)), loc)
    check("A: old Python runtime removed (_internal)", not os.path.exists(os.path.join(app, "_internal")))
    check("A: exe replaced by the new build", os.path.getsize(os.path.join(app, "ProcessKiller.exe")) == old_size + 2048)
    check("A: app relaunched automatically (/RELAUNCH=1)", wait_marker(marker) and os.path.normcase(open(marker).read()) == os.path.normcase(os.path.join(app, "ProcessKiller.exe")))
    uninstall(app)
    check("A: uninstall removes registry entry", reg("DisplayVersion") is None)
    check("A: uninstall removes install folder", not os.path.exists(os.path.join(app, "ProcessKiller.exe")))


def scenario_b(work, stub, new_ver, new_setup_dir):
    print("\n== B: 2.0.0 'Process Killer' -> current 'Task Reaper' (rename) ==")
    app = os.path.join(work, "app_b"); marker = os.path.join(work, "relaunched_b.txt")
    programs, desktop = folders()
    old_grp, old_desk = os.path.join(programs, "PKTest Process Killer"), os.path.join(desktop, "PKTest Process Killer.lnk")
    new_grp, new_desk = os.path.join(programs, "PKTest Task Reaper"), os.path.join(desktop, "PKTest Task Reaper.lnk")
    data = os.path.join(os.environ["APPDATA"], "PKTestData")
    for p in (old_grp, new_grp):
        shutil.rmtree(p, ignore_errors=True)
    for p in (old_desk, new_desk):
        try: os.remove(p)
        except OSError: pass
    old_src = os.path.join(work, "old20_src"); os.makedirs(old_src)
    shutil.copy(stub, os.path.join(old_src, "ProcessKiller.exe"))
    old_text = run(["git", "show", f"{OLD_20_COMMIT}:tauri-app/installer/ProcessKiller.iss"], cwd=REPO).stdout
    if "Process Killer" not in old_text:
        sys.exit("could not read the 2.0.0 installer script from git")
    out_old = os.path.join(work, "out_old20"); os.makedirs(out_old)
    old_iss = os.path.join(ROOT, "installer", "_upgrade_old20.iss")
    try:
        compile_iss(old_text, {"AppVersion": "2.0.0", "SourceDir": old_src, "OutDir": out_old}, old_iss)
    finally:
        try: os.remove(old_iss)
        except OSError: pass
    run([os.path.join(out_old, "ProcessKiller-Setup-2.0.0.exe"), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER", "/TASKS=desktopicon", f"/DIR={app}"])
    check("B: 2.0.0 registered as the old product name", reg("DisplayName") == "PKTest Process Killer" and reg("DisplayVersion") == "2.0.0", f"{reg('DisplayName')} {reg('DisplayVersion')}")
    check("B: old Start menu + desktop shortcuts exist before the update", os.path.isfile(os.path.join(old_grp, "PKTest Process Killer.lnk")) and os.path.isfile(old_desk))
    os.makedirs(data, exist_ok=True)
    open(os.path.join(data, "settings.json"), "w").write('{"auto_check": false}')
    r = upgrade(os.path.join(new_setup_dir, f"TaskReaper-Setup-{new_ver}.exe"), marker)
    check("B: installer exit code 0", r.returncode == 0, f"code={r.returncode}")
    check(f"B: version updated to {new_ver}", reg("DisplayVersion") == new_ver, f"version={reg('DisplayVersion')}")
    check("B: product renamed in Apps & features", reg("DisplayName") == "PKTest Task Reaper", str(reg("DisplayName")))
    check("B: old Start menu folder + shortcuts removed", not os.path.exists(old_grp), old_grp)
    check("B: old desktop shortcut removed", not os.path.exists(old_desk))
    check("B: new Start menu shortcut created", os.path.isfile(os.path.join(new_grp, "PKTest Task Reaper.lnk")))
    check("B: desktop shortcut re-created under the new name (task remembered)", os.path.isfile(new_desk))
    lnk = os.path.join(new_grp, "PKTest Task Reaper.lnk")
    check("B: shortcuts launch ProcessKiller.exe in the existing folder", os.path.isfile(lnk) and os.path.normcase(lnk_target(lnk)) == os.path.normcase(os.path.join(app, "ProcessKiller.exe")), lnk_target(lnk) if os.path.isfile(lnk) else "")
    check("B: exe replaced by the new build", os.path.getsize(os.path.join(app, "ProcessKiller.exe")) == os.path.getsize(stub) + 2048)
    check("B: app relaunched automatically", wait_marker(marker))
    check("B: settings folder survived the update", os.path.isfile(os.path.join(data, "settings.json")))
    uninstall(app)
    check("B: uninstall removes registry entry", reg("DisplayVersion") is None)
    check("B: uninstall removes the new shortcuts", not os.path.exists(new_grp) and not os.path.exists(new_desk))
    shutil.rmtree(data, ignore_errors=True)


def main():
    work = tempfile.mkdtemp(prefix="pk_upgrade_")
    new_iss = os.path.join(ROOT, "installer", "_upgrade_test.iss")        # must sit next to the real script (relative paths)
    new_ver = version_now()
    try:
        stub = build_stub(work)
        for n in ("wizard_small.bmp", "wizard_large.bmp"):                   # normally produced by release.py
            if not os.path.exists(os.path.join(ROOT, "installer", n)):
                shutil.copy(os.path.join(REPO, "assets", n), os.path.join(ROOT, "installer", n))
        out_new = os.path.join(work, "out_new"); os.makedirs(out_new)
        compile_iss(open(os.path.join(ROOT, "installer", "ProcessKiller.iss"), encoding="utf8").read(),
                    {"AppVersion": new_ver, "SourceDir": new_payload(work, stub), "OutDir": out_new}, new_iss)
        scenario_a(work, stub, new_iss, new_ver, out_new)
        scenario_b(work, stub, new_ver, out_new)
    finally:
        try:
            os.remove(new_iss)
        except OSError:
            pass
        if reg("DisplayVersion") is not None:       # never leave the throw-away entry behind
            try:
                winreg.DeleteKey(winreg.HKEY_CURRENT_USER, KEY)
            except OSError:
                pass
        shutil.rmtree(work, ignore_errors=True)
    bad = [n for n, ok, _ in results if not ok]
    print(f"\n{len(results) - len(bad)}/{len(results)} checks passed" + (f"; FAILED: {bad}" if bad else ""))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
