# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Build the Task Reaper installer + update manifest.

    py tools/release.py                      build exe + installer + dist/latest.json
    py tools/release.py --notes "Fixed X"    release notes shown in the update banner
    py tools/release.py --publish            create a GitHub PRE-RELEASE (old users are NOT offered it yet)
    py tools/release.py --promote            make the pre-release the LATEST release (everyone gets the update)

Version = src-tauri/Cargo.toml == src-tauri/tauri.conf.json (must match). Output: dist/.
"""
import argparse, hashlib, json, os, re, shutil, subprocess, sys, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO = ROOT
TAURI = os.path.join(ROOT, "src-tauri")
INST = os.path.join(ROOT, "installer")
DIST = os.path.join(ROOT, "dist")
SLUG = "mba-x86/task-reaper"
WEBVIEW2_BOOTSTRAPPER = "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
DEFAULT_KEY = os.path.join(os.path.expanduser("~"), ".taskreaper", "update.key")   # secret key lives OUTSIDE the repo


def sh(cmd, **kw):
    print("\n>", " ".join(str(c) for c in cmd))
    subprocess.run(cmd, check=True, **kw)


def version():
    cargo = re.search(r'^version\s*=\s*"([^"]+)"', open(os.path.join(TAURI, "Cargo.toml"), encoding="utf8").read(), re.M).group(1)
    conf = json.load(open(os.path.join(TAURI, "tauri.conf.json"), encoding="utf8"))["version"]
    if cargo != conf:
        sys.exit(f"Version mismatch: Cargo.toml={cargo} tauri.conf.json={conf}")
    return cargo


def find(*names):
    for n in names:
        if os.path.exists(n):
            return n
        w = shutil.which(n)
        if w:
            return w


def make_wizard_images():
    from PIL import Image
    logo = Image.open(os.path.join(INST, "logo_256.png")).convert("RGBA")      # rendered by tools/make_icons.py from ui/js/icons.js
    for name, size in (("wizard_small.bmp", (55, 55)), ("wizard_large.bmp", (164, 314))):
        canvas = Image.new("RGB", size, (255, 255, 255))
        w, h = size
        side = min(w, h) - (8 if h < 100 else 40)
        art = logo.resize((side, side), Image.LANCZOS)
        canvas.paste(art, ((w - side) // 2, 24 if h > 100 else (h - side) // 2), art)
        canvas.save(os.path.join(INST, name))


def webview2_bootstrapper():
    """Bundle Microsoft's small WebView2 bootstrapper (installed only when the runtime is missing). Must carry a valid Microsoft signature."""
    dest = os.path.join(INST, "MicrosoftEdgeWebview2Setup.exe")
    if not os.path.exists(dest):
        print("downloading WebView2 bootstrapper ...")
        urllib.request.urlretrieve(WEBVIEW2_BOOTSTRAPPER, dest)
    ps = f"(Get-AuthenticodeSignature '{dest}').Status.ToString() + '|' + (Get-AuthenticodeSignature '{dest}').SignerCertificate.Subject"
    out = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True).stdout.strip()
    if not out.startswith("Valid") or "Microsoft" not in out:
        os.remove(dest)
        sys.exit(f"WebView2 bootstrapper signature check failed: {out}")
    print("bootstrapper signature OK:", out.split("|")[1][:60])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--notes", default="")
    ap.add_argument("--publish", action="store_true")
    ap.add_argument("--promote", action="store_true")
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--sign", default=os.environ.get("PK_MINISIGN_KEY") or (DEFAULT_KEY if os.path.exists(DEFAULT_KEY) else ""),
                    help="minisign secret key (default: %%USERPROFILE%%\\.taskreaper\\update.key); needs `rsign` (cargo install rsign2)")
    a = ap.parse_args()
    v = version()
    gh = find(r"C:\Program Files\GitHub CLI\gh.exe", "gh")
    if a.promote:
        sh([gh, "release", "edit", f"v{v}", "--repo", SLUG, "--prerelease=false", "--latest"])
        print(f"v{v} is now the latest release: installed apps will offer it.")
        return
    os.makedirs(DIST, exist_ok=True)
    if not a.skip_build:
        sh(["cargo", "build", "--release"], cwd=TAURI)
    exe = os.path.join(TAURI, "target", "release", "TaskReaper.exe")
    if not os.path.exists(exe):
        sys.exit(f"missing {exe}")
    make_wizard_images()
    webview2_bootstrapper()
    iscc = find(os.path.expandvars(r"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe"), r"C:\Program Files (x86)\Inno Setup 6\ISCC.exe", "iscc")
    sh([iscc, f"/DAppVersion={v}", f"/DSourceDir={os.path.dirname(exe)}", f"/DOutDir={DIST}", os.path.join(INST, "TaskReaper.iss")])
    setup = os.path.join(DIST, f"TaskReaper-Setup-{v}.exe")
    data = open(setup, "rb").read()
    notes = a.notes
    manifest = {"version": v, "url": f"https://github.com/{SLUG}/releases/download/v{v}/{os.path.basename(setup)}",
                "sha256": hashlib.sha256(data).hexdigest(), "size": len(data), "notes": notes}
    shipped_key = re.search(r'UPDATE_PUBLIC_KEY:\s*&str\s*=\s*"([^"]*)"', open(os.path.join(TAURI, "src", "updates.rs"), encoding="utf8").read()).group(1)
    if shipped_key and not a.sign:
        sys.exit("The app ships an update public key, so every release MUST be signed - and the secret key was not found "
                 f"({DEFAULT_KEY}). Unsigned updates are rejected by installed apps.")
    if a.sign:
        rsign = find(os.path.expandvars(r"%USERPROFILE%\.cargo\bin\rsign.exe"), "rsign")
        if not rsign:
            sys.exit("signing needs `rsign` (cargo install rsign2)")
        sh([rsign, "sign", "-s", a.sign, "-W", "-x", setup + ".minisig", "-t", f"Task Reaper {v}", setup])
        manifest["signature"] = open(setup + ".minisig", encoding="utf8").read()
    json.dump(manifest, open(os.path.join(DIST, "latest.json"), "w", encoding="utf8"), indent=2)
    print(f"\nexe      : {os.path.getsize(exe) / 1048576:.1f} MB\ninstaller: {setup} ({len(data) / 1048576:.1f} MB)\nsha256   : {manifest['sha256']}")
    if a.publish:
        files = [setup, os.path.join(DIST, "latest.json")] + ([setup + ".minisig"] if a.sign else [])
        branch = subprocess.run(["git", "rev-parse", "--abbrev-ref", "HEAD"], capture_output=True, text=True, cwd=REPO).stdout.strip() or "main"
        print(f"tagging v{v} on branch '{branch}' (it must already be pushed: git push origin {branch})")
        sh([gh, "release", "create", f"v{v}", *files, "--repo", SLUG, "--target", branch, "--title", f"Task Reaper {v}",
            "--notes", notes or f"Version {v}", "--prerelease"])
        print(f"Published v{v} as a PRE-RELEASE (installed apps do not see it). Test it, then: py tools/release.py --promote")


if __name__ == "__main__":
    main()
