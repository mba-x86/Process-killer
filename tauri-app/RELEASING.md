# Releasing Process Killer 2.x (Tauri)

The 1.x line (Python, repo root) keeps shipping from `main` until 2.0 is promoted. Existing 1.x users update to 2.0 through their
built-in updater: it reads `latest.json` from the repo's **latest** GitHub release, downloads the installer, checks the SHA-256
and runs it silently with `/RELAUNCH=1`. The 2.x installer (`installer/ProcessKiller.iss`) keeps the same `AppId`, exe name and relaunch entry,
removes the old Python runtime (`{app}\_internal`) and installs WebView2 if it is missing - so the upgrade happens in place.

## Every release
1. Bump the version in **both** `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` (they must match; `release.py` checks).
2. Put the "what's new" text in `release_notes.md` (repo root) or pass `--notes "..."`.
3. `py tauri-app/tools/test_upgrade.py` - proves a 1.x install upgrades in place (throw-away app id, touches nothing real).
4. `py tauri-app/tools/release.py --publish` - builds the exe (admin manifest), the installer and `latest.json`, and creates a GitHub
   **PRE-RELEASE**. Pre-releases are ignored by `releases/latest`, so **nobody is offered the update yet**.
5. Install `dist/ProcessKiller-Setup-<v>.exe` by hand on a clean PC and on an old 1.x PC; check it runs, updates and relaunches.
6. `py tauri-app/tools/release.py --promote` - marks it as the latest release: installed apps (1.x and 2.x) now offer the update.
   Rolling back = publish a fixed version (or re-promote the previous release with `gh release edit vX --latest`).

## Checks before promoting
- `node tauri-app/tools/e2e.mjs <exe>` - real backend vs UI contract (build with `PK_NO_ADMIN_MANIFEST=1` for a prompt-free test exe; never ship that one).
- `py tauri-app/tools/smoke.py` - UI interaction suite (mock backend).
- `py tauri-app/tools/measure.py <exe>` - launch time / memory / idle CPU.

## Signing (recommended once there are many users)
Updates are verified with SHA-256 always. To also verify a signature: `cargo install rsign2`, `rsign generate -p keys/pk.pub -s <secret path>`,
put the public key into `UPDATE_PUBLIC_KEY` (src-tauri/src/updates.rs), build with `release.py --sign <secret path>` (adds `signature` to `latest.json`).
Keep the secret key OUTSIDE the repo. A code-signing certificate for the installer (removes the SmartScreen 'unknown publisher' warning) is separate and optional.

## Rules that keep updates working
- Never change `AppId` in `installer/ProcessKiller.iss`; never rename `ProcessKiller.exe`.
- A release must contain `latest.json` AND the installer, and be the repo's latest (non-pre-release) release to reach users.
- Settings/log files in `%APPDATA%\ProcessKiller` are shared with 1.x; keep their formats compatible.
