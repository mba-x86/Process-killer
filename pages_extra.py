# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Process Killer - Optimizer (RAM + junk) and Storage analyzer pages."""
import os
import subprocess
import time
import tkinter as tk
from collections import deque
from tkinter import filedialog, messagebox, ttk

import core
import optimizer
from ui_kit import (ACCENT, BORDER, CARD, CARD2, DANGER, FG, FONT, MUTED, OK, WARN, DataTable, Page,
                    SearchBox, page_header)


# ------------------------------------------------------------------ helpers

def fmt_size(b):
    b = float(b)
    for unit in ("B", "KB", "MB", "GB"):
        if b < 1024:
            return f"{b:.0f} {unit}" if unit == "B" else f"{b:.1f} {unit}"
        b /= 1024
    return f"{b:.2f} TB"


def lerp_hex(a, b, t):
    ca = [int(a[i:i + 2], 16) for i in (1, 3, 5)]
    cb = [int(b[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x + (y - x) * t):02x}" for x, y in zip(ca, cb))


def gauge_color(p):
    if p < 50:
        return OK
    if p < 75:
        return lerp_hex(OK, WARN, (p - 50) / 25)
    if p < 90:
        return lerp_hex(WARN, DANGER, (p - 75) / 15)
    return DANGER


def ease_out(t):
    return 1 - (1 - t) ** 3


def count_up(widget, setter, start, end, ms=800):
    t0 = time.time()

    def step():
        if not widget.winfo_exists():
            return
        t = min(1.0, (time.time() - t0) * 1000 / ms)
        setter(start + (end - start) * ease_out(t))
        if t < 1:
            widget.after(16, step)
    step()


def extra_styles():
    s = ttk.Style()
    s.configure("CardOk.TLabel", background=CARD, foreground=OK, font=(FONT, 12, "bold"))
    s.configure("CardBig.TLabel", background=CARD, font=(FONT, 20, "bold"))
    s.configure("Card.TCheckbutton", background=CARD, foreground=FG, indicatorcolor=CARD2)
    s.map("Card.TCheckbutton", background=[("active", CARD)], indicatorcolor=[("selected", ACCENT)])
    s.configure("Thin.Horizontal.TProgressbar", background=ACCENT, troughcolor=CARD2, thickness=5)
    s.configure("Done.Horizontal.TProgressbar", background=OK, troughcolor=CARD2, thickness=5)


# ------------------------------------------------------------------ widgets

class RamGauge(tk.Canvas):
    """270-degree ring gauge that eases to its target value."""

    def __init__(self, parent, size=215):
        super().__init__(parent, width=size, height=size, bg=CARD, highlightthickness=0)
        pad = 20
        box = (pad, pad, size - pad, size - pad)
        self.track = self.create_arc(*box, start=225, extent=-270, style="arc", width=16, outline=CARD2)
        self.bar = self.create_arc(*box, start=225, extent=-1, style="arc", width=16, outline=OK,
                                   state="hidden")
        self.spin = self.create_arc(*box, start=0, extent=-70, style="arc", width=16, outline=ACCENT,
                                    state="hidden")
        c = size / 2
        self.pct = self.create_text(c, c - 8, text="0%", fill=FG, font=(FONT, 34, "bold"))
        self.cap = self.create_text(c, c + 34, text="RAM in use", fill=MUTED, font=(FONT, 10))
        self.value = self.target = 0.0
        self.busy = self._anim = False
        self._ang = 0

    def set_target(self, v):
        self.target = float(v)
        self._kick()

    def set_busy(self, busy):
        self.busy = busy
        self.itemconfigure(self.spin, state="normal" if busy else "hidden")
        self.itemconfigure(self.cap, text="Optimizing…" if busy else "RAM in use")
        self._kick()

    def _kick(self):
        if not self._anim:
            self._anim = True
            self._step()

    def _step(self):
        if not self.winfo_exists():
            return
        d = self.target - self.value
        self.value = self.target if abs(d) < 0.05 else self.value + d * 0.10
        v = self.value
        self.itemconfigure(self.bar, extent=-max(270 * v / 100, 1), outline=gauge_color(v),
                           state="normal" if v >= 0.5 else "hidden")
        self.itemconfigure(self.pct, text=f"{v:.0f}%", fill=gauge_color(v) if v >= 75 else FG)
        if self.busy:
            self._ang = (self._ang + 12) % 360
            self.itemconfigure(self.spin, start=-self._ang)
        if self.busy or abs(self.target - self.value) >= 0.05:
            self.after(16, self._step)
        else:
            self._anim = False


class Sparkline(tk.Canvas):
    def __init__(self, parent, width=250, height=46, points=60):
        super().__init__(parent, width=width, height=height, bg=CARD, highlightthickness=0)
        self.w, self.h, self.hist = width, height, deque(maxlen=points)
        self.maxlen = points

    def push(self, v):
        self.hist.append(v)
        self.delete("all")
        self.create_line(0, self.h - 1, self.w, self.h - 1, fill=CARD2)
        if len(self.hist) < 2:
            return
        step = self.w / (self.maxlen - 1)
        x0 = self.w - step * (len(self.hist) - 1)
        pts = []
        for i, val in enumerate(self.hist):
            pts += [x0 + i * step, self.h - 3 - (self.h - 8) * val / 100]
        self.create_line(*pts, fill=ACCENT, width=2, smooth=True)


# ------------------------------------------------------------------ Optimizer page

class OptimizerPage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        extra_styles()
        self.rows, self.busy, self.ram_busy, self.ticking = [], False, False, False
        self.cats = optimizer.junk_categories()

        head = page_header(self, "Optimizer", "Free up memory and clear junk files in one click")
        head.pack(fill="x")
        body = ttk.Frame(self)
        body.pack(fill="both", expand=True, pady=(16, 0))
        body.columnconfigure(1, weight=1)
        body.rowconfigure(0, weight=1)

        # --- left: memory
        left = ttk.Frame(body, style="Card.TFrame", padding=20)
        left.grid(row=0, column=0, sticky="ns")
        ttk.Label(left, text="Memory", style="CardH2.TLabel").pack(anchor="w")
        self.gauge = RamGauge(left)
        self.gauge.pack(pady=(6, 0))
        self.used_lbl = ttk.Label(left, text="", style="Card.TLabel")
        self.used_lbl.pack()
        self.avail_lbl = ttk.Label(left, text="", style="CardMuted.TLabel")
        self.avail_lbl.pack(pady=(0, 10))
        self.spark = Sparkline(left, width=230, height=40)
        self.spark.pack()
        ttk.Label(left, text="last 60 seconds", style="CardMuted.TLabel").pack(pady=(0, 10))
        self.btn_ram = ttk.Button(left, text="⚡  Boost RAM", style="Accent.TButton", command=self.boost_ram)
        self.btn_ram.pack(fill="x")
        self.ram_result = ttk.Label(left, text="", style="CardOk.TLabel", justify="center")
        self.ram_result.pack(pady=(8, 0))
        self.ram_detail = ttk.Label(left, text="Releases idle memory held by running apps.\nThe effect is temporary.",
                                    style="CardMuted.TLabel", justify="center")
        self.ram_detail.pack(pady=(4, 0))

        # --- right: junk cleaner
        right = ttk.Frame(body, style="Card.TFrame", padding=20)
        right.grid(row=0, column=1, sticky="nsew", padx=(14, 0))
        top = ttk.Frame(right, style="Card.TFrame")
        top.pack(fill="x")
        ttk.Label(top, text="Junk cleaner", style="CardH2.TLabel").pack(side="left")
        self.total_lbl = ttk.Label(top, text="Press Scan to see what can be removed", style="CardMuted.TLabel")
        self.total_lbl.pack(side="right")
        box = ttk.Frame(right, style="Card.TFrame")
        box.pack(fill="both", expand=True, pady=(14, 0))
        for cat in self.cats:
            f = ttk.Frame(box, style="Card.TFrame")
            f.pack(fill="x", pady=(0, 12))
            f.columnconfigure(0, weight=1)
            var = tk.BooleanVar(value=cat["on"])
            ttk.Checkbutton(f, text=cat["title"], variable=var, style="Card.TCheckbutton").grid(row=0, column=0, sticky="w")
            ttk.Label(f, text=cat["desc"], style="CardMuted.TLabel").grid(row=1, column=0, sticky="w", padx=(24, 0))
            size = ttk.Label(f, text="—", style="CardH2.TLabel")
            size.grid(row=0, column=1, rowspan=2, sticky="e")
            pb = ttk.Progressbar(f, style="Thin.Horizontal.TProgressbar", mode="determinate", maximum=100)
            pb.grid(row=2, column=0, columnspan=2, sticky="ew", pady=(8, 0))
            self.rows.append({"cat": cat, "var": var, "size": size, "pb": pb, "bytes": None})
        bar = ttk.Frame(right, style="Card.TFrame")
        bar.pack(fill="x", pady=(6, 0))
        self.btn_scan = ttk.Button(bar, text="🔍  Scan", command=lambda: self.scan())
        self.btn_scan.pack(side="left")
        self.btn_clean = ttk.Button(bar, text="Clean selected", style="Danger.TButton", command=lambda: self.clean())
        self.btn_clean.pack(side="left", padx=8)
        self.btn_all = ttk.Button(bar, text="⚡  Boost everything", style="Accent.TButton", command=self.boost_all)
        self.btn_all.pack(side="right")

    # ---- live memory
    def on_show(self):
        if not self.ticking:
            self.ticking = True
            self.after(80, self._tick)

    def _tick(self):
        if not self.winfo_exists():
            return
        if self.winfo_ismapped():
            self._update_ram()
        self.after(1000, self._tick)           # keep polling cheaply so it resumes after minimize/restore

    def _update_ram(self):
        r = optimizer.ram_info()
        if not self.ram_busy:
            self.gauge.set_target(r["percent"])
        self.used_lbl.configure(text=f"{fmt_size(r['used'])} of {fmt_size(r['total'])} used")
        self.avail_lbl.configure(text=f"{fmt_size(r['available'])} available")
        self.spark.push(r["percent"])

    def boost_ram(self, then=None):
        if self.ram_busy:
            return
        self.ram_busy = True
        self.btn_ram.configure(state="disabled")
        self.btn_all.configure(state="disabled")
        self.ram_result.configure(text="")
        self.ram_detail.configure(text="Optimizing memory…")
        self.gauge.set_busy(True)

        def done(res):
            self.ram_busy = False
            self.gauge.set_busy(False)
            self.gauge.set_target(res["percent"])
            self.btn_ram.configure(state="normal")
            self.btn_all.configure(state="normal")
            note = "" if res["purged"] else ("\nRun as Administrator for a deeper boost" if not core.is_admin() else "")
            freed = res["freed"]
            self.ram_detail.configure(text=f"{res['trimmed']} apps trimmed{note}")
            count_up(self, lambda v: self.ram_result.configure(text=f"Freed {fmt_size(v)}"), 0, freed, 1100)
            self.main.log(f"RAM boost: freed {fmt_size(freed)}, trimmed {res['trimmed']} processes"
                          + (", standby cache purged" if res["purged"] else ""), "ok")
            self.main.toast(f"RAM boosted — {fmt_size(freed)} freed")
            if then:
                then()

        def fail(e):
            self.ram_busy = False
            self.gauge.set_busy(False)
            self.btn_ram.configure(state="normal")
            self.btn_all.configure(state="normal")
        self.main.bg(optimizer.boost_ram, done, fail)

    # ---- junk cleaner
    def _buttons(self, on):
        for b in (self.btn_scan, self.btn_clean, self.btn_all):
            b.configure(state="normal" if on else "disabled")

    def scan(self, then=None):
        if self.busy:
            return
        self.busy = True
        self._buttons(False)
        for r in self.rows:
            r["bytes"] = None
            r["size"].configure(text="…", style="CardH2.TLabel")
            r["pb"].configure(style="Thin.Horizontal.TProgressbar", mode="determinate", value=0)
        self.total_lbl.configure(text="Scanning…")
        self._scan_next(0, then)

    def _scan_next(self, i, then):
        if i >= len(self.rows):
            self.busy = False
            self._buttons(True)
            total = sum(r["bytes"] or 0 for r in self.rows)
            ticked = sum(r["bytes"] or 0 for r in self.rows if r["var"].get())
            count_up(self, lambda v: self.total_lbl.configure(
                text=f"{fmt_size(v)} found · {fmt_size(ticked)} selected" if v >= total else f"{fmt_size(v)} found"),
                0, total, 700)
            if then:
                then()
            return
        row = self.rows[i]
        row["pb"].configure(mode="indeterminate")
        row["pb"].start(12)

        def finish(size):
            row["pb"].stop()
            row["pb"].configure(mode="determinate", value=0)
            row["bytes"] = size
            if size:
                count_up(self, lambda v: row["size"].configure(text=fmt_size(v)), 0, size, 600)
            else:
                row["size"].configure(text="Empty")
            self._scan_next(i + 1, then)

        self.main.bg(lambda: optimizer.scan_junk(row["cat"])[0], finish,
                     lambda e: (row["size"].configure(text="n/a"), finish(0)))

    def clean(self, then=None):
        if self.busy:
            return
        sel = [r for r in self.rows if r["var"].get()]
        if not sel:
            self.main.toast("Tick at least one category first")
            return
        if any(r["bytes"] is None for r in sel):
            self.scan(then=lambda: self.clean(then))
            return
        todo = [r for r in sel if r["bytes"]]
        if not todo:
            self.main.toast("Nothing to clean — you're already tidy")
            if then:
                then()
            return
        names = "\n".join(f"  • {r['cat']['title']} ({fmt_size(r['bytes'])})" for r in todo)
        if not messagebox.askyesno("Clean junk files", f"Permanently delete:\n\n{names}\n\n"
                                   "Files that are in use are skipped. The Recycle Bin cannot be restored."):
            return
        self.busy, self.freed = True, 0
        self._buttons(False)
        self._clean_next(todo, 0, then)

    def _clean_next(self, todo, i, then):
        if i >= len(todo):
            self.busy = False
            self._buttons(True)
            freed = self.freed
            self.total_lbl.configure(text="")
            count_up(self, lambda v: self.total_lbl.configure(
                text=f"✓ Freed {fmt_size(v)}", foreground=OK), 0, freed, 1000)
            self.main.log(f"Junk cleaner freed {fmt_size(freed)}", "ok")
            self.main.toast(f"Cleaned {fmt_size(freed)} of junk")
            if then:
                then()
            return
        row = todo[i]
        prog = optimizer.Progress(row["bytes"])
        flag = {"done": False}
        row["pb"].configure(maximum=max(row["bytes"], 1), value=0)

        def poll():
            if not flag["done"] and self.winfo_exists():
                row["pb"].configure(value=min(prog.done, row["bytes"]))
                self.after(50, poll)
        poll()

        def finish(freed):
            flag["done"] = True
            row["pb"].configure(value=row["pb"]["maximum"], style="Done.Horizontal.TProgressbar")
            row["size"].configure(text=f"Cleaned {fmt_size(freed)}", style="CardOk.TLabel")
            row["bytes"] = 0
            self.freed += freed
            self.main.log(f"{row['cat']['title']}: cleaned {fmt_size(freed)}", "ok")
            self._clean_next(todo, i + 1, then)

        def fail(e):
            flag["done"] = True
            row["size"].configure(text="failed")
            self.main.log(f"{row['cat']['title']}: {e}", "fail")
            self._clean_next(todo, i + 1, then)
        self.main.bg(lambda: optimizer.clean_junk(row["cat"], prog), finish, fail)

    def boost_all(self):
        if any(r["var"].get() for r in self.rows):
            self.clean(then=self.boost_ram)
        else:
            self.boost_ram()


# ------------------------------------------------------------------ Storage analyzer page

SIZES = {"≥ 1 MB": 1, "≥ 10 MB": 10, "≥ 100 MB": 100, "≥ 500 MB": 500, "≥ 1 GB": 1024}
ROOT_FILES = "  (files in this folder)"


class StoragePage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        extra_styles()
        self.parts, self.cards, self.part_keys = [], {}, []
        self.key, self.custom, self.state = "ALL", None, None
        self.scanning, self.view, self._last_refresh, self._shown = False, "files", 0.0, 0.0
        self.iid_path = {}

        head = page_header(self, "Storage analyzer", "Find the heaviest files and folders on any partition")
        head.pack(fill="x")
        self.cards_row = ttk.Frame(self)
        self.cards_row.pack(fill="x", pady=(14, 0))

        tool = ttk.Frame(self)
        tool.pack(fill="x", pady=(14, 6))
        ttk.Label(tool, text="Partition", style="Muted.TLabel").pack(side="left")
        self.part_cb = ttk.Combobox(tool, state="readonly", width=34)
        self.part_cb.pack(side="left", padx=(8, 12))
        self.part_cb.bind("<<ComboboxSelected>>", lambda e: self.select_scope(self.part_keys[self.part_cb.current()]))
        self.btn_scan = ttk.Button(tool, text="▶  Scan", style="Accent.TButton", command=self.toggle_scan)
        self.btn_scan.pack(side="left")
        ttk.Button(tool, text="Choose folder…", command=self.choose_folder).pack(side="left", padx=8)
        ttk.Button(tool, text="↑ Up", command=self.go_up).pack(side="left")
        self.scope_lbl = ttk.Label(tool, text="", style="Muted.TLabel")
        self.scope_lbl.pack(side="left", padx=12)

        flt = ttk.Frame(self)
        flt.pack(fill="x", pady=(0, 8))
        self.type_cb = ttk.Combobox(flt, state="readonly", width=14,
                                    values=["All types"] + list(optimizer.CATEGORIES) + ["Other"])
        self.type_cb.current(0)
        self.type_cb.pack(side="left")
        self.size_cb = ttk.Combobox(flt, state="readonly", width=10, values=list(SIZES))
        self.size_cb.current(1)
        self.size_cb.pack(side="left", padx=8)
        for cb in (self.type_cb, self.size_cb):
            cb.bind("<<ComboboxSelected>>", lambda e: self.refresh_tables())
        self.search = SearchBox(flt, "Filter by name or path…", self.refresh_tables, 28)
        self.search.pack(side="left")
        self.btn_folders = ttk.Button(flt, text="Folders", command=lambda: self.set_view("folders"))
        self.btn_folders.pack(side="right")
        self.btn_files = ttk.Button(flt, text="Files", style="Accent.TButton", command=lambda: self.set_view("files"))
        self.btn_files.pack(side="right", padx=4)

        prog = ttk.Frame(self)
        prog.pack(fill="x", pady=(0, 8))
        self.pb = ttk.Progressbar(prog, style="Thin.Horizontal.TProgressbar", mode="determinate", maximum=100)
        self.pb.pack(fill="x")
        self.status = ttk.Label(prog, text="Pick a partition and press Scan.", style="Muted.TLabel")
        self.status.pack(anchor="w", pady=(4, 0))

        act = ttk.Frame(self)
        act.pack(side="bottom", fill="x", pady=(10, 0))
        holder = ttk.Frame(self)
        holder.pack(fill="both", expand=True)
        self.tbl_files = DataTable(holder, [("size", "Size", 90), ("name", "Name", 230, 1), ("folder", "Folder", 340),
                                            ("type", "Type", 90), ("modified", "Modified", 90)])
        self.tbl_folders = DataTable(holder, [("folder", "Folder", 460, 1), ("size", "Size", 100),
                                              ("share", "Share of scan", 240)])
        for t in (self.tbl_files, self.tbl_folders):
            t.sort_by("size")
            t.sort_by("size")
        self.tbl_files.tree.bind("<Double-1>", lambda e: self.open_location())
        self.tbl_folders.tree.bind("<Double-1>", lambda e: self.drill())
        self.tbl_files.pack(fill="both", expand=True)

        self.act_files = ttk.Frame(act)
        ttk.Button(self.act_files, text="Open location", command=self.open_location).pack(side="left")
        ttk.Button(self.act_files, text="Copy path", command=self.copy_path).pack(side="left", padx=8)
        ttk.Button(self.act_files, text="Move to Recycle Bin", style="Danger.TButton",
                   command=self.delete).pack(side="left")
        self.act_folders = ttk.Frame(act)
        ttk.Button(self.act_folders, text="Scan inside this folder", style="Accent.TButton",
                   command=self.drill).pack(side="left")
        ttk.Button(self.act_folders, text="Open folder", command=self.open_folder).pack(side="left", padx=8)
        self.act_files.pack(side="left")

    # ---- partitions
    def on_show(self):
        self.load_partitions()

    def load_partitions(self):
        # disk queries can stall (sleeping USB disks, slow security software): keep them off the UI thread
        self.main.bg(optimizer.list_partitions, self._partitions_loaded)

    def _partitions_loaded(self, parts):
        if self.scanning:
            return
        self.parts = parts
        for w in self.cards_row.winfo_children():
            w.destroy()
        self.cards, self.part_keys, values = {}, [], []
        tot = sum(p["total"] for p in self.parts) or 1
        used = sum(p["used"] for p in self.parts)
        entries = [("ALL", "All partitions", f"{len(self.parts)} drives · {fmt_size(tot - used)} free of {fmt_size(tot)}",
                    100 * used / tot)]
        for p in self.parts:
            name = p["label"] or ("Removable disk" if p["removable"] else "Local disk")
            entries.append((p["mount"], f"{p['mount']}  {name}",
                            f"{p['fstype']} · {fmt_size(p['free'])} free of {fmt_size(p['total'])}", p["percent"]))
        for key, title, sub, pct in entries:
            c = self._card(self.cards_row, key, title, sub, pct)
            c.pack(side="left", padx=(0, 12))
            self.cards[key] = c
            self.part_keys.append(key)
            values.append(title)
        self.part_cb.configure(values=values)
        if self.key not in self.cards:
            self.key = "ALL"
        self.select_scope(self.key, keep_custom=True)

    def _card(self, parent, key, title, sub, pct):
        f = tk.Frame(parent, bg=CARD, highlightthickness=2, highlightbackground=BORDER, padx=14, pady=10,
                     cursor="hand2")
        t = tk.Label(f, text=title, bg=CARD, fg=FG, font=(FONT, 11, "bold"), anchor="w")
        s = tk.Label(f, text=sub, bg=CARD, fg=MUTED, font=(FONT, 9), anchor="w")
        c = tk.Canvas(f, width=190, height=8, bg=CARD2, highlightthickness=0)
        c.create_rectangle(0, 0, 190 * pct / 100, 8, fill=gauge_color(pct), width=0)
        t.pack(fill="x")
        s.pack(fill="x")
        c.pack(pady=(8, 0), anchor="w")
        for w in (f, t, s, c):
            w.bind("<Button-1>", lambda e, k=key: self.select_scope(k))
        return f

    def select_scope(self, key, keep_custom=False):
        if self.scanning:
            return
        self.key = key
        if not keep_custom:
            self.custom = None
        for k, f in self.cards.items():
            f.configure(highlightbackground=ACCENT if (k == key and not self.custom) else BORDER)
        if key in self.part_keys:
            self.part_cb.current(self.part_keys.index(key))
        self._scope_label()

    def _scope_label(self):
        if self.custom:
            self.scope_lbl.configure(text=f"Folder: {self.custom}")
        else:
            self.scope_lbl.configure(text="Scope: all partitions" if self.key == "ALL" else f"Scope: {self.key}")

    def choose_folder(self):
        if self.scanning:
            return
        p = filedialog.askdirectory(title="Choose a folder to analyze", initialdir=self.custom or self.key
                                    if self.key != "ALL" else "C:\\")
        if p:
            self.custom = os.path.normpath(p)
            for f in self.cards.values():
                f.configure(highlightbackground=BORDER)
            self._scope_label()
            self.start_scan()

    def go_up(self):
        if self.scanning or not self.custom:
            return
        parent = os.path.dirname(self.custom.rstrip("\\"))
        if not parent or parent == self.custom or len(parent.rstrip("\\")) <= 2:
            drive = os.path.splitdrive(self.custom)[0].upper() + "\\"
            self.select_scope(drive if drive in self.cards else "ALL")
        else:
            self.custom = parent
            self._scope_label()
        self.start_scan()

    def drill(self):
        sel = self.tbl_folders.selected()
        if not sel or self.scanning:
            return
        path = self.iid_path.get(sel[0], "")
        if path.endswith(ROOT_FILES):
            return
        self.custom = path
        self._scope_label()
        for f in self.cards.values():
            f.configure(highlightbackground=BORDER)
        self.start_scan()

    # ---- scanning
    def toggle_scan(self):
        if self.scanning:
            self.state.cancel = True
            self.btn_scan.configure(text="Stopping…", state="disabled")
        else:
            self.start_scan()

    def start_scan(self):
        if self.scanning:
            return
        if self.custom:
            roots, total = [self.custom], 0
        else:
            mounts = [p["mount"] for p in self.parts if self.key in ("ALL", p["mount"])]
            roots = mounts
            total = sum(p["used"] for p in self.parts if p["mount"] in mounts)
        if not roots:
            self.main.toast("No partition to scan")
            return
        self.state = optimizer.ScanState(total)
        self.scanning, self._shown, self._last_refresh = True, 0.0, 0.0
        self.btn_scan.configure(text="■  Stop", style="Danger.TButton", state="normal")
        if total:
            self.pb.configure(mode="determinate", value=0)
        else:
            self.pb.configure(mode="indeterminate")
            self.pb.start(12)
        self.tbl_files.clear()
        self.tbl_folders.clear()
        self.iid_path = {}
        st = self.state
        self.main.bg(lambda: optimizer.scan_disk(roots, st), self._scan_done,
                     lambda e: self._scan_done(st))
        self._poll()

    def _poll(self):
        if not self.scanning or not self.winfo_exists():
            return
        st = self.state
        if self.pb["mode"] == "determinate":
            target = min(99.0, 100 * st.bytes / st.total_bytes)
            self._shown += (target - self._shown) * 0.25
            self.pb.configure(value=self._shown)
        cur = st.current if len(st.current) < 70 else "…" + st.current[-68:]
        self.status.configure(text=f"{st.files:,} files · {fmt_size(st.bytes)} scanned · {cur}")
        if time.time() - self._last_refresh > 1.5:
            self._last_refresh = time.time()
            self.refresh_tables()
        self.after(150, self._poll)

    def _scan_done(self, st):
        self.scanning = False
        self.pb.stop()
        self.pb.configure(mode="determinate", value=0 if st.cancel else 100)
        self.btn_scan.configure(text="▶  Scan", style="Accent.TButton", state="normal")
        verb = "Stopped" if st.cancel else "Finished"
        self.status.configure(text=f"{verb}: {st.files:,} files, {fmt_size(st.bytes)} analysed"
                                   f" ({st.errors:,} protected folders skipped)")
        self.refresh_tables()
        self.main.log(f"Storage scan {verb.lower()}: {st.files:,} files, {fmt_size(st.bytes)}", "ok")

    # ---- results
    def set_view(self, view):
        self.view = view
        self.tbl_files.pack_forget()
        self.tbl_folders.pack_forget()
        self.act_files.pack_forget()
        self.act_folders.pack_forget()
        if view == "files":
            self.tbl_files.pack(fill="both", expand=True)
            self.act_files.pack(side="left")
        else:
            self.tbl_folders.pack(fill="both", expand=True)
            self.act_folders.pack(side="left")
        self.btn_files.configure(style="Accent.TButton" if view == "files" else "TButton")
        self.btn_folders.configure(style="Accent.TButton" if view == "folders" else "TButton")

    def _iid(self, path, prefix):
        iid = f"{prefix}{hash(path) & 0xFFFFFFFFFFFF:x}"
        self.iid_path[iid] = path
        return iid

    def _sync(self, table, rows):
        keep = set()
        for iid, values, keys in rows:
            keep.add(iid)
            table.add(iid, values, keys)
        for iid in set(table.tree.get_children()) - keep:
            table.tree.delete(iid)
            table.keys.pop(iid, None)
        table.resort()

    def refresh_tables(self):
        st = self.state
        if not st:
            return
        tmin = SIZES[self.size_cb.get()] * optimizer.MB
        cat, q = self.type_cb.get(), self.search.get().lower()
        rows = []
        for sz, path, mt in sorted(st.heap, reverse=True):
            if sz < tmin:
                break
            c = optimizer.file_category(path)
            if (cat != "All types" and c != cat) or (q and q not in path.lower()):
                continue
            rows.append((self._iid(path, "f"),
                         (fmt_size(sz), os.path.basename(path), os.path.dirname(path), c,
                          time.strftime("%Y-%m-%d", time.localtime(mt))), {"size": sz, "modified": mt}))
            if len(rows) >= 1500:
                break
        self._sync(self.tbl_files, rows)
        total = max(st.bytes, 1)
        frows = []
        for path, sz in sorted(st.folders.items(), key=lambda kv: kv[1], reverse=True)[:500]:
            if q and q not in path.lower():
                continue
            share = sz / total
            n = round(share * 20)
            frows.append((self._iid(path, "d"), (path, fmt_size(sz), "■" * n + f"  {share * 100:.1f}%"),
                          {"size": sz, "share": share}))
        self._sync(self.tbl_folders, frows)

    # ---- file actions
    def _selected_paths(self):
        return [self.iid_path[i] for i in self.tbl_files.selected() if i in self.iid_path]

    def open_location(self):
        p = self._selected_paths()
        if p:
            subprocess.Popen(["explorer", "/select,", p[0]])

    def open_folder(self):
        sel = self.tbl_folders.selected()
        if sel:
            path = self.iid_path.get(sel[0], "").replace(ROOT_FILES, "")
            if os.path.isdir(path):
                os.startfile(path)

    def copy_path(self):
        p = self._selected_paths()
        if p:
            self.clipboard_clear()
            self.clipboard_append("\n".join(p))
            self.main.toast(f"Copied {len(p)} path(s)")

    def delete(self):
        paths = self._selected_paths()
        if not paths:
            self.main.toast("Select one or more files first")
            return
        sizes = {p: s for s, p, _m in self.state.heap}
        total = sum(sizes.get(p, 0) for p in paths)
        if not messagebox.askyesno(
                "Move to Recycle Bin", f"Move {len(paths)} file(s) ({fmt_size(total)}) to the Recycle Bin?\n\n"
                "Files larger than the Recycle Bin allows are deleted permanently by Windows."):
            return

        def work():
            return [(p, *optimizer.delete_to_recycle(p)) for p in paths]

        def done(res):
            gone = {p for p, ok, _m in res if ok}
            for p, ok, msg in res:
                self.main.log(f"{os.path.basename(p)}: {msg}", "ok" if ok else "fail")
            import heapq
            self.state.heap[:] = [x for x in self.state.heap if x[1] not in gone]
            heapq.heapify(self.state.heap)
            freed = sum(sizes.get(p, 0) for p in gone)
            self.main.toast(f"Moved {len(gone)} of {len(res)} file(s) — {fmt_size(freed)} freed")
            self.refresh_tables()
        self.main.bg(work, done)
