# Downloads Janitor

Downloads Janitor is a keyboard-driven Linux terminal application for reviewing
the files that accumulate in `~/Downloads`.

Milestone 1 provides a safe, read-only inbox view. It displays the immediate
files and directories in `$HOME/Downloads`, lists directories first with a `/`
suffix, and lets you move through every entry without wrapping. Longer lists
scroll to keep the selected entry visible.

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

## Keybindings

| Key | Action |
| --- | --- |
| `j` or Down Arrow | Select the next entry |
| `k` or Up Arrow | Select the previous entry |
| `q` | Quit and restore the terminal |

Selection stops at the first and final entries; it does not wrap around.

## Milestone 1 scope

Milestone 1 is strictly read-only. It scans only the immediate entries in
`$HOME/Downloads`: subdirectories are shown but never traversed.

It does not move, rename, delete, preview, categorize, or otherwise organize
files. Those organization features are reserved for later milestones. It also
does not include configuration, persistence, recursive scanning, filesystem
watching, network access, or a background service.
