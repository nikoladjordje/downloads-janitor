# Downloads Janitor

Downloads Janitor is a keyboard-driven Linux terminal application for reviewing
entries in `~/Downloads` and safely moving selected entries to a directory
beneath `$HOME`. Milestones 1–4 provide reviewed same-filesystem moves without
overwriting, individual renaming, selection and bulk actions, persistent
ignore/restore, desktop Trash, and typed confirmation for permanent deletion.

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
2. **Destination Browser** initially starts at `$HOME` and remembers the current
   directory for subsequent moves in the session. Browse to an existing directory
   and press `d` to choose the current directory.
3. **Move Preview** shows the selected entry's type, exact source path, chosen
   Destination, exact resulting path, and any validation failures. A valid
   Preview warns that `Enter` changes the filesystem; pressing it starts one Move
   Attempt directly.

Returning to an earlier screen preserves its selection. Multiple marked entries
share one Destination and open Bulk Move Preview.

## Keybindings

### Inbox

| Key | Action |
| --- | --- |
| `j` or Down Arrow | Select the next Inbox Entry |
| `k` or Up Arrow | Select the previous Inbox Entry |
| `gg` | Select the first Inbox Entry |
| `G` | Select the final Inbox Entry |
| `Enter` | Open the Destination Browser for marked entries, or the highlighted entry |
| `r` | Edit the selected entry's basename in place |
| `t` | Review marked entries, or the highlighted entry, for desktop Trash |
| `D` | Open typed confirmation to permanently delete marked entries, or the highlighted entry |
| `Space` | Toggle the highlighted entry's mark |
| `V` | Start or finish an inclusive visual range |
| `a` | Toggle selection of all visible entries |
| `c` or `Esc` | Clear all marks and exit visual mode |
| `R` | Refresh Inbox, reload ignored state, and reconcile marks |
| `i` | Persistently ignore marked entries, or the highlighted entry |
| `I` | Switch between Inbox and Ignored Entries |
| `C` | Open Configuration and manage Favorite Destinations and Rules |
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

### Editors, removal reviews, and bulk results

| Screen | Controls |
| --- | --- |
| Ignored Entries | Inbox navigation and selection controls; `u` restores visibility, `I` returns to Inbox, `R` refreshes, `q` quits |
| Filename editor | Type to append, Backspace removes the final character, Ctrl+u clears; Enter reviews, Esc cancels; ordinary letters including `q` are text |
| Rename Preview | Enter executes a valid rename, Esc cancels to Inbox, `q` quits |
| Trash Preview | Enter executes, Esc cancels, `q` quits; `j/k` or arrows scroll, `h/l` or arrows pan, Home resets |
| Permanent Deletion Confirmation | Type exactly `delete`, then Enter; Backspace edits, Ctrl+u clears, Esc cancels; arrows scroll/pan, Home resets; `q` is text |
| Bulk review | Enter executes (deletion requires typed `delete` first), Esc goes back; arrows scroll/pan, Page Up/Down page, Home resets; `j/k`, `h/l`, and `q` work except when entering deletion text |
| Bulk progress | Esc stops before the next entry; other keys have no effect |
| Bulk results | Enter or Esc returns to Inbox, `q` quits; `j/k` or arrows scroll, `h/l` or arrows pan, Page Up/Down page, Home resets |

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

Enter, `t`, and `D` support multiple marked entries; `r` refuses multiple marks
with an explanation. One marked entry takes precedence over the cursor; without marks
they use the highlighted entry. A marked entry that has been replaced or
removed is refused until refresh. Starting an action exits visual mode.
Successful individual actions clear the consumed mark; cancelling preserves it.

## Move selected entries

Mark two or more entries with Space, `V`, or `a`, then press `Enter` to browse
for one Destination. Press `d` to open **Bulk Move Preview**. It lists every
exact source and resulting path, preserving all original basenames; bulk moves
have no name editor. Use `j`/`k` or arrows to scroll, Page Up/Down to page,
`h`/`l` or left/right arrows to pan long paths, and Home to return to the start.
Quoted, escaped paths preserve exact filename bytes.

Press `Enter` to check the entire reviewed set again and execute only if every
entry passes. Known collisions, missing or replaced sources, invalid Destinations,
and cross-filesystem moves block the whole set without moving any entry.
Per-entry problems appear in Preview. After fixing a problem, Enter rechecks
and moves if valid; Esc returns to the browser. To accept a replaced source,
return to Inbox, refresh with `R`, and select it again.

Execution runs in the foreground in ascending source-path byte order, with a
progress redraw between entries. Each entry is freshly validated, including its
marked filesystem identity and real-directory Destination ancestry beneath HOME,
immediately before its atomic no-replace move. There is no copying, merging,
overwriting, automatic renaming, or rollback. The same best-effort userspace
identity and path-check race limits as individual moves apply.

During execution, `Esc` stops before the next entry. An entry already being
processed finishes first. Other keys have no effect while processing. The first
execution failure also stops the batch; completed moves remain completed.
**Bulk Move Results** lists each entry as Completed, Failed (with its reason),
or Unattempted, and shows counts for all three outcomes. The result list supports
the same scrolling controls. Enter or Esc returns to Inbox.

After processing, Inbox refreshes and its cursor stays near the former index.
Completed entries lose their marks. Failed and unattempted entries remain marked
when their identities still match, ready for another reviewed attempt. Completed
results cannot be executed again. If refresh fails, known completed entries are
removed from the retained list and the outcome includes a stale-list warning;
`R` retries refresh from Inbox.

## Trash or permanently delete selected entries

Mark entries with Space, `V`, or `a`. Press `t` for **Bulk Trash Preview**, or
uppercase `D` for **Bulk Permanent Deletion Confirmation**. Marked entries take
precedence over the highlighted row. With no marks, the individual workflow
uses the highlighted entry; with one mark, it uses that entry.

Both batch reviews list every exact source path and its action, in ascending
source-path byte order. Use arrows to scroll vertically or pan long paths,
Page Up/Down to page, and Home to reset the view. Trash also supports `j`/`k`
and `h`/`l`. Paths use quoted, escaped notation to retain exact filename bytes.

From Trash preview, Enter rechecks the entire set and starts only if all entries
pass. For permanent deletion, the review warns that the action bypasses Trash,
removes all selected directory contents, and cannot be undone. Type exactly
`delete`, then Enter, once for the whole reviewed set. Letters including `q`
are confirmation text; Backspace edits and Ctrl+u clears. Empty text, different
case, or extra characters cannot authorize deletion. A blocked attempt clears
the confirmation and requires typing it again. Esc cancels either review and
preserves marks. Repeated action keys and reported repeat/release events cannot
bypass review or typed confirmation.

Preflight checks all source identities and parent access. Trash also checks
existing storage directories, storage ancestry, available parent access, and
filesystem compatibility without creating storage. Known problems block the
whole set and appear beside the affected entries. These checks do not guarantee
execution: permissions, filesystem state, and directory contents can change;
recursive child failures may only become apparent during deletion.

Processing runs in the foreground, with progress drawn between entries. Each
entry is freshly validated immediately before execution. Esc requests stopping
before the next entry; it does not interrupt the current entry or recursive
deletion. The first execution failure also stops processing. Completed actions
remain completed, with no rollback. Trash never falls back to permanent deletion.

Results show each entry as **Completed**, **Failed** with its error, or
**Unattempted**, with counts for all three. Recursive-deletion failures warn
that some contents may already be permanently deleted. Trash errors retain any
warning about orphan restoration metadata. Enter or Esc returns to Inbox;
results cannot be executed again. Restore successfully trashed entries through
your file manager; permanent deletions have no undo.

Inbox refreshes after processing. Completed marks are cleared, and failed or
unattempted entries keep their marks only if the same filesystem identities
remain. The cursor stays near its previous index. If refresh fails, completed
rows are removed from the retained list, results remain truthful, and the notice
warns that other rows may be stale. Return to Inbox and press `R` to refresh.
Surviving marks are ready for a new review, which excludes completed entries.

## Permanently delete one entry

Highlight an entry and press uppercase `D` to open **Permanent Deletion
Confirmation**. One marked entry takes precedence over the cursor; multiple
marks open the bulk confirmation described above. The screen shows the exact source path, a count of one,
and a warning that deletion bypasses Trash and removes all directory contents.

Nothing is authorized initially. Type exactly lowercase `delete`, then press
`Enter` to execute. Empty text, different capitalization, or extra characters
cannot execute. `Backspace` edits and `Ctrl+u` clears the text. All ordinary
letters, including `q`, are confirmation text; `Esc` cancels and preserves marks.
Arrows scroll/pan long paths and errors, and Home resets the view. Reported key
repeat/release events are ignored. Reopening confirmation starts with empty text.

Files and non-empty directories are supported. Symlink Entries are unlinked,
and links inside deleted directories are not followed. The Linux implementation
uses [Rust's non-symlink-following recursive removal](https://doc.rust-lang.org/std/fs/fn.remove_dir_all.html).
Source identity and type are checked again immediately before mutation. A
missing or replaced entry is refused; cancel and refresh with `R` to review it
again. Identity checks retain the same best-effort userspace race limitations
as moves, and do not freeze a directory's contents during review.

Permanent deletion cannot be undone through the application or recovered from
Trash. Recursive deletion is not atomic: on failure, some contents may already
have been deleted. The error makes this explicit, with no rollback guarantee.
The reviewed path and mark remain; any retry requires typing `delete` again.
An error warning remains visible when Enter is pressed without new consent.

Success clears the consumed mark, refreshes Inbox, and selects near the old
index. If refresh fails, the app still reports **Permanently deleted 1 entry**,
removes the known-deleted row from its retained list, and warns that other rows
may be stale. `R` retries the refresh.

## Favorite Destinations

Press uppercase `C` from Inbox to open **Configuration**. Favorites have a
unique, case-sensitive name and an existing non-symlink directory at or below
`$HOME`. Use `a` to add one, `Enter` or `e` to edit the highlighted Favorite,
and `x` to remove it. Adding or editing first accepts the name, then reuses the
Destination Browser; press `d` there to save the currently shown directory. A path
that later disappears or becomes invalid remains visible as **unavailable** so
it can be repaired or removed.

Rules are ordered, deterministic instructions made of a non-empty basename
pattern, an Entry Kind (`Any`, `File`, `Directory`, or `Symlink`), and a
reference to a Favorite by name. In Configuration, use uppercase `A`, `E`, and
`X` to add, edit, and remove Rules; uppercase `J`/`K` selects a Rule and `[`/`]
reorders it. The list shows its first-match-wins order. A Rule never duplicates
a Favorite path. If a Favorite is removed, renamed, or becomes unavailable,
the Rule is retained and visibly reported as missing or unavailable for repair.
Rules do not yet change manual move, rename, ignore, Trash, or deletion
workflows.

Favorites and Rules are saved in
`$HOME/.config/downloads-janitor/configuration-v1` and reload on restart. An
unreadable configuration is left untouched and displayed as a warning; repair
it before changing Configuration.

## Send an entry to Trash

Highlight an entry and press `t` to open **Trash Preview**. One marked entry
takes precedence over the cursor; multiple marks open Bulk Trash Preview. Preview shows the exact source in quoted, escaped notation and
states that the action includes directory contents. Use `h`/`l` or left/right
arrows to pan long paths and errors, `j`/`k` or up/down arrows to scroll, and
Home to reset the view.

Press a separate `Enter` to send the reviewed entry to Trash, or `Esc` to
cancel and retain its mark. Repeated `t` presses cannot execute, and reported
key repeat/release events are ignored. Files, non-empty directories, and
symlinks are supported. A symlink is moved as a link, leaving its target alone.
Fresh identity checks refuse a removed or replaced source; cancel and press
`R` to review the current entry. Other failures retain Preview for retry.

After success, Inbox refreshes, clears the consumed mark, and highlights the
entry near the previous index. A refresh error still reports **Sent to Trash**,
removes the known-trashed row, and warns that other rows may be stale. Press `R`
to retry the refresh. Recovery is through your file manager's Trash view;
Downloads Janitor has no restore-from-Trash or undo command.

Storage follows the [freedesktop Trash specification](https://specifications.freedesktop.org/trash/latest/):
`$XDG_DATA_HOME/Trash`, defaulting to `$HOME/.local/share/Trash` when the override
is absent, empty, or relative. The `files` payload has matching `.trashinfo`
metadata in `info`, recording the percent-encoded original absolute path and
local deletion time. Names are reserved exclusively, and prior payloads are
never overwritten. Metadata is written and flushed before moving the entry.

This implementation supports Linux filesystems with atomic no-replace rename,
with source and home Trash on the same filesystem. It does not use per-mount
Trash, copy across filesystems, or fall back to permanent deletion. Trash,
`files`, and `info` must be private directories owned by the current user,
without symlinks; missing directories are created on execution. It refuses
trashing Trash itself, its ancestors, or its contents.

Failure can leave newly created storage directories. If reserved metadata cannot
be cleaned up after a failed move, the error identifies the orphan metadata
path. Identity and directory checks have the same userspace race limits as
moves; this is not a transaction against concurrent filesystem changes. A crash
between metadata creation and rename can leave orphan metadata, and the two
records have no crash-atomic or power-loss durability guarantee. No recursive
copy or deletion occurs during this action.

## Ignore and restore entries

Press `i` in Inbox to persistently hide marked entries, or the highlighted entry
when nothing is marked. No confirmation is needed: ignoring changes visibility
and saved state, without moving or deleting the entries. Successful ignore
clears the affected marks and keeps the cursor near its former numeric position.

Press uppercase `I` to switch to **Ignored Entries**. This view supports the same
navigation, Space, visual ranges, select-all, clear, and refresh controls.
Press `u` to restore marked entries, or the highlighted entry. Restore changes
saved state and returns entries to the normal Inbox; use `I` to return there
for move or rename actions. Move, rename, Trash, permanent deletion, and ignore bindings are disabled in
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
merge or collision resolution,
undo or rollback, queues, configuration or rules, bulk rename, recursive Inbox
scanning or organization, filesystem watching, timers, or background work.
Explicit permanent deletion of a directory does recursively remove its contents.

## Roadmap

Milestones 1 through 4 are implemented and verified. Milestone 4 delivers
efficient manual Inbox processing; configuration and rules remain Milestone 5,
and history/undo remain Milestone 6. Both later milestones are proposed directions.

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
explicit refresh, persistent ignore/restore, bulk moves, and individual desktop
Trash and confirmed permanent deletion (individual and bulk) are implemented.
The [mixed-entry acceptance scenario](docs/milestone-4-acceptance.md) records
the verified workflow, expected filesystem results, and remaining limitations.

### Milestone 5 — Configuration and Rules

The fifth milestone will introduce user-controlled Configuration, Favorite
Destinations, and deterministic organization Rules. Rules will suggest a
Destination for review but never execute moves automatically. They will use
case-sensitive basename globs and an explicit Any/File/Directory/Symlink kind
filter; the first matching Rule wins, and unmatched or non-Unicode entries stay
manual. Favorites must name Destinations beneath `$HOME`; stale Favorites are
retained and reported. A dedicated TUI Configuration screen will manage the
ordered Rules and Favorites, while invalid Configuration remains preserved for
repair and manual Inbox actions remain available. Bulk moves continue to use one
shared Destination.

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

For a disposable terminal walkthrough, fixture setup, and recorded results, see
[Milestone 4 acceptance](docs/milestone-4-acceptance.md). The combined workflow
regression test can also be run alone:

```bash
cargo test milestone_four_mixed_workflow_survives_actions_and_restart
```
