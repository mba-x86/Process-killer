# Activity API
Poll: none (event driven). Admin: no.
- `get_activity({ limit })` -> `{ "entries": [{ "ts": 1790000000, "level": "info"|"ok"|"fail", "message": "Started in 0.8s" }] }` oldest first, newest last; UI asks for 500. Slow-task lines ("Slow task: X took 3.3s") are level `info`; the UI colours them amber.
- `clear_activity()` -> null (clears the in-memory buffer; the log file stays).
- `open_log_file()` -> null (opens the log file in the default editor; Error if it does not exist).
- Event `activity:entry` `{ ts, level, message }` for every new line (also emitted from background threads).
