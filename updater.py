# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Process Killer - self-update: check a manifest, download the installer, verify it, run it.

Manifest (latest.json) format:
    {"version": "1.2.0", "url": "https://.../ProcessKiller-Setup-1.2.0.exe",
     "sha256": "<hex>", "notes": "What's new...", "size": 12345678}
"""
import hashlib
import json
import os
import re
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request

from version import APP_NAME, UPDATE_SOURCE, __version__

SETTINGS_DIR = os.path.join(os.environ.get("APPDATA", "."), "ProcessKiller")
SETTINGS_FILE = os.path.join(SETTINGS_DIR, "settings.json")
LOCAL_HOSTS = {"127.0.0.1", "localhost", "::1"}


class UpdateError(Exception):
    pass


# ------------------------------------------------------------------ settings

def load_settings():
    try:
        with open(SETTINGS_FILE, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_settings(**changes):
    data = load_settings()
    data.update(changes)
    try:
        os.makedirs(SETTINGS_DIR, exist_ok=True)
        with open(SETTINGS_FILE, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)
    except OSError:
        pass


def auto_check_enabled():
    return load_settings().get("auto_check", True)


# ------------------------------------------------------------------ manifest

def parse_version(v):
    return tuple(int(x) for x in re.findall(r"\d+", str(v))[:4]) or (0,)


def manifest_url():
    src = (load_settings().get("update_source") or UPDATE_SOURCE or "").strip()
    if not src:
        return ""
    if src.startswith("github:"):
        return f"https://github.com/{src[7:].strip('/')}/releases/latest/download/latest.json"
    return src


def _check_url(url):
    u = urllib.parse.urlparse(url)
    if u.scheme != "https" and not (u.scheme == "http" and u.hostname in LOCAL_HOSTS):
        raise UpdateError("Updates must be served over HTTPS.")


def _get(url, timeout=15):
    _check_url(url)
    req = urllib.request.Request(url, headers={"User-Agent": f"{APP_NAME}/{__version__}"})
    return urllib.request.urlopen(req, timeout=timeout)


def check():
    """-> dict(available, current, version, url, sha256, notes, size). Raises UpdateError."""
    url = manifest_url()
    if not url:
        raise UpdateError("No update source is configured for this build.")
    try:
        with _get(url) as r:
            m = json.loads(r.read().decode("utf-8-sig"))
    except UpdateError:
        raise
    except Exception as e:  # noqa: BLE001
        raise UpdateError(f"Could not reach the update server ({e}).") from e
    for key in ("version", "url"):
        if key not in m:
            raise UpdateError(f"Update manifest is missing '{key}'.")
    _check_url(m["url"])
    return {"available": parse_version(m["version"]) > parse_version(__version__), "current": __version__,
            "version": str(m["version"]), "url": m["url"], "sha256": (m.get("sha256") or "").lower(),
            "notes": m.get("notes", ""), "size": int(m.get("size") or 0)}


# ------------------------------------------------------------------ download + install

def download(info, progress=None, cancel=lambda: False):
    """Download the installer to a temp folder and verify its hash. -> path"""
    folder = tempfile.mkdtemp(prefix="pk_update_")
    path = os.path.join(folder, os.path.basename(urllib.parse.urlparse(info["url"]).path) or "setup.exe")
    sha = hashlib.sha256()
    with _get(info["url"], timeout=30) as r, open(path, "wb") as f:
        total = int(r.headers.get("Content-Length") or info.get("size") or 0)
        done, last = 0, 0.0
        while True:
            if cancel():
                raise UpdateError("Download cancelled.")
            chunk = r.read(256 * 1024)
            if not chunk:
                break
            f.write(chunk)
            sha.update(chunk)
            done += len(chunk)
            if progress and time.time() - last > 0.05:
                last = time.time()
                progress(done, total)
        if progress:
            progress(done, total or done)
    if info.get("sha256") and sha.hexdigest() != info["sha256"]:
        os.remove(path)
        raise UpdateError("The downloaded file is corrupted (checksum mismatch). Try again.")
    return path


def run_installer(path, extra=()):
    """Launch the Inno Setup installer silently; it closes this app and restarts the new version."""
    args = [path, "/SILENT", "/SUPPRESSMSGBOXES", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS", "/NORESTART", *extra]
    flags = subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP
    subprocess.Popen(args, creationflags=flags, close_fds=True)
