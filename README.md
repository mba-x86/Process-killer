# Process Killer

A Windows desktop app to find, stop, uninstall and clean up software, plus a system dashboard, RAM booster,
junk cleaner and disk-space analyzer.

- **Dashboard** - live CPU / RAM / disk / network, health check, full system specs (Secure Boot, UEFI, TPM, DirectX...)
- **Applications** - uninstall wizard that stops related processes/services first, then removes leftovers
- **Services / Processes / Startup & tasks** - manage everything running on the PC
- **Optimizer** - boost RAM, clear temp files, caches and the Recycle Bin
- **Storage analyzer** - heaviest files and folders per partition
- **Auto-update** - installed copies update themselves from GitHub Releases

## Install
Download `ProcessKiller-Setup-x.y.z.exe` from the [latest release](../../releases/latest) and run it.

## Run from source
```
py -m pip install -r requirements.txt
py app.py
```

## Build & release
See [RELEASING.md](RELEASING.md).
