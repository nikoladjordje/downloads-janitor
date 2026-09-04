# Downloads Janitor

Downloads Janitor is a keyboard-driven Linux terminal application for reviewing
entries in `~/Downloads` and safely moving one selected entry to a directory
beneath `$HOME`. Milestone 3 supports explicitly requested, same-filesystem
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

The application has three screens:

1. **Inbox** lists the immediate files, directories, and usable symlinks in
   `$HOME/Downloads`. Select one entry and press `Enter`.
2. **Destination Browser** starts at `$HOME`. Browse to an existing directory
   and press `d` to choose the current directory.
3. **Move Preview** shows the selected entry's type, exact source path, chosen
   Destination, exact resulting path, and any validation failures. A valid
   Preview warns that `m` changes the filesystem; pressing it starts one Move
   Attempt directly.

Returning to an earlier screen preserves its selection. Only one Proposed Move
is represented at a time.

## Keybindings

### Inbox

| Key | Action |
| --- | --- |
| `j` or Down Arrow | Select the next Inbox Entry |
| `k` or Up Arrow | Select the previous Inbox Entry |
| `gg` | Select the first Inbox Entry |
| `G` | Select the final Inbox Entry |
| `Enter` | Open the Destination Browser for the selected entry |
| `q` | Quit and restore the terminal |

### Destination Browser

| Key | Action |
| --- | --- |
| `j` or Down Arrow | Select the next row |
| `k` or Up Arrow | Select the previous row |
| `gg` | Select the first row |
| `G` | Select the final row |
| `Enter`, `l`, or Right Arrow | Enter the selected real directory, or select `..` |
| `h`, Left Arrow, or Backspace | Return to the parent directory |
| `d` | Choose the current directory and open Preview |
| `Esc` | Return to the Inbox |
| `q` | Quit and restore the terminal |

### Move Preview

| Key | Action |
| --- | --- |
| `m` | Execute one freshly validated Move Attempt when the proposal is valid |
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
guarantee writability or that a future move will succeed.

## Execution and safety

A **Proposed Move** is the read-only source, Destination, and resulting path
shown in Preview. Pressing `m` captures the source's non-following filesystem
identity and entry type and creates one **Move Attempt**: all Preview validation
is repeated, the current source identity and type are compared with the captured
values, and only then can mutation occur.
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

On failure, Move Preview retains the exact operation and displays the reason.
`m` retries with fresh validation and identity verification; `Esc` rebuilds the
proposal from current filesystem facts before returning to Destination Browser.
No retry overwrites, implicitly renames, copies, rolls back, or queues an entry.

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

### Milestone 1 — Read-Only Inbox Review

Milestone 1 solves the first problem in cleaning up Downloads: understanding
what is there without risking accidental changes. It provides a keyboard-driven
Inbox that lists immediate files, directories, and usable symlinks while keeping
the entire application read-only.

### Milestone 2 — Destination Selection and Move Preview

Milestone 2 solves the planning problem: deciding where one Inbox Entry should
go and checking whether that move is sensible before changing the filesystem.
It adds the bounded Destination Browser and a read-only Move Preview with path,
collision, source, and directory-ancestry validation.

### Milestone 3 — Safe Move Execution

Milestone 3 solves the execution problem: carrying out the exact move the user
reviewed while protecting against stale filesystem state and collisions. It
adds direct execution from Move Preview with a narrow guarantee of one
deliberate same-filesystem move at a time, using atomic no-replace behavior.

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
