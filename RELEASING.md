# Releasing Process Killer

## One-time setup
1. Create a GitHub repo (public is simplest - the app downloads updates without a login).
2. In `version.py` set `UPDATE_SOURCE = "github:YOUR-USER/YOUR-REPO"`.
3. Optional for one-command publishing: install the GitHub CLI (`winget install GitHub.cli`) and run `gh auth login`.

## Every release
1. Bump `__version__` in `version.py` (e.g. `1.0.0` -> `1.1.0`).
2. Put the "what's new" text in `release_notes.md`.
3. Run `py build.py` -> creates in `dist/`:
   - `ProcessKiller-Setup-<version>.exe`  (the installer you share)
   - `latest.json`                        (tells installed copies a new version exists)
4. Publish: `py build.py --publish`
   (or manually create a GitHub release tagged `v<version>` and upload BOTH files).

Installed copies check `latest.json` on startup, show "Version X is available", then download,
verify the SHA-256, and run the installer silently. The app closes and reopens on the new version.

## Rules that keep updates working
- Never change `AppId` in `installer.iss` - Windows uses it to recognise upgrades.
- Always upload BOTH files; the release must be the repo's *latest* release.
- Windows may show SmartScreen "unknown publisher" for new installers until the file is code-signed.
