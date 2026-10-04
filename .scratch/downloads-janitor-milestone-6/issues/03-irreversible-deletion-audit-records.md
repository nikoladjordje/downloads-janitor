# 03: Irreversible Deletion Audit Records

**What to build:** Individual and batch permanent deletions appear in History as
clearly irreversible completed actions, giving the user a truthful audit trail
without suggesting that deleted data can be recovered.

**Blocked by:** 01: Persistent Move and Rename History.

**Status:** complete

- [x] Each completed permanent deletion has one visible irreversible History
  record, including entries completed before a batch stop or failure.
- [x] Permanent-deletion records never offer Undo or alter the newest reversible
  Undo candidate.
