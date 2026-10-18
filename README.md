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

## License
MIT © 2026 mba-x86 - see [LICENSE](LICENSE).

## Build & release
See [RELEASING.md](RELEASING.md).

## Removing Microsoft Edge
Edge is built into Windows, so Windows hides its Uninstall button. Process Killer lists it and runs Edge's own
uninstaller (needs Administrator). It refuses to run unless another browser is installed, never removes the
Edge WebView2 Runtime (other apps need it), and can optionally set the Edge Update policy
`HKLM\SOFTWARE\Policies\Microsoft\EdgeUpdate\Install{56EB18F8-B008-4CBD-B6D2-8C97FE7E9062} = 0`
so Windows doesn't reinstall it. To undo that, delete that value (or set it to 1) and reinstall Edge from microsoft.com/edge.
