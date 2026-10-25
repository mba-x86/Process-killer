# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Render a UI route to a PNG with headless Chrome and report JavaScript errors.

    py tools/shot.py dashboard                     -> .cache/shots/dashboard.png
    py tools/shot.py applications --out a.png --size 1440x960 --query "admin=0" --wait 4000

Each run starts its own throw-away web server on a free port (safe to run from several agents at once) and uses
the mock backend in ui/mock. Compare the result with design-ref/png/<Page>.png (same 1440x1500 default).
"""
import argparse, functools, http.server, os, re, shutil, socket, subprocess, sys, threading

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
UI = os.path.join(ROOT, "ui")
CACHE = os.path.join(ROOT, ".cache")


def chrome():
    for p in (r"C:\Program Files\Google\Chrome\Application\chrome.exe", r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
              r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"):
        if os.path.exists(p):
            return p
    return shutil.which("chrome") or shutil.which("msedge")


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("route")
    ap.add_argument("--out")
    ap.add_argument("--size", default="1440x1500")
    ap.add_argument("--query", default="")
    ap.add_argument("--wait", type=int, default=5000, help="virtual milliseconds to let timers/animations run")
    a = ap.parse_args()
    out = os.path.abspath(a.out or os.path.join(CACHE, "shots", a.route + ".png"))
    os.makedirs(os.path.dirname(out), exist_ok=True)

    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=UI))
    port = srv.server_address[1]
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{port}/index.html" + (f"?{a.query}" if a.query else "") + f"#/{a.route}"
    cmd = [chrome(), "--headless=new", "--disable-gpu", "--no-first-run", f"--user-data-dir={os.path.join(CACHE, 'chrome-profile-' + str(port))}",
           f"--window-size={a.size.replace('x', ',')}", f"--virtual-time-budget={a.wait}", "--enable-logging=stderr", "--v=0",
           f"--screenshot={out}", url]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    finally:
        srv.shutdown()
        shutil.rmtree(os.path.join(CACHE, "chrome-profile-" + str(port)), ignore_errors=True)
    errs = [l.strip() for l in r.stderr.splitlines() if "CONSOLE" in l and re.search(r"error|uncaught|failed", l, re.I)]
    print(f"screenshot: {out}" if os.path.exists(out) else "NO SCREENSHOT PRODUCED")
    print(f"js errors: {len(errs)}")
    for e in errs[:15]:
        print("  ", e[:240])
    sys.exit(0 if os.path.exists(out) and not errs else 1)


if __name__ == "__main__":
    main()
