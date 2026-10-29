# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Measure launch time, memory and idle CPU of a built Task Reaper exe.

    py tools/measure.py path\\to\\TaskReaper.exe [--runs 3] [--window-title "Task Reaper"]

Build a prompt-free test exe first:   set PK_NO_ADMIN_MANIFEST=1 && cargo build --release   (never ship that build)
Reports: time until the main window is visible, memory of the whole process tree (our exe + the WebView2 helper processes),
idle CPU after the UI settled, and file size.
"""
import argparse, ctypes, os, statistics, subprocess, sys, time

import psutil

user32 = ctypes.windll.user32


def window_visible(title):
    hwnd = user32.FindWindowW(None, title)
    return bool(hwnd) and bool(user32.IsWindowVisible(hwnd))


def kill_tree(p):
    try:
        for c in psutil.Process(p.pid).children(recursive=True):
            try:
                c.kill()
            except psutil.Error:
                pass
        p.kill()
    except psutil.Error:
        pass


def one_run(exe, title, settle):
    t0 = time.perf_counter()
    p = subprocess.Popen([exe])
    shown = None
    while time.perf_counter() - t0 < 30:
        if window_visible(title):
            shown = time.perf_counter() - t0
            break
        time.sleep(0.01)
    time.sleep(settle)                                           # let pages load / first polls finish
    try:
        root = psutil.Process(p.pid)
        procs = [root] + root.children(recursive=True)
        for x in procs:
            x.cpu_percent(None)
        time.sleep(5)
        cpu = sum(x.cpu_percent(None) for x in procs if x.is_running()) / (psutil.cpu_count() or 1)
        rss = priv = 0
        for x in procs:
            try:
                rss += x.memory_info().rss
                priv += x.memory_full_info().uss
            except psutil.Error:
                pass
        n = len(procs)
    except psutil.Error:
        rss = priv = n = cpu = 0
    kill_tree(p)
    return shown, rss / 1048576, priv / 1048576, cpu, n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("exe")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--window-title", default="Task Reaper")
    ap.add_argument("--settle", type=float, default=4.0)
    a = ap.parse_args()
    if not os.path.exists(a.exe):
        sys.exit("exe not found")
    rows = []
    for i in range(a.runs):
        r = one_run(a.exe, a.window_title, a.settle)
        rows.append(r)
        print(f"run {i + 1}: window visible after {r[0] and round(r[0], 2)} s | tree RSS {r[1]:.0f} MB, private {r[2]:.0f} MB, {r[4]} processes | idle CPU {r[3]:.1f}%")
        time.sleep(1.5)
    ok = [r for r in rows if r[0]]
    if ok:
        print(f"\nmedian: window {statistics.median(r[0] for r in ok):.2f} s | RSS {statistics.median(r[1] for r in ok):.0f} MB | "
              f"private {statistics.median(r[2] for r in ok):.0f} MB | idle CPU {statistics.median(r[3] for r in ok):.1f}% | exe {os.path.getsize(a.exe) / 1048576:.1f} MB")


if __name__ == "__main__":
    main()
