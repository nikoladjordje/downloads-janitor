# 05: Original-Path-Only Undo for Trash

**What to build:** The user can review and restore the newest reversible Trash
record from its recorded payload to its original path only. Successful restore
marks the record reversed; Milestone 6 does not add Redo or alternative routing.

**Blocked by:** 02: History for Completed Trash Operations; 04: Reviewed Undo for Moves and Renames.

**Status:** complete

- [x] Undo Preview validates the recorded Trash payload and original path before
  it authorizes restoration.
- [x] An occupied or unavailable original path refuses restoration clearly and
  never offers overwrite, auto-rename, merge, or a newly chosen Destination.
