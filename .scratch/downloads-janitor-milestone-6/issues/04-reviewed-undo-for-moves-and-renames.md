# 04: Reviewed Undo for Moves and Renames

**What to build:** The user can inspect and undo only the newest reversible Move
or in-Inbox rename through a dedicated Undo Preview. A successful Undo returns
the entry to its recorded original path without overwriting anything and marks
the record reversed.

**Blocked by:** 01: Persistent Move and Rename History.

**Status:** complete

- [x] Undo Preview shows the recorded action, current path, intended original
  path, and fresh validation before Enter can authorize the reverse action.
- [x] A missing or replaced source, collision, invalid parent, cross-filesystem
  condition, or newer blocking record refuses Undo without skipping to history
  entries below it.
