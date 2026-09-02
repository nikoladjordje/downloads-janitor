# Downloads Janitor

Downloads Janitor is a keyboard-driven Linux terminal application for reviewing
entries in `~/Downloads` and previewing where one entry could be placed. The
Milestone 2 workflow is strictly read-only: it never moves, renames, deletes,
queues, or persists anything.

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

## Verification

Run the reproducible automated checks from the repository root:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
