# Downloads Janitor

Downloads Janitor is a keyboard-driven Linux terminal application for reviewing
entries in `~/Downloads` and safely moving one selected entry to a directory
beneath `$HOME`. Milestone 3 supports explicitly requested, same-filesystem
moves without overwriting an existing path. In-place renaming and editing the
resulting basename during a move are also implemented as part of Milestone 4.

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

Moving an entry uses three screens:

1. **Inbox** lists the immediate files, directories, and usable symlinks in
   `$HOME/Downloads`. Select one entry and press `Enter`.
2. **Destination Browser** starts at `$HOME`. Browse to an existing directory
   and press `d` to choose the current directory.
3. **Move Preview** shows the selected entry's type, exact source path, chosen
   Destination, exact resulting path, and any validation failures. A valid
   Preview warns that `Enter` changes the filesystem; pressing it starts one Move
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
| `r` | Edit the selected entry's basename in place |
| `Space` | Toggle the highlighted entry's mark |
| `V` | Start or finish an inclusive visual range |
| `a` | Toggle selection of all visible entries |
| `c` or `Esc` | Clear all marks and exit visual mode |
| `R` | Refresh Inbox, reload ignored state, and reconcile marks |
| `i` | Persistently ignore marked entries, or the highlighted entry |
| `I` | Switch between Inbox and Ignored Entries |
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
| `Enter` | Execute one freshly validated Move Attempt when the proposal is valid |
| `r` | Edit the resulting basename and return to Preview |
| `Esc` | Return to the Destination Browser |
| `q` | Quit and restore the terminal |

Navigation stops at the first and final rows rather than wrapping. Opening Preview
with `d` never executes a move; press `Enter` after reviewing it, or `Esc` to
return without executing. Key repeat and release events are ignored. Repeated
Enter presses in the Destination Browser only navigate; `d` is required to
open Preview.

## Select and refresh Inbox Entries

The highlighted row is the cursor. A separate `[x]` suffix marks an entry, and
the header reports the marked count. Space toggles the highlighted entry.
`a` selects all visible entries, or clears them when all are marked; `c`
clears marks.

Uppercase `V` starts visual selection at the cursor and marks that entry.
Navigation, including `gg` and `G`, grows or shrinks the inclusive range around
that fixed anchor. Marks present before visual mode remain selected even when
they fall outside the current range. Press `V` again to keep the marks and
finish visual mode; `Esc` exits and clears all marks. Space or `a` finishes
visual mode before applying its toggle.

Navigation uses a stable list. Press uppercase `R` to refresh explicitly:
visual mode ends, and marks survive only when the same path and non-following
filesystem identity are still present. A renamed, removed, or replaced entry
loses its mark; content changes to the same entry retain it. Symlinks are
identified by the link itself. Identity comparison is best-effort and cannot
distinguish filesystem identifier reuse. The cursor follows the same entry
when possible, otherwise its index is clamped to the refreshed list.
Refresh failure keeps the list and marks and reports that entries may be stale;
`R` retries.

Until bulk actions are available, Enter and `r` refuse multiple marked entries
with an explanation. One marked entry takes precedence over the cursor; without
marks they use the highlighted entry. A marked entry that has been replaced or
removed is refused until refresh. Starting an action exits visual mode.
Successful individual actions clear the consumed mark; cancelling preserves it.

## Ignore and restore entries

Press `i` in Inbox to persistently hide marked entries, or the highlighted entry
when nothing is marked. No confirmation is needed: ignoring changes visibility
and saved state, without moving or deleting the entries. Successful ignore
clears the affected marks and keeps the cursor near its former numeric position.

Press uppercase `I` to switch to **Ignored Entries**. This view supports the same
navigation, Space, visual ranges, select-all, clear, and refresh controls.
Press `u` to restore marked entries, or the highlighted entry. Restore changes
saved state and returns entries to the normal Inbox; use `I` to return there
for move or rename actions. Move, rename, and ignore bindings are disabled in
Ignored Entries. Switching views clears marks and starts at the first row.

Ignored state is stored in
`$HOME/.local/state/downloads-janitor/ignored-v1`. It records exact pathname
bytes and the entry's device, inode, and file type without following symlinks.
Content changes to the same entry keep it ignored; a replacement at the same
path is visible. Changing a symlink target does not unignore the link.
Identity matching is best-effort: filesystem identifier reuse can make a new
entry appear to be the old one. Missing entries are omitted from both views;
their records remain saved in case the same entry returns.

For a bulk ignore or restore, every selected entry is checked before saving.
A missing or replaced source blocks the whole selection. The set is saved in
one transaction, in deterministic path order, with no partially hidden subset:
failure leaves list visibility and marks unchanged. A private temporary file is
flushed before atomic replacement of the saved file; the containing directory
is then flushed. If directory flushing fails after replacement, the operation
is reported as saved with a crash-durability warning.

Sessions coordinate saves through `ignored.lock`; stale sessions refuse to
overwrite state changed by another session. Press `R` to reload and retry.
The lock is released automatically when a process exits. Temporary files left
by a crash are unused and may be removed while the app is stopped.

If state is unreadable, malformed, or not a regular file, all entries are shown
in the normal Inbox with a persistent warning. Ignore and restore saves are
blocked to preserve that file. Back up and repair it, or move it aside to reset
ignored state, then press `R` (or restart). A refresh scan failure retains the
current usable list. Ignored entries remain filtered after successful moves,
renames, and refreshes.

## Choose a new name during a move

From Move Preview, press `r` to edit the resulting basename. The editor starts
with the currently proposed name and uses the same append, Backspace, and
`Ctrl+u` controls as in-place rename. Ordinary letters, including `q`, are
filename text. Press `Enter` to return to a freshly validated Move Preview;
press `Esc` to discard edits and keep the previous proposal.

Preview displays the exact source, Destination, and resulting path using quoted,
escaped notation, preserving non-UTF-8 bytes. Press a separate `Enter` to move
directly to that resulting path. There is no intermediate rename in the Inbox.
Keeping the original basename is valid when moving to a different directory.

Filename validation, collision and ancestry checks, Destination restrictions,
source identity verification, and atomic same-filesystem no-overwrite behavior
still apply. The source identity is captured when editing first opens and retained
through edits and retries. A failed attempt retains the edited path; `Enter`
retries it, or `r` edits it again. The accepted name persists when returning to
the Destination Browser, but resets when starting another entry from Inbox.
If a proposal was already invalid at Preview time, reopen the editor and accept
the name to refresh validation after fixing the filesystem problem.

## Rename an Inbox Entry

Press `r` in Inbox to edit the selected entry's current basename. Type to append,
use `Backspace` to remove the final character, or `Ctrl+u` to clear the name.
Letters such as `q`, `j`, and `g` are ordinary filename text in this editor.

Press `Enter` to open **Rename Preview**, a read-only **Proposed Rename** showing
the exact old and new paths within the Inbox. A separate `Enter` executes a
valid rename. `Esc` from either rename screen cancels and returns to Inbox.
From Preview, `q` quits.

Empty names, `.`, `..`, slash-separated paths, NUL bytes, unchanged names, and
occupied paths (including broken symlinks) cannot execute. Other Linux filename
characters, including spaces, Unicode, and backslashes, are accepted. Filesystem
name-length limits are also checked when inspecting the resulting path.
The editor and Preview display quoted, escaped names: the quotes and escapes
are display notation, not text inserted into the filename. Existing non-UTF-8
bytes are retained exactly; Backspace removes a final Unicode character when
valid, or a single raw byte otherwise.

The source identity and type are captured when the editor opens and checked
again at execution. Renaming uses the same fresh validation and atomic
no-overwrite operation as moving. Non-empty directories retain their contents;
symlinks themselves are renamed and their targets remain untouched. The same
best-effort source identity limitation described below applies.

Failure retains the reviewed paths and shows an error; `Enter` retries with
fresh validation. To change the proposal, cancel and reopen the editor.
Success refreshes Inbox and highlights the renamed entry at its new sorted
position. If refresh fails, the known rename is reflected in the retained list,
the entry stays selected, and a success notice warns that other entries may be
stale.

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

A Proposed Move defaults to the selected Inbox Entry's basename; the user may
explicitly edit the resulting basename. The entry's identity is preserved:
selecting a symlink proposes the link itself, not its target.

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
shown in Preview. The source's non-following filesystem identity and entry type
are captured when basename editing first opens, or when execution is first
requested if the editor was not opened. A marked entry uses its recorded
identity throughout the move workflow, including retries. Pressing `Enter` creates one
**Move Attempt**: all Preview validation is repeated, the current source
identity and type are compared with the captured
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
`Enter` retries with fresh validation and identity verification; `Esc` rebuilds the
proposal from current filesystem facts before returning to Destination Browser.
No retry overwrites, changes the reviewed basename, copies, rolls back, or queues an entry.

After a Completed Move, the Inbox is rescanned and selection remains at the old
numeric index where possible, clamped to the final entry or cleared when empty.
The success notice remains until the next handled action. If refresh fails after
the move, the move still counts as completed: the app returns to Inbox, removes
the known-moved source from its retained list, and reports both success and that
the remaining entries may be stale.

Downloads Janitor does not provide cross-filesystem copy-then-delete, overwrite,
merge or collision resolution, delete or
trash, undo or rollback, queues or bulk filesystem execution, rules,
recursive processing, filesystem watching, timers, or background work.

## Roadmap

Milestones 1 through 3 are implemented. Milestone 4 is in progress: Enter
execution, individual in-place renaming, basename editing during moves, selection,
explicit refresh, and persistent ignore/restore are available. Milestones 5 and 6
remain proposed directions.

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

The fourth milestone makes repeated review faster. Enter execution,
individual in-place renaming, basename editing during moves, selection, and
explicit refresh, and persistent ignore/restore are implemented. Remaining
planned work includes bulk moves and reviewed trash or permanent deletion.

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
