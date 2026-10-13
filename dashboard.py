"""Process Killer - Dashboard (home) page."""
import os
import socket
import time
import tkinter as tk
from tkinter import ttk

import psutil

import core
import sysinfo
from pages_extra import Sparkline, fmt_size, gauge_color
from ui_kit import ACCENT, BORDER, CARD, CARD2, DANGER, FG, FONT, MUTED, OK, WARN, DataTable, Page, page_header


def dash_styles():
    s = ttk.Style()
    s.configure("CardAccent.TLabel", background=CARD, foreground=ACCENT, font=(FONT, 9, "bold"))
    s.configure("CardVal.TLabel", background=CARD, font=(FONT, 15, "bold"))
    s.configure("CardKey.TLabel", background=CARD, foreground=MUTED)


class RingStat(tk.Canvas):
    """Small full-circle gauge that eases to its value."""

    def __init__(self, parent, size=96, width=10):
        super().__init__(parent, width=size, height=size, bg=CARD, highlightthickness=0)
        pad = width / 2 + 3
        box = (pad, pad, size - pad, size - pad)
        self.create_oval(*box, outline=CARD2, width=width)
        self.bar = self.create_arc(*box, start=90, extent=-1, style="arc", width=width, outline=OK, state="hidden")
        self.txt = self.create_text(size / 2, size / 2, text="0%", fill=FG, font=(FONT, 16, "bold"))
        self.value = self.target = 0.0
        self._anim = False

    def set_target(self, v):
        self.target = float(v)
        if not self._anim:
            self._anim = True
            self._step()

    def _step(self):
        if not self.winfo_exists():
            return
        d = self.target - self.value
        self.value = self.target if abs(d) < 0.05 else self.value + d * 0.18
        v = self.value
        self.itemconfigure(self.bar, extent=-min(max(360 * v / 100, 1), 359.9), outline=gauge_color(v),
                           state="normal" if v >= 0.5 else "hidden")
        self.itemconfigure(self.txt, text=f"{v:.0f}%")
        if abs(self.target - self.value) >= 0.05:
            self.after(16, self._step)
        else:
            self._anim = False


class ScrollFrame(ttk.Frame):
    """Vertically scrollable container with mouse-wheel support."""

    def __init__(self, parent):
        super().__init__(parent, style="Card.TFrame")
        self.canvas = tk.Canvas(self, bg=CARD, highlightthickness=0)
        sb = ttk.Scrollbar(self, orient="vertical", command=self.canvas.yview)
        self.canvas.configure(yscrollcommand=sb.set)
        self.canvas.pack(side="left", fill="both", expand=True)
        sb.pack(side="right", fill="y")
        self.inner = ttk.Frame(self.canvas, style="Card.TFrame")
        self.win = self.canvas.create_window((0, 0), window=self.inner, anchor="nw")
        self.inner.bind("<Configure>", lambda e: self.canvas.configure(scrollregion=self.canvas.bbox("all")))
        self.canvas.bind("<Configure>", lambda e: self.canvas.itemconfigure(self.win, width=e.width))
        self.canvas.bind("<Enter>", lambda e: self.canvas.bind_all("<MouseWheel>", self._wheel))
        self.canvas.bind("<Leave>", lambda e: self.canvas.unbind_all("<MouseWheel>"))

    def _wheel(self, e):
        self.canvas.yview_scroll(int(-e.delta / 120) * 2, "units")

    def clear(self):
        for w in self.inner.winfo_children():
            w.destroy()
        self.canvas.yview_moveto(0)


class DashboardPage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        dash_styles()
        self.sections, self.ticking, self.tick_n = [], False, 0
        self.dx_lbl = None
        self.last_net = psutil.net_io_counters()
        self.last_t = time.time()
        self.peak = 1.0
        self.proc_pending = False

        head = page_header(self, "Dashboard", "")
        head.pack(fill="x")
        self.sub = head.sub
        self.sub.configure(text=f"{socket.gethostname()} · reading system information…")

        # ---- health banner with quick actions
        hb = ttk.Frame(self, style="Card.TFrame", padding=(18, 12))
        hb.pack(fill="x", pady=(14, 12))
        self.health = ttk.Label(hb, text="Checking your PC…", style="Card.TLabel", font=(FONT, 11, "bold"))
        self.health.pack(side="left")
        for text, style, cmd in [("⚡  Boost RAM", "Accent.TButton", self.quick_boost),
                                 ("🧹  Clean junk", "TButton", lambda: self.main.show("optimizer")),
                                 ("📊  Analyze storage", "TButton", lambda: self.main.show("storage"))][::-1]:
            ttk.Button(hb, text=text, style=style, command=cmd).pack(side="right", padx=(8, 0))

        # ---- live stat cards
        row = ttk.Frame(self)
        row.pack(fill="x")
        for i in range(4):
            row.columnconfigure(i, weight=1, uniform="c")
        self.cpu = self._ring_card(row, 0, "PROCESSOR")
        self.ram = self._ring_card(row, 1, "MEMORY")
        self.dsk = self._ring_card(row, 2, f"SYSTEM DRIVE ({os.environ.get('SystemDrive', 'C:')})")
        net = ttk.Frame(row, style="Card.TFrame", padding=16)
        net.grid(row=0, column=3, sticky="nsew", padx=(6, 0))
        ttk.Label(net, text="NETWORK", style="CardAccent.TLabel").pack(anchor="w")
        self.net_down = ttk.Label(net, text="↓ 0 B/s", style="CardVal.TLabel")
        self.net_down.pack(anchor="w", pady=(8, 0))
        self.net_up = ttk.Label(net, text="↑ 0 B/s", style="CardKey.TLabel")
        self.net_up.pack(anchor="w")
        self.net_spark = Sparkline(net, width=150, height=34)
        self.net_spark.pack(anchor="w", pady=(6, 0))

        # ---- lower area: specs (left) + drives / top processes (right)
        low = ttk.Frame(self)
        low.pack(fill="both", expand=True, pady=(12, 0))
        low.columnconfigure(0, weight=1)
        low.columnconfigure(1, weight=0)
        low.rowconfigure(0, weight=1)

        specs = ttk.Frame(low, style="Card.TFrame", padding=(18, 14, 8, 14))
        specs.grid(row=0, column=0, sticky="nsew")
        top = ttk.Frame(specs, style="Card.TFrame")
        top.pack(fill="x", pady=(0, 8), padx=(0, 10))
        ttk.Label(top, text="System specifications", style="CardH2.TLabel").pack(side="left")
        ttk.Button(top, text="Refresh", style="Ghost.TButton", command=self.load_specs).pack(side="right")
        ttk.Button(top, text="Copy report", style="Ghost.TButton", command=self.copy_report).pack(side="right")
        self.spec_scroll = ScrollFrame(specs)
        self.spec_scroll.pack(fill="both", expand=True)

        right = ttk.Frame(low, style="Card.TFrame", padding=16, width=360)
        right.grid(row=0, column=1, sticky="ns", padx=(12, 0))
        right.grid_propagate(False)
        ttk.Label(right, text="Drives", style="CardH2.TLabel").pack(anchor="w")
        self.drives_box = ttk.Frame(right, style="Card.TFrame")
        self.drives_box.pack(fill="x", pady=(8, 12))
        ttk.Label(right, text="Top apps by memory", style="CardH2.TLabel").pack(anchor="w")
        self.top = DataTable(right, [("name", "App", 150, 1), ("mem", "Memory", 80), ("cpu", "CPU", 50)],
                             selectmode="none")
        self.top.tree.configure(height=6)
        self.top.pack(fill="both", expand=True, pady=(8, 0))
        self.top.sort_by("mem")
        self.top.sort_by("mem")

        self.load_specs()

    def _ring_card(self, parent, col, title):
        f = ttk.Frame(parent, style="Card.TFrame", padding=16)
        f.grid(row=0, column=col, sticky="nsew", padx=(0 if col == 0 else 6, 6))
        ttk.Label(f, text=title, style="CardAccent.TLabel").pack(anchor="w")
        body = ttk.Frame(f, style="Card.TFrame")
        body.pack(fill="x", pady=(8, 0))
        ring = RingStat(body)
        ring.pack(side="left")
        txt = ttk.Frame(body, style="Card.TFrame")
        txt.pack(side="left", padx=(14, 0))
        big = ttk.Label(txt, text="—", style="CardVal.TLabel")
        big.pack(anchor="w")
        small = ttk.Label(txt, text="", style="CardKey.TLabel", justify="left")
        small.pack(anchor="w")
        return {"ring": ring, "big": big, "small": small}

    # ---- specs
    def load_specs(self):
        sf = self.spec_scroll
        sf.clear()
        ttk.Label(sf.inner, text="Reading hardware information…", style="CardMuted.TLabel").pack(anchor="w", pady=8)
        pb = ttk.Progressbar(sf.inner, mode="indeterminate", style="Thin.Horizontal.TProgressbar", length=260)
        pb.pack(anchor="w")
        pb.start(12)
        self.main.bg(sysinfo.collect, self._specs_loaded, lambda e: self._specs_loaded([]))

    def _specs_loaded(self, sections):
        self.sections = sections
        sf = self.spec_scroll
        sf.clear()
        if not sections:
            ttk.Label(sf.inner, text="Could not read hardware information.", style="CardMuted.TLabel").pack(anchor="w")
            return
        for title, rows in sections:
            ttk.Label(sf.inner, text=title.upper(), style="CardAccent.TLabel").pack(anchor="w", pady=(10, 4))
            grid = ttk.Frame(sf.inner, style="Card.TFrame")
            grid.pack(fill="x")
            grid.columnconfigure(1, weight=1)
            for i, (k, v) in enumerate(rows):
                ttk.Label(grid, text=k, style="CardKey.TLabel", width=24).grid(row=i, column=0, sticky="nw", pady=2)
                val = ttk.Label(grid, text=v, style="Card.TLabel", wraplength=420, justify="left")
                val.grid(row=i, column=1, sticky="nw", pady=2)
                if k == "DirectX":
                    self.dx_lbl = val
        if getattr(self, "dx_lbl", None) is not None and self.dx_lbl.winfo_exists():
            self.main.bg(sysinfo.directx_text, self._dx_done)
        dev = dict(sections[0][1]) if sections else {}
        osr = dict(sections[1][1]) if len(sections) > 1 else {}
        self.sub.configure(text=f"{dev.get('Device name', socket.gethostname())} · {osr.get('Edition', 'Windows')}"
                                f" · {dev.get('Model', '')} · up {sysinfo.uptime_text()}")

    def _dx_done(self, text):
        if self.dx_lbl.winfo_exists():
            self.dx_lbl.configure(text=text)
        for title, rows in self.sections:
            for n, (k, _v) in enumerate(rows):
                if k == "DirectX":
                    rows[n] = (k, text)

    def copy_report(self):
        if self.sections:
            self.clipboard_clear()
            self.clipboard_append(sysinfo.report_text(self.sections))
            self.main.toast("System report copied to the clipboard")

    # ---- live data
    def on_show(self):
        if not self.ticking:
            self.ticking = True
            self.after(80, self._tick)

    def _tick(self):
        if not self.winfo_exists():
            return
        if not self.winfo_ismapped():
            self.ticking = False
            return
        self.tick_n += 1
        cpu = psutil.cpu_percent(interval=None)
        self.cpu["ring"].set_target(cpu)
        self.cpu["big"].configure(text=f"{psutil.cpu_count(logical=False) or '?'}C / {psutil.cpu_count()}T")
        f = psutil.cpu_freq()
        self.cpu["small"].configure(text=f"{f.current / 1000:.2f} GHz now" if f else "")
        r = psutil.virtual_memory()
        self.ram["ring"].set_target(r.percent)
        self.ram["big"].configure(text=f"{fmt_size(r.total - r.available)}")
        self.ram["small"].configure(text=f"of {fmt_size(r.total)}\n{fmt_size(r.available)} free")
        if self.tick_n % 10 == 1:
            self._update_drives()
        now, net = time.time(), psutil.net_io_counters()
        dt = max(now - self.last_t, 0.001)
        down, up = (net.bytes_recv - self.last_net.bytes_recv) / dt, (net.bytes_sent - self.last_net.bytes_sent) / dt
        self.last_net, self.last_t = net, now
        self.net_down.configure(text=f"↓ {fmt_size(down)}/s")
        self.net_up.configure(text=f"↑ {fmt_size(up)}/s")
        self.peak = max(self.peak * 0.98, down, 50_000)
        self.net_spark.push(100 * down / self.peak)
        if self.tick_n % 3 == 1:
            self._refresh_top()
        self._health(cpu, r.percent)
        self.after(1000, self._tick)

    def _update_drives(self):
        parts = []
        for p in psutil.disk_partitions(all=False):
            if "cdrom" in p.opts or not p.fstype:
                continue
            try:
                parts.append((p.mountpoint, psutil.disk_usage(p.mountpoint)))
            except OSError:
                continue
        for w in self.drives_box.winfo_children():
            w.destroy()
        sysdrive = os.environ.get("SystemDrive", "C:").upper()
        self.drive_pct = {}
        for mount, u in parts:
            self.drive_pct[mount.upper()[:2]] = u.percent
            line = ttk.Frame(self.drives_box, style="Card.TFrame")
            line.pack(fill="x", pady=(0, 8))
            ttk.Label(line, text=mount, style="Card.TLabel", font=(FONT, 10, "bold")).pack(side="left")
            ttk.Label(line, text=f"{fmt_size(u.free)} free of {fmt_size(u.total)}", style="CardKey.TLabel").pack(side="right")
            c = tk.Canvas(self.drives_box, height=6, bg=CARD2, highlightthickness=0)
            c.pack(fill="x", pady=(0, 6))
            c.bind("<Configure>", lambda e, c=c, pct=u.percent: (c.delete("all"), c.create_rectangle(
                0, 0, e.width * pct / 100, 6, fill=gauge_color(pct), width=0)))
        sd = psutil.disk_usage(sysdrive + "\\")
        self.dsk["ring"].set_target(sd.percent)
        self.dsk["big"].configure(text=f"{fmt_size(sd.free)} free")
        self.dsk["small"].configure(text=f"{fmt_size(sd.used)} used of {fmt_size(sd.total)}")

    def _refresh_top(self):
        if self.proc_pending:
            return
        self.proc_pending = True

        def work():
            agg = {}
            for p in psutil.process_iter(["name", "memory_info", "cpu_percent"]):
                i = p.info
                if not i["memory_info"]:
                    continue
                a = agg.setdefault(i["name"] or "?", [0, 0.0, 0])
                a[0] += i["memory_info"].rss
                a[1] += i["cpu_percent"] or 0
                a[2] += 1
            return sorted(agg.items(), key=lambda kv: kv[1][0], reverse=True)[:6]

        def done(rows):
            self.proc_pending = False
            keep = set()
            for name, (mem, cpu, n) in rows:
                keep.add(name)
                label = name if n == 1 else f"{name} ×{n}"
                self.top.add(name, (label, fmt_size(mem), f"{cpu / (psutil.cpu_count() or 1):.0f}%"), {"mem": mem})
            for iid in set(self.top.tree.get_children()) - keep:
                self.top.tree.delete(iid)
            self.top.resort()
        self.main.bg(work, done, lambda e: setattr(self, "proc_pending", False))

    def _health(self, cpu, ram):
        issues = []
        if ram >= 85:
            issues.append(f"memory is high ({ram:.0f}%)")
        if cpu >= 90:
            issues.append(f"CPU is busy ({cpu:.0f}%)")
        for d, pct in getattr(self, "drive_pct", {}).items():
            if pct >= 90:
                issues.append(f"{d} is almost full ({pct:.0f}%)")
        if sysinfo.uptime_days() >= 14:
            issues.append(f"no restart for {sysinfo.uptime_days():.0f} days")
        if issues:
            self.health.configure(text="⚠  Needs attention: " + ", ".join(issues), foreground=WARN)
        else:
            self.health.configure(text="●  Your PC is running well", foreground=OK)

    def quick_boost(self):
        self.main.show("optimizer")
        self.main.pages["optimizer"].boost_ram()
