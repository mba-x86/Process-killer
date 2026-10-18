# Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
"""Single source of truth for the app version and update location."""
__version__ = "1.0.3"
APP_NAME = "Process Killer"
AUTHOR = "mba-x86"
COPYRIGHT = "© 2026 mba-x86"
LICENSE_NAME = "MIT License"
REPO_URL = "https://github.com/mba-x86/Process-killer"

# Where new versions are published. Use ONE of:
#   "github:OWNER/REPO"            -> reads  https://github.com/OWNER/REPO/releases/latest/download/latest.json
#   "https://example.com/latest.json"  -> any HTTPS URL that serves the same JSON
# Leave empty to disable updates. Users can override it in %APPDATA%\ProcessKiller\settings.json
UPDATE_SOURCE = "github:mba-x86/Process-killer"