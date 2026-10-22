# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Prove that an installed 1.x (Python) copy upgrades IN PLACE to the 2.x installer.

Uses a THROW-AWAY AppId and a temp install folder, so no real Process Killer install is touched.
  1. build a tiny stub ProcessKiller.exe (writes a marker file when launched)
  2. compile + silently install a fake "1.0.3" (stub + a fake Python `_internal` folder) with the real 1.x installer.iss
  3. compile the real 2.x installer.iss (stub exe) and run it EXACTLY like the 1.x updater does:
        /SILENT /SUPPRESSMSGBOXES /CLOSEAPPLICATIONS /RESTARTAPPLICATIONS /NORESTART /RELAUNCH=1
  4. check: same folder reused, `_internal` removed, exe replaced, version registered as 2.0.0, app relaunched,
     then uninstall leaves nothing behind.
Run:  py tauri-app/tools/test_upgrade.py
"""
import os, re, shutil, subprocess, sys, tempfile, time, winreg

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))     # tauri-app
REPO = os.path.dirname(ROOT)
TEST_GUID = "{{5A1D7C0E-0B7B-4C1B-9F11-AAAA00000001}"                   # Inno syntax ({{ = literal {)
KEY = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{5A1D7C0E-0B7B-4C1B-9F11-AAAA00000001}_is1"
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


def compile_iss(src_iss, defines, out_iss_path):
    text = open(src_iss, encoding="utf8").read()
    text = text.replace("{{8F2C6E1B-5B1E-4E0C-9C55-3D7A6B1E9A42}", TEST_GUID)          # throw-away product id
    text = re.sub(r"(?m)^(WizardSmallImageFile|WizardImageFile|SetupIconFile)=assets\\", lambda m: f"{m.group(1)}={REPO}\\assets\\", text)
    text = text.replace("LicenseFile=LICENSE", f"LicenseFile={REPO}\\LICENSE")
    text = text.replace("SetupIconFile=..\\src-tauri\\icons\\icon.ico", f"SetupIconFile={ROOT}\\src-tauri\\icons\\icon.ico")
    open(out_iss_path, "w", encoding="utf8").write(text)
    r = run([iscc(), *[f"/D{k}={v}" for k, v in defines.items()], out_iss_path], cwd=os.path.dirname(out_iss_path))
    if r.returncode != 0:
        sys.exit("ISCC failed:\n" + r.stdout[-1500:] + r.stderr[-500:])


def reg_version():
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, KEY) as k:
            return winreg.QueryValueEx(k, "DisplayVersion")[0], winreg.QueryValueEx(k, "InstallLocation")[0]
    except OSError:
        return None, None


def main():
    work = tempfile.mkdtemp(prefix="pk_upgrade_")
    app = os.path.join(work, "app")
    marker = os.path.join(work, "relaunched.txt")
    new_iss = os.path.join(ROOT, "installer", "_upgrade_test.iss")        # must sit next to the real script (relative paths)
    try:
        stub = build_stub(work)
        # ---- fake 1.x payload: stub exe + a Python-ish _internal folder
        old_src = os.path.join(work, "old_src"); os.makedirs(os.path.join(old_src, "_internal", "lib"))
        shutil.copy(stub, os.path.join(old_src, "ProcessKiller.exe"))
        for n in ("python314.dll", "tcl86t.dll", os.path.join("lib", "_ssl.pyd")):
            open(os.path.join(old_src, "_internal", n), "wb").write(os.urandom(4096))
        out_old = os.path.join(work, "out_old"); os.makedirs(out_old)
        old_iss = os.path.join(work, "old.iss"); shutil.copy(os.path.join(REPO, "installer.iss"), old_iss)
        compile_iss(os.path.join(REPO, "installer.iss"), {"AppVersion": "1.0.3", "SourceDir": old_src, "OutDir": out_old}, old_iss)
        # ---- fake 2.x payload: a different exe (padded so it has a different hash/size)
        new_src = os.path.join(work, "new_src"); os.makedirs(new_src)
        shutil.copy(stub, os.path.join(new_src, "ProcessKiller.exe"))
        with open(os.path.join(new_src, "ProcessKiller.exe"), "ab") as f:
            f.write(b"\0" * 2048)
        out_new = os.path.join(work, "out_new"); os.makedirs(out_new)
        for n in ("wizard_small.bmp", "wizard_large.bmp"):                   # normally produced by release.py
            if not os.path.exists(os.path.join(ROOT, "installer", n)):
                shutil.copy(os.path.join(REPO, "assets", n), os.path.join(ROOT, "installer", n))
        compile_iss(os.path.join(ROOT, "installer", "ProcessKiller.iss"), {"AppVersion": "2.0.0", "SourceDir": new_src, "OutDir": out_new}, new_iss)

        # ---- 1) install the fake 1.0.3
        r = run([os.path.join(out_old, "ProcessKiller-Setup-1.0.3.exe"), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER", "/TASKS=", f"/DIR={app}"])
        v, loc = reg_version()
        check("1.x install registered", v == "1.0.3", f"version={v}")
        check("1.x has Python runtime folder", os.path.isdir(os.path.join(app, "_internal")))
        old_size = os.path.getsize(os.path.join(app, "ProcessKiller.exe"))

        # ---- 2) upgrade the way the 1.x updater does (NOTE: no /DIR - Inno must reuse the existing folder)
        env = dict(os.environ, PK_TEST_MARKER=marker)
        r = subprocess.run([os.path.join(out_new, "ProcessKiller-Setup-2.0.0.exe"), "/SILENT", "/SUPPRESSMSGBOXES", "/CLOSEAPPLICATIONS",
                            "/RESTARTAPPLICATIONS", "/NORESTART", "/RELAUNCH=1"], env=env, capture_output=True, text=True, timeout=180)
        check("2.x installer exit code 0", r.returncode == 0, f"code={r.returncode}")
        v, loc = reg_version()
        check("version updated to 2.0.0", v == "2.0.0", f"version={v}")
        check("same install folder reused", os.path.normcase(os.path.normpath(loc or "")) == os.path.normcase(os.path.normpath(app)) or os.path.isfile(os.path.join(app, "ProcessKiller.exe")), loc or "")
        check("old Python runtime removed (_internal)", not os.path.exists(os.path.join(app, "_internal")))
        check("exe replaced by 2.x build", os.path.getsize(os.path.join(app, "ProcessKiller.exe")) == old_size + 2048)
        for _ in range(40):
            if os.path.exists(marker):
                break
            time.sleep(0.25)
        check("app relaunched automatically (/RELAUNCH=1)", os.path.exists(marker) and os.path.normcase(open(marker).read()) == os.path.normcase(os.path.join(app, "ProcessKiller.exe")))

        # ---- 3) uninstall leaves nothing
        unins = os.path.join(app, "unins000.exe")
        if os.path.exists(unins):
            run([unins, "/VERYSILENT", "/SUPPRESSMSGBOXES"])
            time.sleep(2)
        check("uninstall removes registry entry", reg_version()[0] is None)
        check("uninstall removes install folder", not os.path.exists(os.path.join(app, "ProcessKiller.exe")))
    finally:
        try:
            os.remove(new_iss)
        except OSError:
            pass
        if reg_version()[0] is not None:       # never leave the throw-away entry behind
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
