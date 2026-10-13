"""Build the Windows installer and the update manifest.

    py build.py                      build dist/ProcessKiller-Setup-<version>.exe + dist/latest.json
    py build.py --notes "Fixed X"    release notes shown to users in the update banner
    py build.py --publish            also create a GitHub release with `gh` (needs gh + `gh auth login`)
    py build.py --no-admin --out dist-test --skip-installer     quick test build

The version comes from version.py - bump __version__ there before every release.
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
os.chdir(ROOT)
ASSETS = "assets"
LOGO = os.path.join(ASSETS, "Process Killer Logo.png")


def sh(cmd, **kw):
    print("\n>", " ".join(cmd))
    subprocess.run(cmd, check=True, **kw)


def read_version():
    src = open("version.py", encoding="utf8").read()
    return (re.search(r'__version__\s*=\s*"([^"]+)"', src).group(1),
            re.search(r'UPDATE_SOURCE\s*=\s*"([^"]*)"', src).group(1))


def make_images():
    """Multi-size .ico for the exe + installer artwork, generated from the logo PNG."""
    from PIL import Image
    logo = Image.open(LOGO).convert("RGBA")
    ico = os.path.join(ASSETS, "app.ico")
    logo.save(ico, sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])

    def bmp(name, size, bg):
        canvas = Image.new("RGB", size, bg)
        w, h = size
        side = min(w, h) - (8 if h < 100 else 40)
        art = logo.resize((side, side), Image.LANCZOS)
        canvas.paste(art, ((w - side) // 2, 24 if h > 100 else (h - side) // 2), art)
        canvas.save(os.path.join(ASSETS, name))
    bmp("wizard_small.bmp", (55, 55), (255, 255, 255))
    bmp("wizard_large.bmp", (164, 314), (255, 255, 255))


def version_file(version):
    v = tuple((list(map(int, re.findall(r"\d+", version))) + [0, 0, 0, 0])[:4])
    text = f"""VSVersionInfo(
  ffi=FixedFileInfo(filevers={v}, prodvers={v}, mask=0x3f, flags=0x0, OS=0x40004, fileType=0x1, subtype=0x0, date=(0, 0)),
  kids=[StringFileInfo([StringTable('040904B0', [
    StringStruct('CompanyName', 'Process Killer'), StringStruct('FileDescription', 'Process Killer'),
    StringStruct('FileVersion', '{version}'), StringStruct('InternalName', 'ProcessKiller'),
    StringStruct('OriginalFilename', 'ProcessKiller.exe'), StringStruct('ProductName', 'Process Killer'),
    StringStruct('ProductVersion', '{version}')])]), VarFileInfo([VarStruct('Translation', [1033, 1200])])])
"""
    path = os.path.join("build", "version_info.txt")
    os.makedirs("build", exist_ok=True)
    open(path, "w", encoding="utf8").write(text)
    return path


def find_gh():
    return shutil.which("gh") or next((p for p in (r"C:\Program Files\GitHub CLI\gh.exe",) if os.path.exists(p)), None)


def find_iscc():
    for p in (shutil.which("iscc"),
              os.path.expandvars(r"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe"),
              r"C:\Program Files (x86)\Inno Setup 6\ISCC.exe", r"C:\Program Files\Inno Setup 6\ISCC.exe"):
        if p and os.path.exists(p):
            return p
    sys.exit("Inno Setup not found. Install it:  winget install JRSoftware.InnoSetup")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--notes", default="")
    ap.add_argument("--out", default="dist")
    ap.add_argument("--no-admin", action="store_true", help="don't force the UAC prompt (testing only)")
    ap.add_argument("--skip-installer", action="store_true")
    ap.add_argument("--publish", action="store_true")
    a = ap.parse_args()
    version, source = read_version()
    print(f"Building Process Killer {version}")

    make_images()
    app_dir = os.path.join(a.out, "ProcessKiller")
    shutil.rmtree(app_dir, ignore_errors=True)
    cmd = [sys.executable, "-m", "PyInstaller", "--noconfirm", "--clean", "--noconsole", "--name", "ProcessKiller",
           "--icon", os.path.abspath(os.path.join(ASSETS, "app.ico")),
           "--add-data", f"{os.path.abspath(ASSETS)}{os.pathsep}{ASSETS}",
           "--version-file", os.path.abspath(version_file(version)), "--distpath", os.path.abspath(a.out),
           "--workpath", os.path.abspath(os.path.join("build", "pyi")), "--specpath", os.path.abspath("build")]
    if not a.no_admin:
        cmd.append("--uac-admin")
    # keep the bundle lean: nothing here uses these
    for m in ("numpy", "matplotlib", "scipy", "pandas", "IPython", "pytest", "setuptools", "unittest"):
        cmd += ["--exclude-module", m]
    sh(cmd + [os.path.abspath("app.py")])
    if a.skip_installer:
        print("\nBuilt:", os.path.join(app_dir, "ProcessKiller.exe"))
        return

    sh([find_iscc(), f"/DAppVersion={version}", f"/DSourceDir={os.path.abspath(app_dir)}",
        f"/DOutDir={os.path.abspath(a.out)}", "installer.iss"])
    setup = os.path.join(a.out, f"ProcessKiller-Setup-{version}.exe")
    digest = hashlib.sha256(open(setup, "rb").read()).hexdigest()

    notes = a.notes
    if not notes and os.path.exists("release_notes.md"):
        notes = open("release_notes.md", encoding="utf8").read().strip()
    if source.startswith("github:"):
        url = f"https://github.com/{source[7:]}/releases/download/v{version}/{os.path.basename(setup)}"
    else:
        url = f"https://YOUR-SERVER/{os.path.basename(setup)}"
    manifest = {"version": version, "url": url, "sha256": digest, "size": os.path.getsize(setup), "notes": notes}
    mpath = os.path.join(a.out, "latest.json")
    json.dump(manifest, open(mpath, "w", encoding="utf8"), indent=2)

    print(f"\nInstaller : {setup}  ({manifest['size'] / 1048576:.1f} MB)\nManifest  : {mpath}\nSHA-256   : {digest}")
    if a.publish:
        gh = find_gh()
        if not gh or not source.startswith("github:"):
            sys.exit("--publish needs the GitHub CLI (gh) and UPDATE_SOURCE = 'github:OWNER/REPO' in version.py")
        sh([gh, "release", "create", f"v{version}", setup, mpath, "--repo", source[7:],
            "--title", f"Process Killer {version}", "--notes", notes or f"Version {version}"])
        print("Published. Users will see the update the next time they open the app.")
    else:
        print("\nTo publish: upload BOTH files to a GitHub release tagged v" + version +
              " (or run `py build.py --publish`). Users get the update banner automatically.")


if __name__ == "__main__":
    main()
