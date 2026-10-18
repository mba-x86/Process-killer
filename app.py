# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Process Killer - desktop UI (Windows).  Run:  pyw app.py"""
import ctypes
import os
import queue
import subprocess
import sys
import threading
import time
import tkinter as tk
from tkinter import messagebox, ttk

import psutil

import core
from ui_kit import *  # noqa: F401,F403  (theme, widgets, helpers)
from ui_kit import LOG_DIR, LOG_FILE, Page  # noqa: F401
import updater
from about import AboutPage
from dashboard import DashboardPage
from pages_extra import OptimizerPage, StoragePage
from version import COPYRIGHT, __version__


# ------------------------------------------------------------------ Applications page

class AppsPage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        self.apps, self.by_iid, self.cur, self.token, self.loaded = [], {}, None, 0, False
        self.related = []

        head = page_header(self, "Applications", "Loading installed applications…")
        head.pack(fill="x")
        self.sub = head.sub

        bar = ttk.Frame(self)
        bar.pack(fill="x", pady=(14, 10))
        self.search = SearchBox(bar, "Search by name or publisher…", self.apply_filter, 40)
        self.search.pack(side="left")
        self.drive = ttk.Combobox(bar, state="readonly", width=12, values=["All drives"])
        self.drive.current(0)
        self.drive.pack(side="left", padx=(10, 0))
        self.drive.bind("<<ComboboxSelected>>", lambda e: self.apply_filter())
        ttk.Button(bar, text="⟳  Refresh", command=self.reload).pack(side="left", padx=10)

        body = ttk.Frame(self)
        body.pack(fill="both", expand=True)
        body.columnconfigure(0, weight=1)
        body.rowconfigure(0, weight=1)
        self.table = DataTable(body, [("name", "Name", 200, 1), ("publisher", "Publisher", 130),
                                      ("version", "Version", 90), ("size", "Size", 70),
                                      ("date", "Installed", 90)], selectmode="browse")
        self.table.grid(row=0, column=0, sticky="nsew")
        self.table.tree.bind("<<TreeviewSelect>>", self._selected)
        self.table.tree.bind("<Double-1>", lambda e: self.uninstall())
        self.table.sort_by("name")

        self.panel = ttk.Frame(body, style="Card.TFrame", padding=18, width=330)
        self.panel.grid(row=0, column=1, sticky="nsew", padx=(14, 0))
        self.panel.grid_propagate(False)
        self._build_panel()

    # ---- detail panel
    def _build_panel(self):
        p = self.panel
        self.empty = ttk.Label(p, style="CardMuted.TLabel", justify="center", wraplength=270,
                               text="Select an application to see what is running in the "
                                    "background and uninstall it safely.")
        self.empty.place(relx=0.5, rely=0.5, anchor="center")
        self.detail = ttk.Frame(p, style="Card.TFrame")
        d = self.detail
        self.d_name = ttk.Label(d, style="CardH2.TLabel", wraplength=290, justify="left")
        self.d_name.pack(anchor="w")
        self.d_meta = ttk.Label(d, style="CardMuted.TLabel", wraplength=290, justify="left")
        self.d_meta.pack(anchor="w", pady=(4, 0))
        self.d_loc = ttk.Label(d, style="CardMuted.TLabel", wraplength=290, justify="left")
        self.d_loc.pack(anchor="w", pady=(2, 10))
        self.d_warn = ttk.Label(d, style="CardWarn.TLabel", wraplength=290, justify="left",
                                text="⚠  The uninstaller file is missing, so Windows can't remove this. "
                                     "Process Killer can remove the dead entry and its leftover files.")
        ttk.Separator(d).pack(fill="x", pady=6)
        ttk.Label(d, text="Running in the background", style="CardH2.TLabel").pack(anchor="w", pady=(6, 4))
        self.d_status = ttk.Label(d, style="CardMuted.TLabel", wraplength=290, justify="left")
        self.d_status.pack(anchor="w")
        box = ttk.Frame(d, style="Card.TFrame")
        box.pack(fill="both", expand=True, pady=(8, 8))
        self.rel = ttk.Treeview(box, columns=("state",), show="tree", height=7,
                                selectmode="none")
        self.rel.column("#0", width=210)
        self.rel.column("state", width=70, anchor="e")
        self.rel.pack(fill="both", expand=True)
        self.btn_uninstall = ttk.Button(d, text="Uninstall…", style="Accent.TButton",
                                        command=self.uninstall)
        self.btn_uninstall.pack(fill="x", pady=(4, 6))
        self.btn_stop = ttk.Button(d, text="End related processes", command=self.stop_related)
        self.btn_stop.pack(fill="x", pady=2)
        self.btn_open = ttk.Button(d, text="Open install folder", command=self.open_folder)
        self.btn_open.pack(fill="x", pady=2)

    def _selected(self, _e=None):
        sel = self.table.selected()
        if not sel:
            return
        self.cur = app = self.by_iid[sel[0]]
        self.empty.place_forget()
        self.detail.pack(fill="both", expand=True)
        self.d_name.configure(text=app.name)
        meta = " · ".join(x for x in (app.publisher, app.version, core.fmt_kb(app.size_kb)) if x)
        self.d_meta.configure(text=meta or "No details published")
        self.d_loc.configure(text=app.location or "Install folder unknown")
        broken = core.uninstaller_missing(app)
        if broken:
            self.d_warn.pack(anchor="w", pady=(0, 8), after=self.d_loc)
        else:
            self.d_warn.pack_forget()
        self.btn_uninstall.configure(text="Remove broken entry…" if broken else "Uninstall…")
        self.btn_open.configure(state="normal" if app.location and os.path.isdir(app.location) else "disabled")
        self.refresh_related()

    def refresh_related(self):
        app, self.token = self.cur, self.token + 1
        tok = self.token
        self.rel.delete(*self.rel.get_children())
        self.d_status.configure(text="Checking processes, services and startup items…")
        self.btn_stop.configure(state="disabled")
        self.main.bg(lambda: core.find_related(app), lambda r: self._related(tok, r))

    def _related(self, tok, items):
        if tok != self.token:
            return
        self.related = items
        for i, r in enumerate(items):
            dot = "●" if r.state.lower() in ("running", "ready") else "○"
            self.rel.insert("", "end", iid=f"x{i}", text=f" {dot} {r.name.split(' (')[0] if r.kind == 'service' else r.name.lstrip(chr(92))}",
                            values=(r.state,))
        running = [r for r in items if r.kind == "process"]
        svc = [r for r in items if r.kind == "service" and r.state.lower() == "running"]
        if not items:
            self.d_status.configure(text="✓ Nothing is running or registered in the background.")
        else:
            bits = []
            if running:
                bits.append(f"{len(running)} process{'es' if len(running) != 1 else ''}")
            if svc:
                bits.append(f"{len(svc)} running service{'s' if len(svc) != 1 else ''}")
            other = len(items) - len(running) - len(svc)
            if other:
                bits.append(f"{other} other item{'s' if other != 1 else ''}")
            self.d_status.configure(text="Found " + ", ".join(bits) + ". The uninstall wizard will "
                                                                    "let you stop them first.")
        self.btn_stop.configure(state="normal" if any(
            r.kind == "process" and r.reason == "path" and not r.protected for r in items) else "disabled")

    # ---- actions
    def stop_related(self):
        targets = [r for r in self.related if r.kind == "process" and r.reason == "path" and not r.protected]
        if not targets or not messagebox.askyesno(
                "End processes", f"End {len(targets)} process(es) running from {self.cur.name}'s "
                                 "install folder?\nUnsaved work in them will be lost."):
            return

        def work():
            return [(r, *core.act_kill(r.data)) for r in targets]

        def done(res):
            for r, ok, msg in res:
                self.main.log(f"{r.name}: {msg}", "ok" if ok else "fail")
            self.main.toast(f"Ended {sum(1 for _, ok, _m in res if ok)} of {len(res)} processes")
            self.refresh_related()
        self.main.bg(work, done)

    def open_folder(self):
        if self.cur and self.cur.location:
            os.startfile(self.cur.location)

    def uninstall(self):
        if self.cur:
            Wizard(self.main, self.cur, self.reload)

    # ---- list
    def on_show(self):
        if not self.loaded:
            self.reload()

    def reload(self):
        self.sub.configure(text="Loading installed applications…")
        self.main.bg(core.list_apps, self._loaded)

    def _loaded(self, apps):
        self.apps, self.loaded = apps, True
        drives = sorted({a.location[:2].upper() for a in apps if len(a.location) > 1 and a.location[1] == ":"})
        cur = self.drive.get()
        self.drive.configure(values=["All drives"] + [d + "\\" for d in drives])
        if cur not in self.drive["values"]:
            self.drive.current(0)
        self.apply_filter()

    def apply_filter(self):
        q = self.search.get().lower()
        keep = self.cur.name if self.cur else None
        self.table.clear()
        self.by_iid = {}
        shown = 0
        for i, a in enumerate(self.apps):
            if q and q not in a.name.lower() and q not in a.publisher.lower():
                continue
            if self.drive.get() != "All drives" and a.location[:2].upper() != self.drive.get()[:2]:
                continue
            iid = f"a{i}"
            self.by_iid[iid] = a
            self.table.add(iid, (a.name, a.publisher, a.version, core.fmt_kb(a.size_kb), a.date),
                           {"size": a.size_kb})
            if a.name == keep:
                self.table.tree.selection_set(iid)
            shown += 1
        self.table.resort()
        self.sub.configure(text=f"{shown} of {len(self.apps)} applications"
                                + (f" matching “{q}”" if q else "")
                                + (f" on {self.drive.get()}" if self.drive.get() != "All drives" else ""))


# ------------------------------------------------------------------ Uninstall wizard

class Wizard(tk.Toplevel):
    STEPS = ["Review", "Uninstall", "Clean up", "Finish"]

    def __init__(self, main, app, on_close):
        super().__init__(main)
        self.main, self.app, self.on_close, self.busy = main, app, on_close, False
        self.broken = core.uninstaller_missing(app)      # stale entry: there is no uninstaller to run
        self.title(f"Uninstall {app.name}")
        self.configure(bg=BG)
        self.transient(main)
        self.minsize(720, 540)
        center(self, main, 780, 600)
        dark_titlebar(self)
        self.protocol("WM_DELETE_WINDOW", self._close)

        hdr = ttk.Frame(self, style="Side.TFrame", padding=(24, 16))
        hdr.pack(fill="x")
        self.hdr_logo = logo_image(28)
        ttk.Label(hdr, text="  " + app.name, image=self.hdr_logo or "", compound="left",
                  style="Logo.TLabel").pack(anchor="w")
        row = ttk.Frame(hdr, style="Side.TFrame")
        row.pack(anchor="w", pady=(8, 0))
        self.step_lbls = []
        for i, s in enumerate(self.STEPS):
            lb = ttk.Label(row, text=f"{i + 1}  {s}", style="SideMuted.TLabel")
            lb.pack(side="left", padx=(0, 22))
            self.step_lbls.append(lb)
        self.foot = ttk.Frame(self, padding=(24, 8, 24, 18))
        self.foot.pack(side="bottom", fill="x")
        self.body = ttk.Frame(self, padding=(24, 20, 24, 8))
        self.body.pack(fill="both", expand=True)

        self.update_idletasks()
        try:
            self.wait_visibility()
            self.grab_set()
        except tk.TclError:
            pass
        self.step_scan()

    # ---- scaffolding
    def _close(self):
        if self.busy:
            return
        self.on_close()
        self.destroy()

    def set_step(self, i):
        for n, lb in enumerate(self.step_lbls):
            txt = f"{n + 1}  {self.STEPS[n]}"
            if n < i:
                lb.configure(text=f"✓  {self.STEPS[n]}", foreground=OK)
            elif n == i:
                lb.configure(text=txt, foreground=ACCENT, font=(FONT, 10, "bold"))
            else:
                lb.configure(text=txt, foreground=MUTED, font=(FONT, 10))

    def clear(self):
        for w in self.body.winfo_children():
            w.destroy()

    def buttons(self, left=(), right=()):
        for w in self.foot.winfo_children():
            w.destroy()
        for text, style, cmd in left:
            ttk.Button(self.foot, text=text, style=style, command=cmd).pack(side="left", padx=(0, 8))
        for text, style, cmd in reversed(right):
            ttk.Button(self.foot, text=text, style=style, command=cmd).pack(side="right", padx=(8, 0))

    def spinner(self, text):
        self.clear()
        ttk.Label(self.body, text=text, style="Big.TLabel").pack(pady=(80, 14))
        pb = ttk.Progressbar(self.body, mode="indeterminate", length=320)
        pb.pack()
        pb.start(12)

    # ---- step 1: scan + review
    def step_scan(self):
        self.set_step(0)
        self.spinner("Checking what is running…")
        self.buttons(left=[("Cancel", "Ghost.TButton", self._close)])
        self.main.bg(lambda: core.find_related(self.app), self.step_review, lambda e: self._close())

    def step_review(self, items):
        self.clear()
        self.items = items
        if self.broken:
            warn = ttk.Frame(self.body, style="Banner.TFrame", padding=12)
            warn.pack(fill="x", pady=(0, 12))
            ttk.Label(warn, style="Banner.TLabel", wraplength=700, justify="left",
                      text=f"⚠  The uninstaller for {self.app.name} is missing "
                           f"({os.path.basename(core._exe_of(self.app.uninstall))}), so Windows' Uninstall button "
                           "fails. We'll remove the dead entry from “Installed apps” (a backup is saved so it can "
                           "be restored) and then clean up the leftover files.").pack(anchor="w")
        if items:
            ttk.Label(self.body, text="Stop related items before uninstalling?", style="Big.TLabel").pack(anchor="w")
            ttk.Label(self.body, wraplength=720, justify="left", style="Muted.TLabel",
                      text=f"We found {len(items)} item(s) connected to {self.app.name}. Running "
                           "programs can block the uninstaller, so tick the ones to stop first. "
                           "“Install path” matches are certain; “Name only” matches are guesses "
                           "and start unticked.").pack(anchor="w", pady=(4, 12))
            link = ttk.Frame(self.body)
            link.pack(side="bottom", fill="x", pady=(8, 0))
            ttk.Button(link, text="Select all", style="Ghost.TButton",
                       command=lambda: self.checks.set_all(True)).pack(side="left")
            ttk.Button(link, text="Select none", style="Ghost.TButton",
                       command=lambda: self.checks.set_all(False)).pack(side="left")
            self.checks = CheckList(self.body, core.PRE_LABELS, height=6)
            self.checks.pack(fill="both", expand=True)
            self.checks.set_items(items, lambda r: r.reason == "path")
        else:
            self.checks = None
            ttk.Label(self.body, text="✓", style="Ok.TLabel", font=(FONT, 40)).pack(pady=(60, 0))
            ttk.Label(self.body, text="Nothing related is running", style="Big.TLabel").pack()
            ttk.Label(self.body, text="No background processes, services, startup items or tasks "
                                      "were found. You can continue straight away.",
                      style="Muted.TLabel", wraplength=560, justify="center").pack(pady=8)
        self.silent = tk.BooleanVar(value=False)
        if self.app.quiet and not self.broken:
            ttk.Checkbutton(self.body, text="Use the silent uninstaller (no prompts)",
                            variable=self.silent).pack(anchor="w", pady=(10, 0))
        right = []
        verb = "remove entry" if self.broken else "uninstall"
        if items:
            right.append((f"{verb.capitalize()} without stopping", "TButton", lambda: self.step_run([])))
            right.append((f"Stop selected & {verb}", "Accent.TButton",
                          lambda: self.step_run(self.checks.checked())))
        else:
            right.append((verb.capitalize(), "Accent.TButton", lambda: self.step_run([])))
        self.buttons(left=[("Cancel", "Ghost.TButton", self._close)], right=right)

    # ---- step 2: stop + uninstall
    def step_run(self, todo):
        self.set_step(1)
        self.busy = True
        self.clear()
        ttk.Label(self.body, text="Removing entry…" if self.broken else "Uninstalling…",
                  style="Big.TLabel").pack(anchor="w")
        ttk.Label(self.body, style="Muted.TLabel", wraplength=720, justify="left",
                  text="Saving a backup, then removing the dead entry." if self.broken else
                       "Follow any window the uninstaller opens. This page continues by itself "
                       "when it is finished.").pack(anchor="w", pady=(2, 10))
        pb = ttk.Progressbar(self.body, mode="indeterminate")
        pb.pack(fill="x")
        pb.start(12)
        self.logbox = self._logbox(self.body)
        self.buttons()
        silent = self.silent.get()
        say = lambda t, lvl="info": self.main.ui(self.say, t, lvl)  # noqa: E731

        def work():
            for r in todo:
                ok, msg = core.PRE_ACTIONS[r.kind](r.data)
                say(f"{KIND[r.kind]} “{r.name}” — {msg}", "ok" if ok else "fail")
                self.main.log(f"[{self.app.name}] {r.name}: {msg}", "ok" if ok else "fail")
            if self.broken:
                say("Removing the dead entry from Installed apps…")
                ok, msg = core.remove_uninstall_entry(self.app)
            else:
                say("Starting the uninstaller…")
                ok, msg = core.run_uninstall(self.app, silent)
            say(msg, "ok" if ok else "fail")
            self.main.log(f"[{self.app.name}] uninstall: {msg}", "ok" if ok else "fail")
            return ok, msg
        self.main.bg(work, self._ran, lambda e: self._ran((False, str(e))))

    def _ran(self, res):
        self.busy = False
        ok, msg = res
        if ok:
            self.step_cleanup()
            return
        self.clear()
        ttk.Label(self.body, text="✕", font=(FONT, 40), foreground=DANGER).pack(pady=(60, 0))
        ttk.Label(self.body, text="The uninstall did not complete", style="Big.TLabel").pack()
        ttk.Label(self.body, text=msg, style="Muted.TLabel", wraplength=560).pack(pady=8)
        self.buttons(right=[("Close", "TButton", self._close),
                            ("Look for leftovers anyway", "Accent.TButton", self.step_cleanup)])

    def _logbox(self, parent):
        t = tk.Text(parent, height=10, bg=CARD, fg=FG, bd=0, highlightthickness=0, padx=12, pady=10,
                    font=("Consolas", 10), state="disabled", wrap="word")
        t.pack(fill="both", expand=True, pady=(12, 0))
        t.tag_configure("ok", foreground=OK)
        t.tag_configure("fail", foreground=DANGER)
        return t

    def say(self, text, level="info"):
        if not self.logbox.winfo_exists():
            return
        self.logbox.configure(state="normal")
        self.logbox.insert("end", ("✓ " if level == "ok" else "✕ " if level == "fail" else "• ") + text + "\n",
                           level)
        self.logbox.see("end")
        self.logbox.configure(state="disabled")

    # ---- step 3: leftovers
    def step_cleanup(self):
        self.set_step(2)
        self.spinner("Looking for leftovers…")
        self.buttons()
        self.busy = True
        self.main.bg(lambda: core.find_leftovers(self.app), self._leftovers,
                     lambda e: self._leftovers([]))

    def _leftovers(self, items):
        self.busy = False
        if not items:
            self.step_done([], "Nothing was left behind.")
            return
        self.clear()
        ttk.Label(self.body, text="Clean up leftovers", style="Big.TLabel").pack(anchor="w")
        ttk.Label(self.body, wraplength=720, justify="left", style="Muted.TLabel",
                  text="The app is gone, but these related items remain. Folders go to the Recycle "
                       "Bin so you can restore them. Review the list and untick anything you want "
                       "to keep.").pack(anchor="w", pady=(4, 12))
        self.cleanup = CheckList(self.body, core.CLEAN_LABELS, height=6)
        self.cleanup.pack(fill="both", expand=True)
        self.cleanup.set_items(items, lambda r: r.reason == "path")
        self.buttons(left=[("Keep everything", "Ghost.TButton", lambda: self.step_done([], "Leftovers were kept."))],
                     right=[("Clean up selected", "Accent.TButton", self._do_cleanup)])

    def _do_cleanup(self):
        todo = self.cleanup.checked()
        if not todo:
            self.step_done([], "No leftovers were selected.")
            return
        self.busy = True
        self.spinner("Cleaning up…")
        self.buttons()

        def work():
            res = []
            for r in todo:
                ok, msg = core.CLEAN_ACTIONS[r.kind](r.data)
                res.append((r, ok, msg))
                self.main.log(f"[{self.app.name}] {r.name}: {msg}", "ok" if ok else "fail")
            return res
        self.main.bg(work, lambda res: self.step_done(res, None), lambda e: self.step_done([], str(e)))

    # ---- step 4: done
    def step_done(self, results, note):
        self.busy = False
        self.set_step(3)
        for n, lb in enumerate(self.step_lbls):
            lb.configure(text=f"✓  {self.STEPS[n]}", foreground=OK, font=(FONT, 10))
        self.clear()
        bad = [r for r in results if not r[1]]
        ttk.Label(self.body, text="✓", style="Ok.TLabel", font=(FONT, 40)).pack(pady=(30, 0))
        ttk.Label(self.body, text=f"{self.app.name} has been removed", style="Big.TLabel").pack()
        summary = note or f"{len(results) - len(bad)} leftover item(s) cleaned" + (
            f", {len(bad)} could not be removed." if bad else ".")
        ttk.Label(self.body, text=summary, style="Muted.TLabel").pack(pady=6)
        if bad:
            self.logbox = self._logbox(self.body)
            for r, _ok, msg in bad:
                self.say(f"{r.name} — {msg}", "fail")
        self.buttons(right=[("Close", "Accent.TButton", self._close)])


# ------------------------------------------------------------------ Services page

class ServicesPage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        self.rows, self.loaded = {}, False
        head = page_header(self, "Services", "Start, stop and disable Windows services")
        head.pack(fill="x")
        self.sub = head.sub
        bar = ttk.Frame(self)
        bar.pack(fill="x", pady=(14, 10))
        self.search = SearchBox(bar, "Search services…", self.apply_filter, 36)
        self.search.pack(side="left")
        self.flt = ttk.Combobox(bar, state="readonly", width=12, values=["All", "Running", "Stopped"])
        self.flt.current(0)
        self.flt.pack(side="left", padx=10)
        self.flt.bind("<<ComboboxSelected>>", lambda e: self.apply_filter())
        ttk.Button(bar, text="⟳  Refresh", command=self.reload).pack(side="left")

        self.table = DataTable(self, [("display", "Display name", 270, 1), ("name", "Service name", 150),
                                      ("status", "Status", 90), ("start", "Startup", 100),
                                      ("path", "Path", 330)])
        self.table.pack(fill="both", expand=True)
        self.table.tree.tag_configure("run", foreground=OK)
        self.table.tree.tag_configure("stop", foreground=MUTED)
        self.table.sort_by("display")

        act = ttk.Frame(self)
        act.pack(fill="x", pady=(10, 0))
        for text, style, op in [("▶  Start", "TButton", "start"), ("■  Stop", "TButton", "stop"),
                                ("↻  Restart", "TButton", "restart")]:
            ttk.Button(act, text=text, style=style, command=lambda o=op: self.do(o)).pack(side="left", padx=(0, 6))
        ttk.Separator(act, orient="vertical").pack(side="left", fill="y", padx=10)
        ttk.Button(act, text="Enable (automatic)", command=lambda: self.do("enable")).pack(side="left", padx=(0, 6))
        ttk.Button(act, text="Manual", command=lambda: self.do("manual")).pack(side="left", padx=(0, 6))
        ttk.Button(act, text="Disable", style="Danger.TButton", command=lambda: self.do("disable")).pack(side="left")

    def on_show(self):
        if not self.loaded:
            self.reload()

    def reload(self):
        self.sub.configure(text="Loading services…")
        self.main.bg(core.snap_services, self._loaded)

    def _loaded(self, rows):
        self.rows = {r["name"]: r for r in rows}
        self.loaded = True
        self.apply_filter()

    def apply_filter(self):
        q, f = self.search.get().lower(), self.flt.get()
        sel = set(self.table.selected())
        self.table.clear()
        n = 0
        for name, r in self.rows.items():
            st = r["status"].lower()
            if (f == "Running" and st != "running") or (f == "Stopped" and st != "stopped"):
                continue
            if q and q not in name.lower() and q not in r["display"].lower() and q not in r["path"].lower():
                continue
            self.table.add(name, (r["display"], name, r["status"].capitalize(), r["start"].capitalize(),
                                  r["path"]), tags=("run" if st == "running" else "stop",))
            n += 1
        self.table.resort()
        for s in sel:
            if self.table.tree.exists(s):
                self.table.tree.selection_add(s)
        self.sub.configure(text=f"{n} of {len(self.rows)} services")

    def do(self, op):
        names = self.table.selected()
        if not names:
            self.main.toast("Select one or more services first")
            return
        system = [n for n in names if self._is_system(self.rows[n])]
        verb = {"disable": "Disable", "stop": "Stop", "restart": "Restart"}.get(op)
        if verb and system and not messagebox.askyesno(
                "Windows service", f"{len(system)} selected service(s) belong to Windows "
                                   f"(e.g. {self.rows[system[0]]['display']}).\n{verb} anyway? "
                                   "This can affect system stability."):
            return
        if op == "disable" and not system and len(names) > 1 and not messagebox.askyesno(
                "Disable services", f"Stop and disable {len(names)} services?"):
            return
        fn = {"start": core.svc_start, "stop": core.svc_stop, "restart": core.svc_restart,
              "enable": core.svc_enable, "disable": core.svc_disable,
              "manual": lambda d: core.svc_config(d, "demand")}[op]

        def work():
            return [(n, *fn({"name": n})) for n in names]

        def done(res):
            for n, ok, msg in res:
                self.main.log(f"service {self.rows[n]['display']}: {msg}", "ok" if ok else "fail")
            good = sum(1 for _, ok, _m in res if ok)
            self.main.toast(f"{good} of {len(res)} succeeded" + ("" if good == len(res) else " — see Activity"))
            self.reload()
        self.sub.configure(text="Working…")
        self.main.bg(work, done)

    @staticmethod
    def _is_system(r):
        p = r["path"].lower().replace('"', "")
        return p.startswith(core.WIN_DIR + "\\") and "svchost" in p or p.startswith(core.WIN_DIR + "\\system32\\")


# ------------------------------------------------------------------ Processes page

class ProcessesPage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        self.rows, self.pending = {}, False
        head = page_header(self, "Processes", "Everything running right now")
        head.pack(fill="x")
        self.sub = head.sub
        bar = ttk.Frame(self)
        bar.pack(fill="x", pady=(14, 10))
        self.search = SearchBox(bar, "Search processes…", self.apply_filter, 36)
        self.search.pack(side="left")
        self.auto = tk.BooleanVar(value=True)
        ttk.Checkbutton(bar, text="Auto-refresh", variable=self.auto).pack(side="left", padx=14)
        self.table = DataTable(self, [("name", "Name", 220, 1), ("pid", "PID", 70), ("cpu", "CPU %", 70),
                                      ("mem", "Memory", 90), ("path", "Path", 420)])
        self.table.pack(fill="both", expand=True)
        self.table.sort_by("mem")
        self.table.sort_by("mem")  # descending
        act = ttk.Frame(self)
        act.pack(fill="x", pady=(10, 0))
        ttk.Button(act, text="End task", style="Danger.TButton",
                   command=lambda: self.kill(False)).pack(side="left", padx=(0, 6))
        ttk.Button(act, text="End process tree", command=lambda: self.kill(True)).pack(side="left", padx=(0, 6))
        ttk.Button(act, text="Open file location", command=self.locate).pack(side="left")
        self.main.after(4000, self._tick)

    def on_show(self):
        self.refresh()

    def _tick(self):
        if self.auto.get() and self.winfo_ismapped():
            self.refresh()
        self.main.after(4000, self._tick)

    def refresh(self):
        if self.pending:
            return
        self.pending = True

        def work():
            out = []
            for p in psutil.process_iter(["pid", "name", "exe", "memory_info", "cpu_percent"]):
                i = p.info
                mem = i["memory_info"].rss if i["memory_info"] else 0
                out.append((i["pid"], i["name"] or "?", i["exe"] or "", mem,
                            (i["cpu_percent"] or 0) / (psutil.cpu_count() or 1)))
            return out
        self.main.bg(work, self._loaded, lambda e: setattr(self, "pending", False))

    def _loaded(self, rows):
        self.pending = False
        self.rows = {str(r[0]): r for r in rows}
        self.apply_filter()

    def apply_filter(self):
        q = self.search.get().lower()
        keep = set()
        for iid, (pid, name, exe, mem, cpu) in self.rows.items():
            if q and q not in name.lower() and q not in exe.lower() and q != iid:
                continue
            keep.add(iid)
            self.table.add(iid, (name, pid, f"{cpu:.1f}", core.fmt_bytes(mem), exe),
                           {"pid": pid, "cpu": cpu, "mem": mem})
        for iid in set(self.table.tree.get_children()) - keep:
            self.table.tree.delete(iid)
            self.table.keys.pop(iid, None)
        self.table.resort()
        self.sub.configure(text=f"{len(keep)} processes")

    def kill(self, tree):
        sel = self.table.selected()
        if not sel:
            self.main.toast("Select a process first")
            return
        blocked = [s for s in sel if core.is_protected_proc_pid(int(s))]
        sel = [s for s in sel if s not in blocked]
        if blocked:
            self.main.toast(f"{len(blocked)} protected system process(es) skipped")
        if not sel:
            return
        names = ", ".join(self.rows[s][1] for s in sel[:3]) + ("…" if len(sel) > 3 else "")
        if not messagebox.askyesno("End process", f"End {names}?\nUnsaved data in them will be lost."):
            return

        def work():
            return [(s, *core.act_kill({"pid": int(s)}, tree)) for s in sel]

        def done(res):
            for s, ok, msg in res:
                self.main.log(f"{self.rows.get(s, (0, s))[1]} (PID {s}): {msg}", "ok" if ok else "fail")
            self.main.toast(f"Ended {sum(1 for _, ok, _m in res if ok)} of {len(res)}")
            self.refresh()
        self.main.bg(work, done)

    def locate(self):
        sel = self.table.selected()
        if sel and self.rows[sel[0]][2]:
            subprocess.Popen(["explorer", "/select,", self.rows[sel[0]][2]])


# ------------------------------------------------------------------ Startup & tasks page

class StartupPage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        self.entries, self.loaded = {}, False
        head = page_header(self, "Startup & tasks", "Programs that launch by themselves")
        head.pack(fill="x")
        self.sub = head.sub
        bar = ttk.Frame(self)
        bar.pack(fill="x", pady=(14, 10))
        ttk.Button(bar, text="⟳  Refresh", command=self.reload).pack(side="left")
        self.table = DataTable(self, [("type", "Type", 120), ("name", "Name", 250), ("cmd", "Command", 380, 1),
                                      ("where", "Location", 140)])
        self.table.pack(fill="both", expand=True)
        self.table.sort_by("type")
        act = ttk.Frame(self)
        act.pack(fill="x", pady=(10, 0))
        ttk.Button(act, text="Remove selected", style="Danger.TButton", command=self.remove).pack(side="left")
        ttk.Label(act, text="  Microsoft's built-in tasks are hidden.", style="Muted.TLabel").pack(side="left")

    def on_show(self):
        if not self.loaded:
            self.reload()

    def reload(self):
        self.sub.configure(text="Loading…")
        self.main.bg(lambda: (core.list_startup(), core.list_tasks(force=True)), self._loaded)

    def _loaded(self, res):
        startup, tasks = res
        self.loaded = True
        self.entries = {}
        self.table.clear()
        for i, s in enumerate(startup):
            iid = f"s{i}"
            self.entries[iid] = ("startup", s)
            self.table.add(iid, ("Startup entry", s["name"], s["cmd"], s["label"]))
        for i, t in enumerate(tasks):
            iid = f"t{i}"
            self.entries[iid] = ("task", t)
            self.table.add(iid, ("Scheduled task", t["name"], t["cmd"], t["status"]))
        self.table.resort()
        self.sub.configure(text=f"{len(startup)} startup entries · {len(tasks)} scheduled tasks")

    def remove(self):
        sel = self.table.selected()
        if not sel:
            self.main.toast("Select entries to remove first")
            return
        if not messagebox.askyesno("Remove", f"Permanently remove {len(sel)} item(s)?"):
            return
        todo = [self.entries[s] for s in sel]

        def work():
            fn = {"startup": core.act_startup_remove, "task": core.act_task_delete}
            return [(d["name"], *fn[k](d)) for k, d in todo]

        def done(res):
            for n, ok, msg in res:
                self.main.log(f"{n}: {msg}", "ok" if ok else "fail")
            self.main.toast(f"Removed {sum(1 for _, ok, _m in res if ok)} of {len(res)}")
            self.reload()
        self.main.bg(work, done)


# ------------------------------------------------------------------ Activity page

class ActivityPage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        head = page_header(self, "Activity", "Everything this app has done")
        head.pack(fill="x")
        bar = ttk.Frame(self)
        bar.pack(fill="x", pady=(14, 10))
        ttk.Button(bar, text="Open log file", command=self.open_file).pack(side="left")
        ttk.Button(bar, text="Clear view", command=self.clear).pack(side="left", padx=8)
        self.text = tk.Text(self, bg=CARD, fg=FG, bd=0, highlightthickness=0, padx=14, pady=12,
                            font=("Consolas", 10), state="disabled", wrap="word")
        self.text.pack(fill="both", expand=True)
        for tag, col in (("ok", OK), ("fail", DANGER), ("info", FG), ("ts", MUTED)):
            self.text.tag_configure(tag, foreground=col)

    def add(self, msg, level):
        self.text.configure(state="normal")
        self.text.insert("end", time.strftime("%H:%M:%S  "), "ts")
        self.text.insert("end", ("✓ " if level == "ok" else "✕ " if level == "fail" else "") + msg + "\n", level)
        self.text.see("end")
        self.text.configure(state="disabled")

    def clear(self):
        self.text.configure(state="normal")
        self.text.delete("1.0", "end")
        self.text.configure(state="disabled")

    def open_file(self):
        if os.path.exists(LOG_FILE):
            os.startfile(LOG_FILE)
        else:
            self.main.toast("No activity has been logged yet")


# ------------------------------------------------------------------ main window

class Main(tk.Tk):
    def __init__(self):
        _t0 = time.perf_counter()
        super().__init__()
        self.title("Process Killer")
        sc = self.winfo_fpixels("1i") / 96
        apply_style(self, sc)
        self.geometry(f"{int(1240 * sc)}x{int(780 * sc)}")
        self.minsize(int(1020 * sc), int(640 * sc))
        set_window_icon(self)
        dark_titlebar(self)
        self.q, self._toast_job = queue.Queue(), None
        self.after(40, self._pump)

        side = ttk.Frame(self, style="Side.TFrame", width=int(230 * sc))
        side.pack(side="left", fill="y")
        side.pack_propagate(False)
        self.side_logo = logo_image(int(40 * sc))
        ttk.Label(side, text="  Process Killer", image=self.side_logo or "", compound="left",
                  style="Logo.TLabel").pack(anchor="w", padx=18, pady=(22, 20))

        right = ttk.Frame(self)
        right.pack(side="left", fill="both", expand=True)
        self.banner = ttk.Frame(right, style="Banner.TFrame", padding=(28, 8))
        ttk.Label(self.banner, style="Banner.TLabel",
                  text="⚠  Not running as Administrator — stopping services, ending protected "
                       "processes and some uninstalls will fail.").pack(side="left")
        ttk.Button(self.banner, text="Restart as Administrator", style="Accent.TButton",
                   command=self.elevate).pack(side="right")
        if not core.is_admin():
            self.banner.pack(fill="x")
        self.status = ttk.Label(right, text="", style="Muted.TLabel", padding=(28, 6))
        self.status.pack(side="bottom", fill="x")
        holder = ttk.Frame(right)
        holder.pack(fill="both", expand=True)
        self._build_update_banner(right, holder)

        self.pages, self.navs = {}, {}
        spec = [("dashboard", "⌂   Dashboard", DashboardPage), ("apps", "▦   Applications", AppsPage), ("services", "⚙   Services", ServicesPage),
                ("procs", "▤   Processes", ProcessesPage), ("startup", "⏻   Startup & tasks", StartupPage),
                ("optimizer", "⚡   Optimizer", OptimizerPage), ("storage", "◔   Storage analyzer", StoragePage),
                ("activity", "☰   Activity", ActivityPage), ("about", "ⓘ   About & updates", AboutPage)]
        for key, label, cls in spec:
            b = ttk.Button(side, text=label, style="Nav.TButton", command=lambda k=key: self.show(k))
            b.pack(fill="x", padx=10, pady=2)
            self.navs[key] = b
            self.pages[key] = cls(holder, self)
        adm = core.is_admin()
        ttk.Label(side, text=f"Version {__version__}\n{COPYRIGHT}", style="SideMuted.TLabel", justify="left",
                  padding=(22, 0)).pack(side="bottom", anchor="w", pady=(0, 18))
        ttk.Label(side, text=("● Administrator" if adm else "● Limited access"),
                  style="Side.TLabel", foreground=OK if adm else WARN,
                  padding=(22, 0)).pack(side="bottom", anchor="w", pady=(0, 4))
        self.current = None
        self.update_info = None
        self.show("dashboard")
        self.log(f"Started in {time.perf_counter() - _t0:.1f}s · {'Administrator' if adm else 'limited'} · "
                 f"version {__version__}")
        self.after(2500, lambda: self.bg(core.snapshot))     # warm service/task caches: first app click is instant
        if updater.auto_check_enabled() and updater.manifest_url():
            self.after(4000, lambda: self.check_updates(manual=False))

    def show(self, key):
        if self.current:
            self.pages[self.current].pack_forget()
            self.navs[self.current].configure(style="Nav.TButton")
        self.current = key
        self.navs[key].configure(style="NavActive.TButton")
        self.pages[key].pack(fill="both", expand=True)
        self.pages[key].on_show()

    # ---- threading helpers (Tk is only touched from the main thread)
    def bg(self, fn, cb=None, on_err=None):
        name = getattr(fn, "__qualname__", "task").replace("<locals>.", "")

        def run():
            t0 = time.perf_counter()
            try:
                res = fn()
            except Exception as e:  # noqa: BLE001
                # bind the exception now: Python deletes `e` when this block ends, before the UI thread runs
                self.q.put(lambda err=e: self._err(err, on_err))
                return
            took = time.perf_counter() - t0
            if took > 1.5:      # performance diagnostics: shows up on the Activity page and in the log file
                self.q.put(lambda: self.log(f"Slow task: {name} took {took:.1f}s", "info"))
            if cb:
                self.q.put(lambda: cb(res))
        threading.Thread(target=run, daemon=True).start()

    def ui(self, fn, *args):
        self.q.put(lambda: fn(*args))

    def _pump(self):
        try:
            while True:
                self.q.get_nowait()()
        except queue.Empty:
            pass
        self.after(40, self._pump)

    def _err(self, e, on_err):
        self.log(f"Error: {e}", "fail")
        self.toast(f"Something went wrong: {e}")
        if on_err:
            on_err(e)

    # ---- feedback
    def toast(self, msg):
        self.status.configure(text=msg)
        if self._toast_job:
            self.after_cancel(self._toast_job)
        self._toast_job = self.after(7000, lambda: self.status.configure(text=""))

    def log(self, msg, level="info"):
        if threading.current_thread() is not threading.main_thread():   # workers must not touch Tk
            self.q.put(lambda: self.log(msg, level))
            return
        self.pages["activity"].add(msg, level)
        try:
            os.makedirs(LOG_DIR, exist_ok=True)
            with open(LOG_FILE, "a", encoding="utf-8") as f:
                f.write(f"{time.strftime('%Y-%m-%d %H:%M:%S')} [{level}] {msg}\n")
        except OSError:
            pass

    def elevate(self):
        exe = sys.executable
        if getattr(sys, "frozen", False):          # installed .exe
            params = subprocess.list2cmdline(sys.argv[1:])
        else:                                      # running from source
            if exe.lower().endswith("python.exe"):
                exe = exe[:-10] + "pythonw.exe"
            params = subprocess.list2cmdline([os.path.abspath(sys.argv[0])] + sys.argv[1:])
        if ctypes.windll.shell32.ShellExecuteW(None, "runas", exe, params, None, 1) > 32:
            self.destroy()

    # ---- updates
    def _build_update_banner(self, right, holder):
        self.upd = ttk.Frame(right, style="Update.TFrame", padding=(28, 10))
        self._upd_holder = holder
        self.upd_label = ttk.Label(self.upd, style="Update.TLabel", text="")
        self.upd_label.pack(side="left")
        self.upd_pb = ttk.Progressbar(self.upd, mode="determinate", maximum=100, length=220,
                                      style="Thin.Horizontal.TProgressbar")
        self.upd_btns = ttk.Frame(self.upd, style="Update.TFrame")
        self.upd_btns.pack(side="right")
        self.upd_now = ttk.Button(self.upd_btns, text="Update now", style="Accent.TButton", command=self.start_update)
        self.upd_now.pack(side="left")
        ttk.Button(self.upd_btns, text="What's new", command=lambda: self.show("about")).pack(side="left", padx=6)
        ttk.Button(self.upd_btns, text="Later", style="Ghost.TButton",
                   command=lambda: self.upd.pack_forget()).pack(side="left")
        self._updating = False

    def check_updates(self, manual=False):
        about = self.pages["about"]
        if not updater.manifest_url():
            if manual:
                about.set_info(None, "No update source is configured for this build.")
                self.toast("Updates are not configured for this build")
            return
        if manual:
            about.set_status("Checking for updates…", MUTED)

        def work():
            try:
                return ("ok", updater.check())
            except updater.UpdateError as e:
                return ("err", str(e))
            except Exception as e:  # noqa: BLE001
                return ("err", f"Update check failed: {e}")

        def done(res):
            kind, val = res
            if kind == "err":
                self.log(f"Update check: {val}", "fail")
                if manual:
                    about.set_info(None, val)
                    self.toast(val)
                return
            self.update_info = val
            about.set_info(val)
            if val["available"]:
                self.log(f"Update available: {val['version']}", "ok")
                self.upd_label.configure(text=f"↑  Version {val['version']} is available")
                self.upd.pack(fill="x", before=self._upd_holder)
            elif manual:
                self.toast(f"You're up to date (version {val['current']})")
        self.bg(work, done)

    def start_update(self):
        info = self.update_info
        if not info or not info["available"] or self._updating:
            return
        if not messagebox.askyesno("Install update",
                                   f"Download and install version {info['version']}?\n\n"
                                   "The app will close and reopen automatically."):
            return
        self._updating = True
        self.upd.pack(fill="x", before=self._upd_holder)
        self.upd_btns.pack_forget()
        self.upd_pb.pack(side="right")
        self.upd_label.configure(text=f"Downloading version {info['version']}…")

        def progress(done, total):
            pct = 100 * done / total if total else 0
            self.ui(self._upd_progress, pct, done, total)

        def work():
            path = updater.download(info, progress)
            return path

        def finished(path):
            self.upd_pb.configure(value=100)
            self.upd_label.configure(text="Installing… the app will restart in a moment")
            self.log(f"Installing update {info['version']}", "ok")
            updater.run_installer(path, extra=["/RELAUNCH=1"])
            self.after(1200, self.destroy)

        def failed(e):
            self._updating = False
            self.upd_pb.pack_forget()
            self.upd_btns.pack(side="right")
            self.upd_label.configure(text=f"Update failed: {e}")
        self.bg(work, finished, failed)

    def _upd_progress(self, pct, done, total):
        self.upd_pb.configure(value=pct)
        mb = lambda b: f"{b / 1048576:.1f} MB"  # noqa: E731
        self.upd_label.configure(text=f"Downloading… {mb(done)}" + (f" of {mb(total)}" if total else ""))


def acquire_single_instance():
    """True if we are the only instance (waits briefly when relaunching after an update/elevation)."""
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    for _ in range(12):
        h = k32.CreateMutexW(None, False, "ProcessKillerAppMutex")
        if ctypes.get_last_error() != 183:     # ERROR_ALREADY_EXISTS
            acquire_single_instance.handle = h
            return True
        k32.CloseHandle(h)
        time.sleep(0.5)
    return False


def main():
    try:  # own taskbar identity so Windows shows our icon, not Python's
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID("ProcessKiller.App")
    except Exception:  # noqa: BLE001
        pass
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(1)
    except Exception:  # noqa: BLE001
        pass
    if not acquire_single_instance():
        ctypes.windll.user32.MessageBoxW(None, "Process Killer is already running.", "Process Killer", 0x40)
        return
    Main().mainloop()


if __name__ == "__main__":
    main()
