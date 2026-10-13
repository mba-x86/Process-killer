# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Process Killer - shared theme and widgets."""
import ctypes
import os
import queue
import subprocess
import sys
import threading
import time
import tkinter as tk
from tkinter import font as tkfont
from tkinter import messagebox, ttk

import psutil

try:
    from PIL import Image, ImageTk
except ImportError:  # logo falls back to Tk's built-in PNG loader
    Image = ImageTk = None

if os.name != "nt":
    sys.exit("Process Killer only runs on Windows.")

import core

# ------------------------------------------------------------------ theme
BG, SIDE, CARD, CARD2 = "#14171c", "#0e1014", "#1b1f27", "#262b35"
FG, MUTED, BORDER, HEAD = "#e6e8ec", "#8b93a1", "#2a303b", "#171b22"
ACCENT, ACCENT_H, DANGER, DANGER_H = "#4c8dff", "#6aa1ff", "#e5484d", "#f06267"
OK, WARN, SEL = "#3ecf8e", "#f5a524", "#2c4a7c"
FONT = "Segoe UI"

KIND = {"process": "Process", "service": "Service", "startup": "Startup entry",
        "task": "Scheduled task", "folder": "Folder"}
LOG_DIR = os.path.join(os.environ.get("APPDATA", "."), "ProcessKiller")
LOG_FILE = os.path.join(LOG_DIR, "activity.log")


def apply_style(root, sc):
    root.configure(bg=BG)
    for n in ("TkDefaultFont", "TkTextFont", "TkMenuFont", "TkHeadingFont"):
        tkfont.nametofont(n).configure(family=FONT, size=10)
    s = ttk.Style(root)
    s.theme_use("clam")
    s.configure(".", background=BG, foreground=FG, fieldbackground=CARD, font=(FONT, 10),
                bordercolor=BORDER, lightcolor=BG, darkcolor=BG, troughcolor=CARD, focuscolor=BG,
                borderwidth=0)
    s.configure("TFrame", background=BG)
    s.configure("Card.TFrame", background=CARD)
    s.configure("Side.TFrame", background=SIDE)
    s.configure("Banner.TFrame", background="#3a2c0b")
    s.configure("TLabel", background=BG, foreground=FG)
    s.configure("Card.TLabel", background=CARD)
    s.configure("Side.TLabel", background=SIDE)
    s.configure("Muted.TLabel", foreground=MUTED)
    s.configure("CardMuted.TLabel", background=CARD, foreground=MUTED)
    s.configure("SideMuted.TLabel", background=SIDE, foreground=MUTED)
    s.configure("Title.TLabel", font=(FONT, 20, "bold"))
    s.configure("H2.TLabel", font=(FONT, 12, "bold"))
    s.configure("CardH2.TLabel", background=CARD, font=(FONT, 12, "bold"))
    s.configure("Logo.TLabel", background=SIDE, font=(FONT, 14, "bold"))
    s.configure("Banner.TLabel", background="#3a2c0b", foreground="#ffd479")
    s.configure("Update.TFrame", background="#12305e")
    s.configure("Update.TLabel", background="#12305e", foreground="#cfe1ff", font=(FONT, 10, "bold"))
    s.configure("Big.TLabel", font=(FONT, 15, "bold"))
    s.configure("Ok.TLabel", foreground=OK)
    s.configure("Warn.TLabel", foreground=WARN)

    s.configure("Treeview", background=CARD, fieldbackground=CARD, foreground=FG,
                rowheight=int(32 * sc), borderwidth=0, relief="flat")
    s.map("Treeview", background=[("selected", SEL)], foreground=[("selected", "#ffffff")])
    s.configure("Treeview.Heading", background=HEAD, foreground=MUTED, relief="flat",
                padding=(10, 8), font=(FONT, 9, "bold"))
    s.map("Treeview.Heading", background=[("active", BORDER)])
    s.layout("Treeview", [("Treeview.treearea", {"sticky": "nswe"})])

    def btn(name, bg, hover, fg=FG):
        s.configure(name, background=bg, foreground=fg, padding=(14, 8), relief="flat")
        s.map(name, background=[("disabled", CARD), ("active", hover)],
              foreground=[("disabled", "#5b6270")])
    btn("TButton", CARD2, BORDER)
    btn("Accent.TButton", ACCENT, ACCENT_H, "#ffffff")
    btn("Danger.TButton", DANGER, DANGER_H, "#ffffff")
    btn("Ghost.TButton", BG, CARD2, MUTED)
    s.configure("Nav.TButton", background=SIDE, foreground=MUTED, anchor="w", padding=(18, 11),
                font=(FONT, 10))
    s.map("Nav.TButton", background=[("active", CARD)], foreground=[("active", FG)])
    s.configure("NavActive.TButton", background=CARD, foreground=FG, anchor="w", padding=(18, 11),
                font=(FONT, 10, "bold"))
    s.map("NavActive.TButton", background=[("active", CARD)])

    s.configure("TEntry", fieldbackground=CARD, foreground=FG, insertcolor=FG, padding=8,
                bordercolor=BORDER)
    s.map("TEntry", bordercolor=[("focus", ACCENT)])
    s.configure("TCombobox", fieldbackground=CARD, background=CARD2, foreground=FG, padding=6,
                arrowcolor=MUTED, bordercolor=BORDER)
    s.map("TCombobox", fieldbackground=[("readonly", CARD)], foreground=[("readonly", FG)])
    root.option_add("*TCombobox*Listbox.background", CARD)
    root.option_add("*TCombobox*Listbox.foreground", FG)
    root.option_add("*TCombobox*Listbox.selectBackground", SEL)
    s.configure("Vertical.TScrollbar", background=CARD2, troughcolor=BG, arrowcolor=MUTED,
                bordercolor=BG, relief="flat")
    s.map("Vertical.TScrollbar", background=[("active", BORDER)])
    s.layout("Vertical.TScrollbar", [("Vertical.Scrollbar.trough", {"sticky": "ns", "children": [
        ("Vertical.Scrollbar.thumb", {"expand": "1", "sticky": "nswe"})]})])
    s.configure("TProgressbar", background=ACCENT, troughcolor=CARD, thickness=6)
    s.configure("TCheckbutton", background=BG, foreground=FG, indicatorcolor=CARD2)
    s.map("TCheckbutton", background=[("active", BG)], indicatorcolor=[("selected", ACCENT)])
    s.configure("TSeparator", background=BORDER)


_BASE = getattr(sys, "_MEIPASS", None) or os.path.dirname(os.path.abspath(__file__))  # PyInstaller-aware
ASSETS = os.path.join(_BASE, "assets")
LOGO_PNG = os.path.join(ASSETS, "Process Killer Logo.png")
LOGO_ICO = os.path.join(ASSETS, "Process Killer Logo.ico")


def logo_image(size):
    """Logo scaled to size x size (None if the file is missing)."""
    if not os.path.exists(LOGO_PNG):
        return None
    try:
        if Image:
            im = Image.open(LOGO_PNG).convert("RGBA").resize((size, size), Image.LANCZOS)
            return ImageTk.PhotoImage(im)
        img = tk.PhotoImage(file=LOGO_PNG)
        return img.subsample(max(1, img.width() // size))
    except Exception:  # noqa: BLE001
        return None


def set_window_icon(win):
    """Crisp icon for title bar / taskbar / Alt-Tab, inherited by every dialog."""
    win._icons = [i for i in (logo_image(n) for n in (256, 64, 32, 16)) if i]
    try:
        if win._icons:
            win.iconphoto(True, *win._icons)
        elif os.path.exists(LOGO_ICO):
            win.iconbitmap(LOGO_ICO)
    except tk.TclError:
        pass


def dark_titlebar(win):
    try:
        win.update_idletasks()
        hwnd = ctypes.windll.user32.GetParent(win.winfo_id())
        v = ctypes.c_int(1)
        ctypes.windll.dwmapi.DwmSetWindowAttribute(hwnd, 20, ctypes.byref(v), 4)
    except Exception:  # noqa: BLE001
        pass


def center(win, parent, w, h):
    parent.update_idletasks()
    x = parent.winfo_rootx() + (parent.winfo_width() - w) // 2
    y = parent.winfo_rooty() + (parent.winfo_height() - h) // 2
    win.geometry(f"{w}x{h}+{max(x, 0)}+{max(y, 0)}")


# ------------------------------------------------------------------ reusable widgets

class SearchBox(ttk.Frame):
    def __init__(self, parent, placeholder, command, width=34):
        super().__init__(parent)
        self.var = tk.StringVar()
        self.command, self._job, self.ph, self.showing_ph = command, None, placeholder, False
        self.entry = ttk.Entry(self, textvariable=self.var, width=width)
        self.entry.pack(fill="x")
        self.entry.bind("<FocusIn>", self._in)
        self.entry.bind("<FocusOut>", self._out)
        self._out()
        self.var.trace_add("write", self._changed)

    def _in(self, _e=None):
        if self.showing_ph:
            self.showing_ph = False
            self.entry.delete(0, "end")
            self.entry.configure(foreground=FG)

    def _out(self, _e=None):
        if not self.var.get() and not self.showing_ph:
            self.showing_ph = True
            self.entry.configure(foreground=MUTED)
            self.entry.insert(0, self.ph)

    def _changed(self, *_):
        if self._job:
            self.after_cancel(self._job)
        self._job = self.after(180, self.command)

    def get(self):
        return "" if self.showing_ph else self.var.get().strip()


class DataTable(ttk.Frame):
    """Treeview with scrollbar and click-to-sort headings."""

    def __init__(self, parent, columns, selectmode="extended"):
        super().__init__(parent, style="Card.TFrame")
        self.cols = [c[0] for c in columns]
        self.titles = {c[0]: c[1] for c in columns}
        self.tree = ttk.Treeview(self, columns=self.cols, show="headings", selectmode=selectmode)
        sb = ttk.Scrollbar(self, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=sb.set)
        for cid, title, width, *rest in columns:
            self.tree.heading(cid, text=title, anchor="w", command=lambda c=cid: self.sort_by(c))
            self.tree.column(cid, width=width, minwidth=50, anchor="w",
                             stretch=bool(rest and rest[0]))
        self.tree.pack(side="left", fill="both", expand=True)
        sb.pack(side="right", fill="y")
        self.keys, self.sort_col, self.sort_rev = {}, None, False

    def clear(self):
        self.tree.delete(*self.tree.get_children())
        self.keys.clear()

    def add(self, iid, values, keys=None, tags=()):
        if self.tree.exists(iid):
            self.tree.item(iid, values=values, tags=tags)
        else:
            self.tree.insert("", "end", iid=iid, values=values, tags=tags)
        self.keys[iid] = keys or {}

    def sort_by(self, col, toggle=True):
        if toggle:
            self.sort_rev = (not self.sort_rev) if self.sort_col == col else False
        self.sort_col = col
        idx = self.cols.index(col)

        def key(iid):
            k = self.keys.get(iid, {}).get(col)
            if k is None:
                k = str(self.tree.item(iid, "values")[idx]).lower()
            return (0, k) if isinstance(k, (int, float)) else (1, k)
        for i, iid in enumerate(sorted(self.tree.get_children(), key=key, reverse=self.sort_rev)):
            self.tree.move(iid, "", i)
        for c in self.cols:
            arrow = (" ▼" if self.sort_rev else " ▲") if c == col else ""
            self.tree.heading(c, text=self.titles[c] + arrow)

    def resort(self):
        if self.sort_col:
            self.sort_by(self.sort_col, toggle=False)

    def selected(self):
        return list(self.tree.selection())


class CheckList(ttk.Frame):
    """List of Related items with tick boxes, an Action column and a match-confidence column."""

    def __init__(self, parent, labels, height=10):
        super().__init__(parent, style="Card.TFrame")
        self.labels, self.items, self.on = labels, {}, set()
        self.tree = ttk.Treeview(self, columns=("type", "action", "match"), height=height,
                                 selectmode="browse")
        self.tree.heading("#0", text="Item", anchor="w")
        self.tree.heading("type", text="Type", anchor="w")
        self.tree.heading("action", text="What will happen", anchor="w")
        self.tree.heading("match", text="Match", anchor="w")
        self.tree.column("#0", width=330)
        self.tree.column("type", width=100)
        self.tree.column("action", width=130)
        self.tree.column("match", width=100)
        self.tree.tag_configure("prot", foreground="#6b7280")
        sb = ttk.Scrollbar(self, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=sb.set)
        self.tree.pack(side="left", fill="both", expand=True)
        sb.pack(side="right", fill="y")
        self.tree.bind("<Button-1>", self._click)
        self.tree.bind("<space>", lambda e: self._toggle(self.tree.focus()))

    def _text(self, iid):
        r = self.items[iid]
        if r.protected:
            return f"  —   {r.name}"
        return f"  {'☑' if iid in self.on else '☐'}   {r.name}"

    def set_items(self, items, default_on):
        self.tree.delete(*self.tree.get_children())
        self.items, self.on = {}, set()
        for n, r in enumerate(items):
            iid = f"r{n}"
            self.items[iid] = r
            if not r.protected and default_on(r):
                self.on.add(iid)
            match = "Protected" if r.protected else ("Install path" if r.reason == "path" else "Name only")
            action = "Not allowed" if r.protected else self.labels.get(r.kind, "")
            self.tree.insert("", "end", iid=iid, text=self._text(iid),
                             values=(KIND.get(r.kind, r.kind), action, match),
                             tags=("prot",) if r.protected else ())

    def _toggle(self, iid):
        if not iid or iid not in self.items or self.items[iid].protected:
            return
        self.on.symmetric_difference_update({iid})
        self.tree.item(iid, text=self._text(iid))

    def _click(self, ev):
        iid = self.tree.identify_row(ev.y)
        if iid and self.tree.identify_column(ev.x) == "#0":
            self._toggle(iid)

    def set_all(self, on):
        self.on = {i for i, r in self.items.items() if on and not r.protected}
        for iid in self.items:
            self.tree.item(iid, text=self._text(iid))

    def checked(self):
        return [self.items[i] for i in self.items if i in self.on]


def page_header(parent, title, subtitle=""):
    f = ttk.Frame(parent)
    ttk.Label(f, text=title, style="Title.TLabel").pack(anchor="w")
    sub = ttk.Label(f, text=subtitle, style="Muted.TLabel")
    sub.pack(anchor="w", pady=(2, 0))
    f.sub = sub
    return f


class Page(ttk.Frame):
    def __init__(self, parent, main):
        super().__init__(parent, padding=(28, 22, 28, 12))
        self.main = main

    def on_show(self):
        pass
