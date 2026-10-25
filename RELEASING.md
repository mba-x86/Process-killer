# Releasing Task Reaper

Installed apps update themselves: they read `latest.json` from the repo's **latest** GitHub release, download the installer, check
the SHA-256 and the minisign signature (public key compiled into the app), and run it silently with `/RELAUNCH=1`.
The installer (`installer/ProcessKiller.iss`) must keep the same `AppId` and the exe name `ProcessKiller.exe`, so every update
installs in place and taskbar pins keep working. The signing key lives outside the repo: `%USERPROFILE%\.processkiller\update.key`
(back it up; never commit it).

## Every release
1. Bump the version in **both** `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` (`release.py` checks they match).
2. `py tools/smoke.py` and `cargo test --lib` (in `src-tauri`, with `PK_NO_ADMIN_MANIFEST=1`).
3. `py tools/release.py --notes "New: ...\nFixed: ..."` - builds the exe (admin manifest), the installer, the signature and
   `dist/latest.json`. Test `dist/TaskReaper-Setup-x.y.z.exe` by hand.
4. `py tools/test_upgrade.py` - installs the previous 2.0.0 installer in a sandbox (throw-away app id, test-named shortcuts and
   data folder) and upgrades it with the new one.
5. `git push origin <branch>`, then `py tools/release.py --skip-build --publish --notes "..."` - creates a GitHub **pre-release**
   (installed apps do not see it). Download its assets back and verify the hash and signature.
6. `py tools/release.py --promote` - makes it the latest release: installed apps now offer the update.

Rollback: `gh release edit vX.Y.Z --repo mba-x86/Process-killer --latest` on the previous release.

## Other checks
- `node tools/e2e.mjs <exe>` - real backend vs UI contract (build with `PK_NO_ADMIN_MANIFEST=1` for a prompt-free test exe; never ship that one).
- `py tools/measure.py <exe>` - launch time / memory / idle CPU.
- `py tools/shot.py <route>` - screenshot of a page rendered with the mock backend.
