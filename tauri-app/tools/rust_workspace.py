# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Private Rust workspaces so parallel agents never break each other's builds.

    py tauri-app/tools/rust_workspace.py <agent>                       create/refresh tauri-app/.cache/rust-work/<agent>/src-tauri
    py tauri-app/tools/rust_workspace.py <agent> --publish src/apps.rs src/apps/mod.rs ...   copy YOUR files back to tauri-app/src-tauri/

`create` copies src-tauri (without target/) and adds a dummy ../ui so tauri-build is happy. It is safe to run again: it only
re-copies the ORCHESTRATOR-owned files (lib.rs, shell.rs, util.rs, protect.rs, jobs.rs, state.rs, activity.rs, main.rs, build.rs,
tauri.conf.json, capabilities, icons) and never overwrites your own module files or Cargo.toml you edited (use --reset to wipe).
`--publish` only accepts paths under src/ and refuses orchestrator-owned files.
"""
import os, shutil, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))     # tauri-app
SRC = os.path.join(ROOT, "src-tauri")
WORK = os.path.join(ROOT, ".cache", "rust-work")
OWNED = {"lib.rs", "main.rs", "shell.rs", "util.rs", "protect.rs", "jobs.rs", "state.rs", "activity.rs"}


def create(agent, reset):
    dst = os.path.join(WORK, agent, "src-tauri")
    if reset and os.path.isdir(os.path.dirname(dst)):
        shutil.rmtree(os.path.dirname(dst))
    first = not os.path.isdir(dst)
    if first:
        shutil.copytree(SRC, dst, ignore=shutil.ignore_patterns("target", "gen"))
    else:                                               # refresh orchestrator-owned files only
        for f in OWNED:
            shutil.copy(os.path.join(SRC, "src", f), os.path.join(dst, "src", f))
        for rel in ("build.rs", "tauri.conf.json", "app.manifest"):
            shutil.copy(os.path.join(SRC, rel), os.path.join(dst, rel))
    ui = os.path.join(WORK, agent, "ui")
    os.makedirs(ui, exist_ok=True)
    open(os.path.join(ui, "index.html"), "w").write("<!doctype html><title>dummy</title>")
    print(("created " if first else "refreshed ") + dst)


def publish(agent, files):
    src_root = os.path.join(WORK, agent, "src-tauri")
    for rel in files:
        rel = rel.replace("\\", "/")
        if not rel.startswith("src/") or ".." in rel:
            sys.exit(f"refusing {rel}: only files under src/ may be published")
        if rel.count("/") == 1 and os.path.basename(rel) in OWNED:
            sys.exit(f"refusing {rel}: orchestrator-owned")
        a, b = os.path.join(src_root, rel), os.path.join(SRC, rel)
        if os.path.isdir(a):
            shutil.copytree(a, b, dirs_exist_ok=True)
        elif os.path.exists(a):
            os.makedirs(os.path.dirname(b), exist_ok=True)
            shutil.copy(a, b)
        else:
            sys.exit(f"missing {a}")
        print("published", rel)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    agent = sys.argv[1]
    if "--publish" in sys.argv:
        publish(agent, sys.argv[sys.argv.index("--publish") + 1:])
    else:
        create(agent, "--reset" in sys.argv)
