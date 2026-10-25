# Task Reaper

A Windows utility (formerly **Process Killer**) to find and stop processes and services, uninstall apps and clean up what they leave
behind, analyse storage (largest files, folders and duplicate files), free RAM, and see live hardware stats.

Built with Tauri 2: a Rust backend and a plain HTML/CSS/JS interface, no bundler. The installer is about 5 MB.

## Install
Download the latest `TaskReaper-Setup-x.y.z.exe` from the [Releases](https://github.com/mba-x86/Process-killer/releases) page.
The app updates itself from there (Settings: *About & updates*).

## Develop
```
py tools/smoke.py                 # UI interaction suite (mock backend, headless Chrome)
cd src-tauri && cargo test --lib  # Rust unit tests (set PK_NO_ADMIN_MANIFEST=1)
py tools/make_icons.py            # regenerate the icons from the logo in ui/js/icons.js
py tools/release.py               # build the exe + installer + update manifest into dist/
```
Release steps: see [RELEASING.md](RELEASING.md). The 1.x Python version is kept at the git tag `v1-final`.

## License
MIT, © 2026 mba-x86. See [LICENSE](LICENSE).
