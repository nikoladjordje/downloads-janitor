# Downloads Janitor

Downloads Janitor is a keyboard-driven Linux terminal application for reviewing
entries in `~/Downloads` and safely moving one selected entry to a directory
beneath `$HOME`. Milestone 3 supports explicitly confirmed, same-filesystem
moves without overwriting an existing path.

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

The application has four screens:

1. **Inbox** lists the immediate files, directories, and usable symlinks in
   `$HOME/Downloads`. Select one entry and press `Enter`.
2. **Destination Browser** starts at `$HOME`. Browse to an existing directory
   and press `d` to choose the current directory.
3. **Move Preview** shows the selected entry's type, exact source path, chosen
   Destination, exact resulting path, and any validation failures. A valid
   Preview remains read-only; press `m` to continue.
4. **Confirmation** repeats the exact operation and warns that execution will
   change the filesystem. `Enter` is the sole action that authorizes a Move
   Attempt.

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
| `m` | Open Confirmation when the Proposed Move is valid |
| `Esc` | Return to the Destination Browser |
| `q` | Quit and restore the terminal |

### Confirmation

| Key | Action |
| --- | --- |
| `Enter` | Execute one freshly validated Move Attempt |
| `Esc` | Rebuild Move Preview using current filesystem state |
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
guarantee writability or that a future move will succeed.

## Execution and safety

A **Proposed Move** is the read-only source, Destination, and resulting path
shown in Preview. Opening Confirmation captures the source's non-following
filesystem identity and entry type. Pressing `Enter` creates one **Move
Attempt**: all Preview validation is repeated, the current source identity and
type are compared with the captured values, and only then can mutation occur.
A **Completed Move** means the kernel successfully placed the entry at the
resulting path.

Execution uses Linux atomic no-replace rename behavior. It supports regular
files, non-empty directories, and Symlink Entries when source and Destination
are on the same filesystem. Directories are moved as single native entries;
their contents are not recursively enumerated, copied, or merged. A symlink
move renames the link itself and leaves its target untouched.

The no-replace kernel operation prevents overwriting even if a collision appears
between validation and execution. Cross-filesystem moves fail without copying
or deleting the source. Source identity verification is a best-effort userspace
defense: another process could still replace the source during the unavoidable
interval between the final identity check and the rename operation.

On failure, Confirmation retains the exact operation and displays the reason.
`Enter` retries with fresh validation and identity verification; `Esc` rebuilds
Preview from current filesystem facts. No retry overwrites, implicitly renames,
copies, rolls back, or queues an entry.

After a Completed Move, the Inbox is rescanned and selection remains at the old
numeric index where possible, clamped to the final entry or cleared when empty.
The success notice remains until the next handled action. If refresh fails after
the move, the move still counts as completed: the app returns to Inbox, removes
the known-moved source from its retained list, and reports both success and that
the remaining entries may be stale.

Downloads Janitor does not provide cross-filesystem copy-then-delete, overwrite,
merge or collision resolution, automatic or user-directed renaming, delete or
trash, undo or rollback, queues or bulk execution, persistence, rules,
recursive processing, filesystem watching, timers, or background work.

## Roadmap

Milestones 1 through 3 are implemented. Milestones 4 through 6 are proposed
directions whose scope will be refined before implementation.

### Milestone 3 — Safe Move Execution

Milestone 3 adds the explicit Confirmation and safe single-move execution
described above. Its narrow guarantee is one confirmed same-filesystem move at
a time, using atomic no-replace behavior.

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
