# 02: History for Completed Trash Operations

**What to build:** Individual and batch desktop Trash actions produce truthful
per-entry History records that retain the concrete recovery information needed
to review a future restoration. Failed and unattempted entries do not become
history.

**Blocked by:** 01: Persistent Move and Rename History.

**Status:** complete

- [x] Every successfully trashed entry appears as one potentially reversible
  History record, including completed entries from a partial batch.
- [x] A Trash record retains enough verified restoration information to identify
  its exact payload and original path after restart.
