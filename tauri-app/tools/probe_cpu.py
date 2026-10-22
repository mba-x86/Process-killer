# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Which process of the running app burns idle CPU?   py tauri-app/tools/probe_cpu.py <exe> [route]"""
import os, subprocess, sys, time

import psutil

exe = sys.argv[1]
route = sys.argv[2] if len(sys.argv) > 2 else None
env = dict(os.environ)
p = subprocess.Popen([exe], env=env)
time.sleep(7)
root = psutil.Process(p.pid)
procs = [root] + root.children(recursive=True)
for x in procs:
    x.cpu_percent(None)
time.sleep(10)
n = psutil.cpu_count()
rows = []
for x in procs:
    try:
        cmd = " ".join(x.cmdline())
        kind = next((a.split("=")[1] for a in x.cmdline() if a.startswith("--type=")), "app")
        if "utility" in cmd and "network" in cmd:
            kind = "utility:network"
        elif "utility" in cmd and "storage" in cmd:
            kind = "utility:storage"
        rows.append((x.cpu_percent(None) / n, kind, x.name(), x.memory_full_info().uss / 1048576))
    except psutil.Error:
        pass
print(f"{'cpu%':>6}  {'role':16} {'process':22} private MB")
for r in sorted(rows, reverse=True):
    print(f"{r[0]:6.2f}  {r[1]:16} {r[2]:22} {r[3]:7.1f}")
print(f"{sum(r[0] for r in rows):6.2f}  TOTAL")
for c in root.children(recursive=True):
    try:
        c.kill()
    except psutil.Error:
        pass
p.kill()
