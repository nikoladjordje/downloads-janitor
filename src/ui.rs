use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{App, Screen},
    destination::DestinationEntryKind,
};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    match app.screen() {
        Screen::Inbox => render_inbox(frame, app),
        Screen::DestinationBrowser => render_destination(frame, app),
        Screen::MovePreview | Screen::RenamePreview => render_preview(frame, app),
        Screen::RenameEditor | Screen::MoveNameEditor => render_rename_editor(frame, app),
    }
}

fn render_inbox(frame: &mut Frame<'_>, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if app.ignored_warning().is_some() {
                6
            } else if app.notice().is_some() {
                4
            } else {
                3
            }),
            Constraint::Fill(1),
            Constraint::Length(5),
        ])
        .split(frame.area());

    let notice = app
        .notice()
        .map(|notice| format!("\n{notice}"))
        .unwrap_or_default();
    let warning = app
        .ignored_warning()
        .map(|warning| format!("\n{warning}"))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(format!(
            "{}    {} entries    {} marked{}{notice}{warning}",
            if app.viewing_ignored() {
                "Ignored Entries"
            } else {
                "~/Downloads"
            },
            app.entries().len(),
            app.marked_count(),
            if app.visual_selection() {
                "    VISUAL"
            } else {
                ""
            }
        ))
        .wrap(ratatui::widgets::Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Downloads Janitor"),
        ),
        areas[0],
    );

    let items = app
        .entries()
        .iter()
        .map(|entry| {
            ListItem::new(format!(
                "{}{}",
                entry.display_name(),
                if app.marked(entry) { " [x]" } else { "" }
            ))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut list_state = ListState::default();
    list_state.select(app.selected());
    frame.render_stateful_widget(list, areas[1], &mut list_state);

    frame.render_widget(
        Paragraph::new(if app.viewing_ignored() {
            "j/k ↑/↓ Navigate  gg Top  G Bottom  u Restore  I Inbox  q Quit\nSpace Mark  V Visual  a All  c Clear  Esc Clear\nR Refresh"
        } else {
            "j/k ↑/↓ Navigate  gg Top  G Bottom  Enter Choose  r Rename  q Quit\nSpace Mark  V Visual  a All  c Clear  Esc Clear\ni Ignore  I Ignored entries  R Refresh"
        })
        .alignment(Alignment::Right)
        .block(Block::default().borders(Borders::ALL)),
        areas[2],
    );
}

fn render_destination(frame: &mut Frame<'_>, app: &App) {
    let destination = app
        .destination()
        .expect("destination screen always has a destination");
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(4),
        ])
        .split(frame.area());
    frame.render_widget(
        Paragraph::new(destination.to_string_lossy()).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Destination Browser"),
        ),
        areas[0],
    );
    let items = app
        .destination_entries()
        .iter()
        .map(|entry| {
            let style = match entry.kind() {
                DestinationEntryKind::Parent => Style::default().fg(Color::Cyan),
                DestinationEntryKind::Directory => Style::default(),
                DestinationEntryKind::DisabledSymlink => Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            };
            ListItem::new(entry.display_name()).style(style)
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut list_state = ListState::default();
    list_state.select(app.destination_selected());
    frame.render_stateful_widget(list, areas[1], &mut list_state);
    let error = app
        .destination_error()
        .map(|error| format!("Error: {error}\n"))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(format!(
            "{error}j/k Navigate  gg Top  G Bottom  Enter/l Open  h/Backspace Parent  d Choose  Esc Back  q Quit"
        ))
        .alignment(Alignment::Right)
        .block(Block::default().borders(Borders::ALL)),
        areas[2],
    );
}

fn render_rename_editor(frame: &mut Frame<'_>, app: &App) {
    let name = app.rename_name().expect("rename editor has a basename");
    let source = app.entries()[app.selected().expect("rename has a selection")].path();
    let mut lines = vec![
        Line::from(format!("From: {source:?}")),
        Line::from(format!("Basename: {name:?}")),
        Line::from("Type to append; Backspace removes the last character; Ctrl+u clears."),
        Line::from("Quotes and escapes show exact filename bytes; they are not added to the name."),
        Line::default(),
        Line::from("Enter Review    Esc Cancel"),
    ];
    if let Some(error) = app.move_error() {
        lines.push(Line::from(Span::styled(
            error,
            Style::default().fg(Color::Red),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(
                if app.screen() == Screen::MoveNameEditor {
                    "Edit Move Basename"
                } else {
                    "Rename Entry"
                },
            )),
        frame.area(),
    );
}

fn render_preview(frame: &mut Frame<'_>, app: &App) {
    let renaming = app.screen() == Screen::RenamePreview;
    let proposal = app
        .proposed_move()
        .expect("preview screen always has a proposed move");
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{:?}", proposal.entry_type())),
        ]),
        Line::from(vec![
            Span::styled("From: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{:?}", proposal.source())),
        ]),
        Line::from(vec![
            Span::styled(
                "Destination: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{:?}", proposal.destination())),
        ]),
        Line::from(vec![
            Span::styled("To:   ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{:?}", proposal.resulting_path())),
        ]),
        Line::default(),
    ];
    if proposal.is_valid() {
        lines.push(Line::from(Span::styled(
            "Warning: pressing Enter will change the filesystem",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Invalid proposal",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        lines.extend(
            proposal
                .failures()
                .iter()
                .map(|failure| Line::from(format!("- {failure}"))),
        );
    }
    if let Some(error) = app.move_error() {
        lines.push(Line::from(Span::styled(
            format!(
                "{} failed: {error}",
                if renaming { "Rename" } else { "Move" }
            ),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(match (renaming, proposal.is_valid()) {
        (true, true) => "Enter Rename    Esc Cancel    q Quit",
        (true, false) => "Esc Cancel    q Quit",
        (false, true) => "Enter Move    r Edit name    Esc Back    q Quit",
        (false, false) => "r Edit name    Esc Back    q Quit",
    }));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(if renaming {
                "Rename Preview"
            } else {
                "Move Preview"
            })),
        frame.area(),
    );
}

#[cfg(test)]
mod tests {
    use std::{
        fs, io,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    use crate::{
        app::App,
        inbox::{EntryKind, InboxEntry},
    };

    use super::render;

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "downloads-janitor-ui-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn rendered(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn destination_render_shows_path_rows_disabled_links_and_controls() {
        let root = TestDirectory::new();
        let directory = root.0.join("eligible");
        fs::create_dir(&directory).unwrap();
        let source = root.0.join("source.txt");
        fs::File::create(&source).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&directory, root.0.join("linked-directory")).unwrap();
        let entry = InboxEntry::test_entry(source, EntryKind::File, false);
        let mut app = App::new(vec![entry], root.0.clone());
        press(&mut app, KeyCode::Enter);

        let output = rendered(&app, 100, 14);

        assert!(output.contains(root.0.to_string_lossy().as_ref()));
        assert!(output.contains("> eligible/"));
        #[cfg(unix)]
        assert!(output.contains("linked-directory@ (disabled)"));
        assert!(output.contains("Enter/l Open"));
        assert!(output.contains("d Choose"));
    }

    #[test]
    fn destination_render_scrolls_to_keep_the_selection_visible() {
        let root = TestDirectory::new();
        for index in 0..20 {
            fs::create_dir(root.0.join(format!("directory-{index:02}"))).unwrap();
        }
        let source = root.0.join("source.txt");
        fs::File::create(&source).unwrap();
        let entry = InboxEntry::test_entry(source, EntryKind::File, false);
        let mut app = App::new(vec![entry], root.0.clone());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('G'));

        let output = rendered(&app, 80, 10);

        assert!(output.contains("> directory-19/"));
        assert!(!output.contains("directory-00/"));
    }

    #[test]
    fn inbox_render_scrolls_to_vim_jump_target() {
        let entries = (0..20)
            .map(|index| InboxEntry::test_file(&format!("entry-{index:02}")))
            .collect();
        let mut app = App::new(entries, "/home/tester".into());
        press(&mut app, KeyCode::Char('G'));

        let output = rendered(&app, 100, 10);

        assert!(output.contains("> entry-19"));
        assert!(!output.contains("entry-00"));
    }

    #[test]
    fn list_footers_advertise_vim_jumps_and_preview_does_not() {
        let root = TestDirectory::new();
        let source = root.0.join("source.txt");
        fs::File::create(&source).unwrap();
        let entry = InboxEntry::test_entry(source, EntryKind::File, false);
        let mut app = App::new(vec![entry], root.0.clone());

        let inbox = rendered(&app, 140, 10);
        assert!(inbox.contains("gg Top"));
        assert!(inbox.contains("G Bottom"));

        press(&mut app, KeyCode::Enter);
        let destination = rendered(&app, 140, 10);
        assert!(destination.contains("gg Top"));
        assert!(destination.contains("G Bottom"));

        press(&mut app, KeyCode::Char('d'));
        let preview = rendered(&app, 140, 10);
        assert!(!preview.contains("gg Top"));
        assert!(!preview.contains("G Bottom"));
    }

    #[test]
    fn preview_render_distinguishes_valid_and_invalid_proposals() {
        let root = TestDirectory::new();
        let destination = root.0.join("destination");
        let source = root.0.join("source.txt");
        fs::create_dir(&destination).unwrap();
        fs::File::create(&source).unwrap();
        let entry = InboxEntry::test_entry(source.clone(), EntryKind::File, false);
        let mut app = App::new(vec![entry], root.0.clone());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));

        let valid = rendered(&app, 100, 16);
        assert!(valid.contains("Warning: pressing Enter will change the filesystem"));
        assert!(valid.contains(source.to_string_lossy().as_ref()));
        assert!(valid.contains(destination.join("source.txt").to_string_lossy().as_ref()));
        assert!(valid.contains("Esc Back    q Quit"));
        assert!(valid.contains("Enter Move"));
        assert!(!valid.contains("m Move"));
        assert!(!valid.contains("d Choose"));

        app.handle_event(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )));
        assert_eq!(rendered(&app, 100, 16), valid);
        assert!(source.exists());
        assert!(!destination.join("source.txt").exists());

        press(&mut app, KeyCode::Esc);
        assert!(rendered(&app, 100, 16).contains("Destination Browser"));
        assert!(source.exists());
        fs::remove_file(&source).unwrap();
        press(&mut app, KeyCode::Char('d'));
        let invalid = rendered(&app, 100, 16);
        assert!(invalid.contains("Invalid proposal"));
        assert!(invalid.contains("source no longer exists"));
        assert!(!invalid.contains("will change the filesystem"));
        assert!(!invalid.contains("Enter Move"));
    }

    #[test]
    fn preview_renders_move_failure_with_preserved_paths() {
        let root = TestDirectory::new();
        let destination = root.0.join("destination");
        let source = root.0.join("source.txt");
        fs::create_dir(&destination).unwrap();
        fs::write(&source, b"source").unwrap();
        let entry = InboxEntry::test_entry(source.clone(), EntryKind::File, false);
        let mut app = App::new(vec![entry], root.0.clone());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        fs::write(destination.join("source.txt"), b"collision").unwrap();
        press(&mut app, KeyCode::Enter);

        let output = rendered(&app, 100, 14);

        assert!(output.contains("Move failed: fresh validation failed"));
        assert!(output.contains(source.to_string_lossy().as_ref()));
        assert!(output.contains(destination.join("source.txt").to_string_lossy().as_ref()));
        assert!(output.contains("Enter Move    r Edit name    Esc Back    q Quit"));
    }

    fn fail_refresh(_: &Path) -> crate::Result<Vec<InboxEntry>> {
        Err(io::Error::other("injected refresh failure").into())
    }

    #[test]
    fn inbox_renders_success_and_stale_warning_after_refresh_failure() {
        let root = TestDirectory::new();
        let downloads = root.0.join("Downloads");
        let source = downloads.join("source.txt");
        fs::create_dir(&downloads).unwrap();
        fs::write(&source, b"source").unwrap();
        let entry = InboxEntry::test_entry(source, EntryKind::File, false);
        let mut app = App::with_inbox_scanner(vec![entry], root.0.clone(), fail_refresh);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Enter);

        let output = rendered(&app, 120, 12);

        assert!(output.contains("Move completed successfully"));
        assert!(output.contains("Inbox refresh failed: injected refresh failure"));
        assert!(output.contains("Remaining entries may be stale"));
        assert!(!output.contains("Move failed"));
    }
    #[test]
    fn rename_screens_show_exact_names_review_warning_and_failure() {
        use std::os::unix::ffi::OsStrExt;
        let root = TestDirectory::new();
        let downloads = root.0.join("Downloads");
        fs::create_dir(&downloads).unwrap();
        let name = std::ffi::OsStr::from_bytes(b"raw-\xff");
        let source = downloads.join(name);
        fs::write(&source, b"source").unwrap();
        let mut app = App::new(
            crate::inbox::scan_inbox(&downloads).unwrap(),
            root.0.clone(),
        );
        assert!(rendered(&app, 100, 14).contains("r Rename"));

        press(&mut app, KeyCode::Char('r'));
        let editor = rendered(&app, 120, 16);
        assert!(editor.contains("Rename Entry"));
        assert!(editor.contains(&format!("Basename: {name:?}")));
        assert!(editor.contains("Enter Review    Esc Cancel"));
        assert!(!editor.contains("q Quit"));

        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Enter);
        let preview = rendered(&app, 120, 16);
        let result = downloads.join(std::ffi::OsStr::from_bytes(b"raw-\xffq"));
        assert!(preview.contains("Rename Preview"));
        assert!(preview.contains(&format!("From: {source:?}")));
        assert!(preview.contains(&format!("To:   {result:?}")));
        assert!(preview.contains("Warning: pressing Enter will change the filesystem"));
        assert!(preview.contains("Enter Rename    Esc Cancel"));
        assert!(source.exists());
        assert!(!result.exists());

        fs::write(&result, b"collision").unwrap();
        press(&mut app, KeyCode::Enter);
        let failed = rendered(&app, 160, 16);
        assert!(failed.contains("Rename failed: fresh validation failed"));
        assert!(failed.contains(&format!("From: {source:?}")));
        assert!(failed.contains(&format!("To:   {result:?}")));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Enter);
        let unchanged = rendered(&app, 120, 16);
        assert!(unchanged.contains("Invalid proposal"));
        assert!(!unchanged.contains("Enter Rename"));
        assert!(!unchanged.contains("will change the filesystem"));
    }
    #[test]
    fn move_preview_edits_and_displays_exact_resulting_filename() {
        use std::os::unix::ffi::OsStrExt;
        let root = TestDirectory::new();
        let downloads = root.0.join("Downloads");
        fs::create_dir(&downloads).unwrap();
        let source = downloads.join(std::ffi::OsStr::from_bytes(b"raw-\xff"));
        fs::write(&source, b"source").unwrap();
        let mut app = App::new(
            crate::inbox::scan_inbox(&downloads).unwrap(),
            root.0.clone(),
        );
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert!(rendered(&app, 140, 16).contains("r Edit name"));
        press(&mut app, KeyCode::Char('r'));
        let editor = rendered(&app, 140, 16);
        assert!(editor.contains("Edit Move Basename"));
        assert!(editor.contains(&format!("Basename: {:?}", source.file_name().unwrap())));
        assert!(!editor.contains("q Quit"));

        press(&mut app, KeyCode::Char('é'));
        press(&mut app, KeyCode::Enter);
        let mut name = source.file_name().unwrap().to_os_string();
        name.push("é");
        let result = root.0.join(name);
        let preview = rendered(&app, 140, 16);
        assert!(preview.contains("Move Preview"));
        assert!(preview.contains(&format!("From: {source:?}")));
        assert!(preview.contains(&format!("Destination: {:?}", root.0)));
        assert!(preview.contains(&format!("To:   {result:?}")));
        assert!(preview.contains("Enter Move    r Edit name"));
        assert!(source.exists());
        assert!(!result.exists());
        press(&mut app, KeyCode::Enter);
        assert_eq!(fs::read(result).unwrap(), b"source");
        assert!(!source.exists());
    }
    #[test]
    fn inbox_renders_marks_cursor_visual_mode_count_and_selection_controls() {
        let root = TestDirectory::new();
        let downloads = root.0.join("Downloads");
        fs::create_dir(&downloads).unwrap();
        fs::write(downloads.join("alpha"), b"a").unwrap();
        fs::write(downloads.join("beta"), b"b").unwrap();
        let mut app = App::new(
            crate::inbox::scan_inbox(&downloads).unwrap(),
            root.0.clone(),
        );
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Down);
        let output = rendered(&app, 100, 14);
        assert!(output.contains("alpha [x]"));
        assert!(output.contains("> beta"));
        assert!(!output.contains("beta [x]"));
        assert!(output.contains("1 marked"));
        for control in [
            "Space Mark",
            "V Visual",
            "a All",
            "c Clear",
            "Esc Clear",
            "R Refresh",
        ] {
            assert!(output.contains(control));
        }
        press(&mut app, KeyCode::Char('V'));
        let output = rendered(&app, 100, 14);
        assert!(output.contains("VISUAL"));
        assert!(output.contains("2 marked"));
        assert!(output.contains("> beta [x]"));
        press(&mut app, KeyCode::Esc);
        let output = rendered(&app, 100, 14);
        assert!(output.contains("0 marked"));
        assert!(!output.contains("VISUAL"));
        assert!(!output.contains("[x]"));
        app = App::with_inbox_scanner(
            crate::inbox::scan_inbox(&downloads).unwrap(),
            root.0.clone(),
            fail_refresh,
        );
        press(&mut app, KeyCode::Char('R'));
        let output = rendered(&app, 100, 14);
        assert!(output.contains("Inbox refresh failed; entries may be stale"));
        assert!(output.contains("> alpha"));
    }
    #[test]
    fn ignored_view_advertises_only_restore_and_unreadable_state_warning_persists() {
        let root = TestDirectory::new();
        let downloads = root.0.join("Downloads");
        fs::create_dir(&downloads).unwrap();
        fs::write(downloads.join("entry"), b"source").unwrap();
        let mut app = App::new(
            crate::inbox::scan_inbox(&downloads).unwrap(),
            root.0.clone(),
        );
        let inbox = rendered(&app, 120, 16);
        assert!(inbox.contains("i Ignore"));
        assert!(inbox.contains("I Ignored entries"));
        press(&mut app, KeyCode::Char('i'));
        press(&mut app, KeyCode::Char('I'));
        let ignored = rendered(&app, 120, 16);
        assert!(ignored.contains("Ignored Entries"));
        assert!(ignored.contains("> entry"));
        assert!(ignored.contains("u Restore"));
        assert!(ignored.contains("Space Mark"));
        assert!(!ignored.contains("Enter Choose"));
        assert!(!ignored.contains("r Rename"));
        assert!(!ignored.contains("i Ignore"));
        press(&mut app, KeyCode::Char('u'));
        assert!(app.entries().is_empty());
        press(&mut app, KeyCode::Char('I'));
        assert!(rendered(&app, 120, 16).contains("> entry"));

        fs::write(
            root.0.join(".local/state/downloads-janitor/ignored-v1"),
            b"corrupt",
        )
        .unwrap();
        press(&mut app, KeyCode::Char('R'));
        press(&mut app, KeyCode::Down);
        let warning = rendered(&app, 120, 18);
        assert!(warning.contains("Ignored state unreadable; showing all entries"));
        assert!(warning.contains("> entry"));
    }
}
