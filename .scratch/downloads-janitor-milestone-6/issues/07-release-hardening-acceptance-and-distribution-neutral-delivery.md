# 07: Release-Hardening Acceptance and Distribution-Neutral Delivery

**What to build:** The completed Milestone 6 workflow is reproducible for users
and maintainers through a distribution-neutral release artifact, installation
guidance, automated verification, and a documented acceptance scenario.

**Blocked by:** 01: Persistent Move and Rename History; 02: History for Completed Trash Operations; 03: Irreversible Deletion Audit Records; 04: Reviewed Undo for Moves and Renames; 05: Original-Path-Only Undo for Trash; 06: History Recovery Controls and Multi-Instance Safety.

**Status:** complete

- [x] CI verifies formatting, compilation, linting, and tests; release artifacts
  and installation guidance can be reproduced without distribution-specific
  package maintenance.
- [x] Acceptance coverage verifies restart persistence, partial batches, blocked
  Undo, Trash restoration, deletion audit, clear confirmation, corruption,
  concurrent history changes, and post-operation journaling failure.
