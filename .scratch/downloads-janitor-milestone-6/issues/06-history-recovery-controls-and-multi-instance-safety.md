# 06: History Recovery Controls and Multi-Instance Safety

**What to build:** The user can intentionally clear all history with typed
confirmation, while corruption and concurrent instances cannot cause the
application to silently trust stale recovery information.

**Blocked by:** 02: History for Completed Trash Operations; 03: Irreversible Deletion Audit Records; 04: Reviewed Undo for Moves and Renames; 05: Original-Path-Only Undo for Trash.

**Status:** complete

- [x] Typing `clear history` clears all audit and reversal records; malformed or
  externally changed history is preserved and disables History/Undo while manual
  Inbox actions remain usable.
- [x] Concurrent instances serialize updates, and a changed newest record
  invalidates an open Undo Preview so the user must review again.

Verification: `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and
`git diff --check` pass. History mutations take an exclusive private companion
lock and reload the persisted journal before applying changes, preventing stale
instances from losing records. The History screen exposes `c` → type `clear
history` → Enter. Corrupt external history remains untouched, blocks History and
Undo, and leaves manual Inbox actions available; a changed newest record blocks
execution from an already open Undo Preview.
