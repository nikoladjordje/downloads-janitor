# 01: Persistent Move and Rename History

**What to build:** Completed Moves and in-Inbox renames appear in a bounded,
persisted History view after restart. The user can distinguish reversible
records from actions that completed but could not be recorded for application
Undo.

**Blocked by:** None (can start immediately).

**Status:** complete

- [x] A completed Move or in-Inbox rename creates one reversible History record
  and remains visible after restart, with the most recent 100 records retained.
- [x] The History view accurately identifies the newest reversible record and
  reports a completed-but-not-undoable action when its journal write fails.
