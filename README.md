# Downloads Janitor

Downloads Janitor is a keyboard-driven Linux terminal application for reviewing
entries in `~/Downloads` and previewing where one entry could be placed. The
currently implemented Milestone 2 workflow is strictly read-only: it never
moves, renames, deletes, queues, or persists anything.

## Requirements

- Linux
- The current stable Rust toolchain, including Cargo
- A readable `$HOME/Downloads` directory

Install stable Rust through [rustup](https://rustup.rs/) if it is not already
available.

## Build and run

From the repository root:

```bash
cargo build
cargo run
```

## Workflow

The application has three screens:

1. **Inbox** lists the immediate files, directories, and usable symlinks in
   `$HOME/Downloads`. Select one entry and press `Enter`.
2. **Destination Browser** starts at `$HOME`. Browse to an existing directory
   and press `d` to choose the current directory.
3. **Move Preview** shows the selected entry's type, exact source path, chosen
   Destination, exact resulting path, and any validation failures. Press `Esc`
   to return to the browser.

Returning to an earlier screen preserves its selection. Only one Proposed Move
is represented at a time.

## Keybindings

### Inbox

| Key | Action |
| --- | --- |
| `j` or Down Arrow | Select the next Inbox Entry |
| `k` or Up Arrow | Select the previous Inbox Entry |
| `Enter` | Open the Destination Browser for the selected entry |
| `q` | Quit and restore the terminal |

### Destination Browser

| Key | Action |
| --- | --- |
| `j` or Down Arrow | Select the next row |
| `k` or Up Arrow | Select the previous row |
| `Enter`, `l`, or Right Arrow | Enter the selected real directory, or select `..` |
| `h`, Left Arrow, or Backspace | Return to the parent directory |
| `d` | Choose the current directory and open Preview |
| `Esc` | Return to the Inbox |
| `q` | Quit and restore the terminal |

### Move Preview

| Key | Action |
| --- | --- |
| `Esc` | Return to the Destination Browser |
| `q` | Quit and restore the terminal |

Navigation stops at the first and final rows rather than wrapping.

## Destination browsing policy

`$HOME` is the browser's hard boundary. The browser can descend into real child
directories but cannot navigate above the home directory.

Ordinary files and hidden directories—names beginning with `.`—are omitted.
Directory symlinks are displayed as disabled and are never followed or chosen.
Below `$HOME`, `..` is shown first, followed by sorted real directories and then
sorted disabled directory symlinks. Directories are rescanned when entered or
returned to, and filesystem errors are shown without changing the current
location.

## Preview and validation

A Proposed Move preserves the selected Inbox Entry's basename and identity. In
particular, selecting a symlink proposes the link itself, not its target.

The source and Destination are checked again every time Preview opens. A
proposal is shown as invalid when:

- the Destination is missing or cannot be inspected;
- the Destination is not a real directory;
- the resulting path already exists or cannot be inspected;
- a directory would be placed inside itself or one of its descendants;
- the source is missing or cannot be inspected; or
- the source and resulting path are identical.

A valid Preview means only that these checks passed at that moment. It does not
guarantee writability or that a future move would succeed.

## Milestone 2 scope and safety

Preview does not perform or authorize a move. The production code only reads
directory entries and filesystem metadata. It does not inspect file contents or
recursively scan directory contents.

Execution, confirmation, collision resolution, renaming, queues, configuration,
persistence, favorites, hidden-directory browsing, directory-symlink traversal,
undo/history, organization rules, filesystem watching, and background work are
deliberately deferred.

## Roadmap

Milestones 1 and 2 are implemented. Milestone 3 has an agreed specification but
is not implemented yet. Milestones 4 through 6 are proposed directions whose
scope will be refined before implementation.

### Milestone 3 — Safe Move Execution

Milestone 3 will turn one valid Proposed Move into an explicitly authorized
filesystem change. A valid Preview will open a separate Confirmation screen
showing the exact source and resulting paths. Pressing `Enter` there will start
one Move Attempt after repeating validation and verifying that the source is
still the same Inbox Entry the user reviewed.

Execution will use Linux's atomic no-replace rename behavior. Regular files,
non-empty directories, and Symlink Entries will be supported when source and
Destination are on the same filesystem. Existing paths will never be
overwritten, and cross-filesystem copy-then-delete behavior will remain out of
scope.

A failed Move Attempt will remain recoverable on Confirmation and may be
retried using fresh filesystem state. A Completed Move will return to a
rescanned Inbox, select the next logical entry, and show a success notice. If
the move succeeds but refreshing the Inbox fails, the application will report
both outcomes accurately rather than claiming that the move failed.

The complete planned behavior is defined in
[the Milestone 3 specification](./Downloads%20Janitor%20%E2%80%94%20Milestone%203%20Specification.md).

### Milestone 4 — Efficient Inbox Processing

The proposed fourth milestone will make repeated review faster after safe
single-move execution has been proven. Likely work includes a smoother
process-the-Inbox loop and explicit rename, ignore, and delete or trash
decisions. Exact action semantics, safety rules, and whether ignored entries
persist have not been decided.

### Milestone 5 — Configuration and Rules

The proposed fifth milestone will introduce user-controlled configuration,
favorite Destinations, and deterministic organization rules. Rules will remain
explicit and understandable rather than using AI classification. Configuration
format, rule precedence, matching behavior, and persistence are still to be
designed.

### Milestone 6 — History, Undo, and Release Hardening

The proposed sixth milestone will focus on trustworthy recovery and a polished
release. Likely work includes operation history, undo where filesystem
semantics permit it, packaging, installation guidance, broader acceptance
testing, and release hardening. The guarantees and limits of undo require a
separate design before this scope is considered committed.

## Verification

Run the reproducible automated checks from the repository root:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
