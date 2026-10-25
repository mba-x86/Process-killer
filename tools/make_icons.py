# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Render the Task Reaper app icon from the SAME vector mark the UI draws (ui/js/icons.js, function logo()).

    py tools/make_icons.py

Writes src-tauri/icons/{icon.ico,icon.png,32x32.png,128x128.png,128x128@2x.png} and installer/logo_256.png.
Each size is rendered by Chrome from the SVG (not scaled down from a big bitmap), so the 16 px icon is as crisp as it can be.
"""
import os, re, struct, subprocess, sys, tempfile
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICONS = os.path.join(ROOT, "src-tauri", "icons")
CHROME = next((p for p in (r"C:\Program Files\Google\Chrome\Application\chrome.exe", r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
                           r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe") if os.path.exists(p)), None)
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)


def mark():
    js = open(os.path.join(ROOT, "ui", "js", "icons.js"), encoding="utf8").read()
    m = re.search(r"svg\.innerHTML = '([^']+)';", js[js.index("export function logo"):])
    if not m:
        sys.exit("could not find the logo markup in ui/js/icons.js")
    return m.group(1)


def tile_svg(size, inner):
    frac = 0.80 if size <= 32 else 0.70            # small sizes: bigger mark inside the tile so it stays legible
    k = 100 * frac / 28
    t = (100 - 28 * k) / 2
    border = '<rect x="1" y="1" width="98" height="98" rx="23" fill="#111418" stroke="#2A313B" stroke-width="2"/>' if size > 24 else \
             '<rect x="0" y="0" width="100" height="100" rx="22" fill="#111418"/>'
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 100 100">{border}'
            f'<g transform="translate({t:.2f} {t:.2f}) scale({k:.4f})" fill="none">{inner}</g></svg>')


def render(size, inner, out):
    if not CHROME:
        sys.exit("Chrome or Edge is needed to render the icons")
    with tempfile.TemporaryDirectory() as d:
        page = os.path.join(d, "i.html")
        open(page, "w", encoding="utf8").write(f'<!doctype html><body style="margin:0;background:transparent">{tile_svg(size, inner)}</body>')
        shot = os.path.join(d, "s.png")
        subprocess.run([CHROME, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1", "--default-background-color=00000000",
                        "--window-size=600,600", f"--screenshot={shot}", "file:///" + page.replace("\\", "/")], check=True, capture_output=True)
        Image.open(shot).convert("RGBA").crop((0, 0, size, size)).save(out)


def write_ico(pngs, path):
    blobs = [open(p, "rb").read() for _, p in pngs]
    head = struct.pack("<HHH", 0, 1, len(pngs))
    off = 6 + 16 * len(pngs)
    entries = b""
    for (s, _), b in zip(pngs, blobs):
        entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(b), off)
        off += len(b)
    open(path, "wb").write(head + entries + b"".join(blobs))


def main():
    inner = mark()
    with tempfile.TemporaryDirectory() as d:
        pngs = []
        for s in ICO_SIZES:
            p = os.path.join(d, f"{s}.png")
            render(s, inner, p)
            pngs.append((s, p))
        write_ico(pngs, os.path.join(ICONS, "icon.ico"))
    for s, name in ((32, "32x32.png"), (128, "128x128.png"), (256, "128x128@2x.png"), (512, "icon.png")):
        render(s, inner, os.path.join(ICONS, name))
    render(256, inner, os.path.join(ROOT, "installer", "logo_256.png"))
    print("icons written to", ICONS)


if __name__ == "__main__":
    main()
