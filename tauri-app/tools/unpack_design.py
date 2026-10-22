# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Unpack 'Process Killer Redesign.html' (a bundled design export) into plain reference files.

Run:  py -I tauri-app/tools/unpack_design.py
Writes (data only, nothing from the export is executed):
  tauri-app/design-ref/<Page>.html   markup + CSS of each screen (font-face block stripped)
  tauri-app/ui/fonts/*.woff2         Geist + Geist Mono (Latin subset) and OFL.txt
"""
import base64, gzip, html as H, json, os, re, shutil, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "Process Killer Redesign.html")
REF = os.path.join(ROOT, "tauri-app", "design-ref")
FONTS = os.path.join(ROOT, "tauri-app", "ui", "fonts")
CACHE = os.path.join(ROOT, "tauri-app", ".cache", "bundles")
PNG = os.path.join(REF, "png")
os.makedirs(REF, exist_ok=True)
os.makedirs(FONTS, exist_ok=True)


def block(kind, text):
    m = re.search(r'<script type="__bundler/%s">(.*?)</script>' % kind, text, re.S)
    return m.group(1) if m else None


def decode(entry):
    raw = base64.b64decode(entry["data"])
    return gzip.decompress(raw) if entry.get("compressed") else raw


outer = open(SRC, encoding="utf8").read()
manifest = json.loads(block("manifest", outer))
board = json.loads(block("template", outer)) if block("template", outer).lstrip().startswith('"') else block("template", outer)
names = dict((u, n) for n, u in re.findall(r'<h2>(.*?)</h2><iframe src="about:blank#([0-9a-f-]+)', board))
print("pages:", len(names))

# fonts + license text
lic = re.search(r"<!--(.*?)-->", outer, re.S)
if lic:
    open(os.path.join(FONTS, "OFL.txt"), "w", encoding="utf8").write(lic.group(1).strip() + "\n")

for uuid, entry in manifest.items():
    name = names.get(uuid, uuid[:8])
    inner = decode(entry).decode("utf8")
    im = json.loads(block("manifest", inner))
    tmpl = block("template", inner)
    html = json.loads(tmpl) if tmpl.lstrip().startswith('"') else tmpl
    # drop the (very long) @font-face block: fonts are bundled locally as ui/fonts/*.woff2
    html = re.sub(r"<style>/\* cyrillic-ext \*/.*?</style>", "", html, count=1, flags=re.S)
    slug = {"Startup & tasks": "Startup", "About & updates": "About", "Storage analyzer": "Storage"}.get(
        H.unescape(name), re.sub(r"[^A-Za-z0-9]+", "", H.unescape(name)))
    os.makedirs(CACHE, exist_ok=True)
    open(os.path.join(CACHE, slug + ".html"), "w", encoding="utf8").write(inner)   # full bundle, used for PNG render
    open(os.path.join(REF, slug + ".html"), "w", encoding="utf8").write(html)
    print(f"  {slug:14} {len(html) / 1024:5.1f} KB")
    for k, v in im.items():
        if v["mime"] == "font/woff2":
            data = decode(v)
            # the two latin files referenced by the design: Geist (~29 KB) and Geist Mono (~23 KB)
            out = "Geist.woff2" if len(data) > 26000 else "GeistMono.woff2"
            open(os.path.join(FONTS, out), "wb").write(data)
print("fonts:", sorted(os.listdir(FONTS)))


# ---- optional: render every screen to PNG with headless Chrome (visual target for the UI work)
def chrome():
    for p in (r"C:\Program Files\Google\Chrome\Application\chrome.exe",
              r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
              r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"):
        if os.path.exists(p):
            return p
    return shutil.which("chrome") or shutil.which("msedge")

if "--render" in sys.argv:
    os.makedirs(PNG, exist_ok=True)
    exe = chrome()
    profile = os.path.join(ROOT, "tauri-app", ".cache", "chrome-profile")
    for f in sorted(os.listdir(CACHE)):
        out = os.path.join(PNG, f[:-5] + ".png")
        subprocess.run([exe, "--headless=new", "--disable-gpu", "--no-first-run", f"--user-data-dir={profile}",
                        "--window-size=1440,1500", "--virtual-time-budget=6000", f"--screenshot={out}",
                        "file:///" + os.path.join(CACHE, f).replace(os.sep, "/")],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
        print("  png", os.path.basename(out), os.path.getsize(out) // 1024, "KB")
