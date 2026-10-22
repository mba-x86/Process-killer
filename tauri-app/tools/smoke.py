# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Interaction smoke tests for the Tauri UI (mock backend), run in headless Chrome. Python stdlib only.

    py tauri-app/tools/smoke.py                    run every scenario file in tauri-app/dev/scenarios
    py tauri-app/tools/smoke.py dashboard storage  run only those files
    py tauri-app/tools/smoke.py -v                 also print notes / per-scenario details
    py tauri-app/tools/smoke.py --list

One throw-away local server serves tauri-app/ui at "/" and tauri-app/dev at "/__dev/" (same origin). For each scenario
file one headless Chrome loads /__dev/smoke.html?run=<file> with a virtual-time budget; the page fills <pre id="result">
with JSON, which is parsed here. Exit code 1 if anything fails. The harness lives in tauri-app/dev (NOT in ui/, which is
embedded in the shipped app): the server injects dev/probe.js + dev/hook.js into index.html only when ?__smoke is present.
"""
import argparse, concurrent.futures as cf, functools, html, http.server, json, mimetypes, os, posixpath, re, shutil, socket, subprocess, sys, threading, urllib.parse

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
UI = os.path.join(ROOT, "ui")
DEV = os.path.join(ROOT, "dev")
SCEN = os.path.join(DEV, "scenarios")
CACHE = os.path.join(ROOT, ".cache")
INJECT = '<script src="/__dev/probe.js"></script><script type="module" src="/__dev/hook.js"></script>\n'

mimetypes.add_type("text/javascript", ".js")
mimetypes.add_type("text/javascript", ".mjs")
mimetypes.add_type("font/woff2", ".woff2")


def chrome():
    for p in (r"C:\Program Files\Google\Chrome\Application\chrome.exe", r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
              r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"):
        if os.path.exists(p):
            return p
    return shutil.which("chrome") or shutil.which("msedge")


class Server(http.server.ThreadingHTTPServer):
    request_queue_size = 256      # the default (5) drops connections when Chrome fetches ES modules in parallel
    daemon_threads = True


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def translate_path(self, path):
        p = urllib.parse.unquote(urllib.parse.urlsplit(path).path)
        base, rel = (DEV, p[len("/__dev/"):]) if p.startswith("/__dev/") else (UI, p.lstrip("/"))
        rel = posixpath.normpath(rel)
        if rel in (".", "") or rel.startswith(".."):
            return os.path.join(base, "index.html") if base == UI else base
        return os.path.join(base, *rel.split("/"))

    def do_GET(self):
        u = urllib.parse.urlsplit(self.path)
        if u.path in ("/", "/index.html") and "__smoke" in u.query:
            body = open(os.path.join(UI, "index.html"), encoding="utf-8").read()
            body = body.replace('<script type="module" src="js/main.js">', INJECT + '<script type="module" src="js/main.js">', 1).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        super().do_GET()


def static_checks():
    """No file under ui/ may import anything outside ui/ (or over the network)."""
    bad = []
    rx = re.compile(r"""(?:import\s*(?:[^'"()]*?from\s*)?|import\s*\(\s*|export\s[^'"]*?from\s*)['"]([^'"]+)['"]""")
    for dirpath, _, names in os.walk(UI):
        for n in names:
            if not n.endswith((".js", ".mjs", ".html", ".css")):
                continue
            full = os.path.join(dirpath, n)
            text = open(full, encoding="utf-8", errors="replace").read()
            specs = rx.findall(text) if n.endswith((".js", ".mjs")) else re.findall(r"""(?:src|href)=["']([^"']+)["']|url\(["']?([^)"']+)""", text)
            for s in specs:
                s = s if isinstance(s, str) else (s[0] or s[1])
                if not s or s.startswith(("data:", "#")):
                    continue
                if re.match(r"^[a-z]+:", s, re.I) or s.startswith("//"):
                    bad.append(f"{os.path.relpath(full, ROOT)}: network import {s}")
                    continue
                target = os.path.normpath(os.path.join(UI, s.lstrip("/")) if s.startswith("/") else os.path.join(dirpath, s.split("?")[0].split("#")[0]))
                if not target.startswith(UI + os.sep) and target != UI:
                    bad.append(f"{os.path.relpath(full, ROOT)}: imports outside ui/: {s}")
                elif n.endswith((".js", ".mjs")) and not os.path.exists(target.split("?")[0]):
                    bad.append(f"{os.path.relpath(full, ROOT)}: import target does not exist: {s}")
    return bad


def run_file(port, name, budget, timeout):
    prof = os.path.join(CACHE, f"smoke-profile-{port}-{name}")
    url = f"http://127.0.0.1:{port}/__dev/smoke.html?run={name}"
    cmd = [chrome(), "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check", f"--user-data-dir={prof}",
           "--window-size=1500,1000", f"--virtual-time-budget={budget}", "--dump-dom", url]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, encoding="utf-8", errors="replace")
        m = re.search(r'<pre id="result">(.*?)</pre>', r.stdout, re.S)
        if not m or not m.group(1).strip():
            return name, None, f"no result produced (chrome exit {r.returncode}); stderr: {r.stderr.strip()[-300:]}"
        return name, json.loads(html.unescape(m.group(1))), None
    except subprocess.TimeoutExpired:
        return name, None, f"chrome timed out after {timeout}s"
    except Exception as e:  # noqa: BLE001
        return name, None, f"{type(e).__name__}: {e}"
    finally:
        shutil.rmtree(prof, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("files", nargs="*", help="scenario file names (default: all)")
    ap.add_argument("-v", "--verbose", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--jobs", type=int, default=4, help="parallel Chrome instances")
    ap.add_argument("--budget", type=int, default=1500000, help="virtual milliseconds per scenario file")
    ap.add_argument("--timeout", type=int, default=300, help="real seconds per scenario file")
    a = ap.parse_args()
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")   # Windows consoles are often cp1252/cp1256
        except Exception:  # noqa: BLE001
            pass

    names = sorted(f[:-3] for f in os.listdir(SCEN) if f.endswith(".js"))
    if a.list:
        print("\n".join(names)); return 0
    if a.files:
        unknown = [f for f in a.files if f not in names]
        if unknown:
            print("unknown scenario file(s):", ", ".join(unknown), "- available:", ", ".join(names)); return 2
        names = a.files
    if not chrome():
        print("Chrome/Edge not found"); return 2

    srv = Server(("127.0.0.1", 0), Handler)
    port = srv.server_address[1]
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    os.makedirs(CACHE, exist_ok=True)
    results, errors = {}, {}
    try:
        with cf.ThreadPoolExecutor(max_workers=max(1, a.jobs)) as ex:
            for name, res, err in ex.map(lambda n: run_file(port, n, a.budget, a.timeout), names):
                (errors if err else results)[name] = err or res
    finally:
        srv.shutdown()

    total = passed = 0
    failures = []
    print(f"{'scenario':<64} {'result':<6} {'virt s':>7}")
    print("-" * 80)
    for name in names:
        if name in errors:
            total += 1
            print(f"{name + ': <file>':<64} {'ERROR':<6}")
            failures.append((name, [errors[name]]))
            continue
        r = results[name]
        if r.get("fatal"):
            failures.append((name, [f"harness fatal: {r['fatal']}"]))
        for fname, fr in r["files"].items():
            if fr.get("error"):
                total += 1; failures.append((fname, [fr["error"]]))
            for sc in fr["scenarios"]:
                total += 1
                passed += sc["ok"]
                print(f"{sc['name'][:63]:<64} {'ok' if sc['ok'] else 'FAIL':<6} {sc['ms'] / 1000:>7.1f}")
                if a.verbose:
                    for n in sc["notes"]:
                        print("      note:", n[:300])
                if not sc["ok"]:
                    failures.append((sc["name"], sc["failures"]))
        if not r.get("finished"):
            failures.append((name, ["run did not finish within the virtual-time budget (partial results above)"]))
    static = static_checks()
    total += 1
    print(f"{'static: no import outside ui/ (and all import targets exist)':<64} {'ok' if not static else 'FAIL':<6}")
    passed += not static
    if static:
        failures.append(("static imports", static))
    print("-" * 80)
    print(f"{passed}/{total} scenarios passed")
    for title, msgs in failures:
        print(f"\nFAIL {title}")
        seen = set()
        for m in msgs:
            if m not in seen:
                seen.add(m); print("   -", m[:500])
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
