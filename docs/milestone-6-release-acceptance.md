# Milestone 6 release acceptance

This checklist is a disposable-fixture acceptance pass for persisted History,
reviewed Undo, and the release deliverables. It does not require a Linux
distribution package or touch a user's real Downloads or Trash directories.

## Automated coverage

Run the full verification set:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

The following focused tests document the acceptance scenarios covered by the
suite. Each uses isolated temporary filesystem state.

```bash
cargo test completed_moves_and_renames_appear_in_persisted_history
cargo test completed_trash_is_recorded_with_its_payload_path_after_restart
cargo test removal_batches_stop_between_entries_and_keep_remaining_marks_on_refresh_failure
cargo test blocked_newest_undo_does_not_skip_to_an_older_record
cargo test undo_preview_restores_the_newest_trash_payload_to_its_original_path
cargo test occupied_original_path_blocks_trash_restore_without_changing_the_payload
cargo test history_clear_requires_typed_confirmation_and_preserves_manual_inbox_work
cargo test malformed_external_history_disables_undo_without_disabling_manual_inbox_actions
cargo test changed_newest_history_record_invalidates_an_open_undo_preview
cargo test completed_move_reports_when_history_cannot_be_written
```

Together these verify restart persistence, partial-batch history records,
blocked Undo, original-path-only Trash restoration, deletion audit entries,
typed clear-history confirmation, corrupt-history recovery, concurrent history
changes, and truthful reporting when journaling fails after a completed
filesystem operation.

## Release artifact acceptance

1. Push a version tag such as `v0.1.0`.
2. Confirm the **Verify** workflow runs formatting, compilation, Clippy, and
   tests on stable Rust.
3. Confirm the **Release source archive** workflow attaches
   `downloads-janitor-v0.1.0-source.tar.gz` to the matching GitHub Release.
4. Download and unpack that archive in a clean directory, then follow the
   installation commands in the README. The build must pass with `--locked`.

The artifact is a source archive generated from the tagged commit with
`git archive`; it deliberately avoids distribution-specific package
maintenance while retaining a reproducible, versioned release input.
