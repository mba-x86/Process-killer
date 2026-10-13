# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Process Killer - About & updates page."""
import os
import tkinter as tk
from tkinter import ttk

import updater
from ui_kit import CARD, FG, FONT, LOG_DIR, MUTED, Page, logo_image, page_header
from version import APP_NAME, COPYRIGHT, LICENSE_NAME, REPO_URL, __version__


class AboutPage(Page):
    def __init__(self, parent, main):
        super().__init__(parent, main)
        head = page_header(self, "About & updates", f"{APP_NAME} · version {__version__}")
        head.pack(fill="x")

        card = ttk.Frame(self, style="Card.TFrame", padding=24)
        card.pack(fill="x", pady=(16, 0))
        self.logo = logo_image(110)
        ttk.Label(card, image=self.logo or "", style="Card.TLabel").pack(side="left", padx=(0, 24))
        info = ttk.Frame(card, style="Card.TFrame")
        info.pack(side="left", fill="x", expand=True)
        ttk.Label(info, text=APP_NAME, style="CardH2.TLabel", font=(FONT, 16, "bold")).pack(anchor="w")
        ttk.Label(info, text=f"Version {__version__}  ·  {COPYRIGHT}  ·  {LICENSE_NAME}",
                  style="CardMuted.TLabel").pack(anchor="w", pady=(2, 10))
        self.status = ttk.Label(info, text="", style="Card.TLabel", wraplength=640, justify="left")
        self.status.pack(anchor="w")
        row = ttk.Frame(info, style="Card.TFrame")
        row.pack(anchor="w", pady=(12, 0))
        self.btn = ttk.Button(row, text="Check for updates", style="Accent.TButton",
                              command=lambda: self.main.check_updates(manual=True))
        self.btn.pack(side="left")
        self.btn_install = ttk.Button(row, text="Install update", command=self.main.start_update)
        self.auto = tk.BooleanVar(value=updater.auto_check_enabled())
        ttk.Checkbutton(row, text="Check automatically at startup", variable=self.auto, style="Card.TCheckbutton",
                        command=lambda: updater.save_settings(auto_check=self.auto.get())).pack(side="left", padx=16)

        notes = ttk.Frame(self, style="Card.TFrame", padding=20)
        notes.pack(fill="both", expand=True, pady=(14, 0))
        ttk.Label(notes, text="What's new", style="CardH2.TLabel").pack(anchor="w")
        self.notes = tk.Text(notes, bg=CARD, fg=FG, bd=0, highlightthickness=0, wrap="word", height=6,
                             font=(FONT, 10), state="disabled", padx=0, pady=8)
        self.notes.pack(fill="both", expand=True)

        src = updater.manifest_url()
        ttk.Label(self, text=f"Update source: {src}" if src else
                  "Update source: not configured (set UPDATE_SOURCE in version.py before building a release).",
                  style="Muted.TLabel").pack(anchor="w", pady=(10, 0))
        link = ttk.Frame(self)
        link.pack(anchor="w", pady=(6, 0))
        ttk.Button(link, text="Project on GitHub", style="Ghost.TButton",
                   command=lambda: os.startfile(REPO_URL)).pack(side="left")
        ttk.Button(link, text="Open data folder", style="Ghost.TButton",
                   command=lambda: os.startfile(LOG_DIR) if os.path.isdir(LOG_DIR) else None).pack(side="left")
        self.set_info(None)

    def set_status(self, text, color=FG):
        self.status.configure(text=text, foreground=color)

    def set_info(self, info, message=None):
        """Show the result of an update check."""
        self.notes.configure(state="normal")
        self.notes.delete("1.0", "end")
        self.btn_install.pack_forget()
        if info and info["available"]:
            self.set_status(f"Version {info['version']} is available (you have {info['current']}).")
            self.btn_install.pack(side="left", padx=(8, 0), after=self.btn)
            self.notes.insert("end", info.get("notes") or "No release notes were provided.")
        elif info:
            self.set_status(f"✓ You're on the latest version ({info['current']}).")
            self.notes.insert("end", info.get("notes") or "")
        else:
            self.set_status(message or "Press “Check for updates” to see if a newer version is available.", MUTED)
            self.notes.insert("end", "Release notes appear here when an update is found.")
        self.notes.configure(state="disabled")
