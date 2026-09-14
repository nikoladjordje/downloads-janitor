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
        Screen::TrashPreview => render_trash(frame, app),
        Screen::DeleteConfirmation => render_delete(frame, app),
        Screen::BulkPreview | Screen::BulkProgress | Screen::BulkResult => render_batch(frame, app),
        Screen::DestinationBrowser | Screen::FavoriteDestinationBrowser => {
            render_destination(frame, app)
        }
        Screen::MovePreview | Screen::RenamePreview => render_preview(frame, app),
        Screen::RenameEditor | Screen::MoveNameEditor => render_rename_editor(frame, app),
        Screen::Configuration => render_configuration(frame, app),
        Screen::FavoriteNameEditor => render_favorite_name_editor(frame, app),
    }
}

fn render_delete(frame: &mut Frame<'_>, app: &App) {
    let areas = Layout::vertical([
        Constraint::Length(5),
        Constraint::Fill(1),
        Constraint::Length(5),
    ])
    .split(frame.area());
    frame.render_widget(Paragraph::new("Permanently delete 1 entry. Bypasses Trash.\nRemoves ALL contents of selected directories. Cannot be undone.")
        .style(Style::default().fg(Color::Red))
        .wrap(ratatui::widgets::Wrap { trim: false })
        .block(Block::bordered().title("Permanent Deletion Confirmation")), areas[0]);
    let review = app
        .delete_review
        .as_ref()
        .expect("Delete confirmation has a review");
    let content = format!(
        "Source: {:?}\n{}",
        review.source,
        app.move_error().unwrap_or("No deletion authorized")
    );
    let max_scroll = content
        .lines()
        .count()
        .saturating_sub(usize::from(areas[1].height.saturating_sub(2)));
    frame.render_widget(
        Paragraph::new(content)
            .scroll((
                app.batch_scroll.0.min(max_scroll) as u16,
                app.batch_scroll.1,
            ))
            .block(Block::bordered()),
        areas[1],
    );
    frame.render_widget(Paragraph::new(format!("Type exactly delete, then Enter: {:?}\nEsc Cancel; Backspace Edit; Ctrl+u Clear\nArrows Scroll/pan path and errors; Home Start", app.delete_confirmation))
        .wrap(ratatui::widgets::Wrap { trim: false }).block(Block::bordered()), areas[2]);
}

fn render_trash(frame: &mut Frame<'_>, app: &App) {
    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(4),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new("Send 1 entry to desktop Trash; recover through your file manager")
            .block(Block::bordered().title("Trash Preview")),
        areas[0],
    );
    let review = app
        .trash_review
        .as_ref()
        .expect("Trash preview has a review");
    let content = format!(
        "Source: {:?}\nAction: Send to Trash (including directory contents)\n{}",
        review.source,
        app.move_error().unwrap_or("Ready for review")
    );
    let max_scroll = content
        .lines()
        .count()
        .saturating_sub(usize::from(areas[1].height.saturating_sub(2)));
    frame.render_widget(
        Paragraph::new(content)
            .scroll((
                app.batch_scroll.0.min(max_scroll) as u16,
                app.batch_scroll.1,
            ))
            .block(Block::bordered()),
        areas[1],
    );
    frame.render_widget(Paragraph::new("Enter Send to Trash (changes filesystem)    Esc Cancel    q Quit\nj/k Scroll; h/l Pan exact path and errors; Home Start")
        .wrap(ratatui::widgets::Wrap { trim: false }).block(Block::bordered()), areas[2]);
}

pub(crate) fn batch_lines(app: &App) -> Vec<String> {
    let batch = app.batch().expect("batch screen has a batch");
    let mut lines = Vec::new();
    for (index, item) in batch.entries.iter().enumerate() {
        let status = match &item.outcome {
            crate::batch::EntryOutcome::Completed => "Completed".to_owned(),
            crate::batch::EntryOutcome::Failed(error) => format!("Failed: {error}"),
            crate::batch::EntryOutcome::Unattempted => if app.screen() == Screen::BulkPreview {
                if item.problems.is_empty() {
                    "Valid"
                } else {
                    "Blocked"
                }
            } else {
                "Unattempted"
            }
            .to_owned(),
        };
        lines.push(format!("{}. {status}", index + 1));
        lines.push(format!("From: {:?}", item.entry.path()));
        if let Some(proposal) = &item.proposal {
            lines.push(format!("To:   {:?}", proposal.resulting_path()));
        } else {
            lines.push(
                match batch.action {
                    crate::batch::BatchAction::Trash(_) => "Action: Send to desktop Trash",
                    _ => "Action: Permanently delete; directory contents included",
                }
                .to_owned(),
            );
        }
        lines.extend(item.problems.iter().map(|error| format!("  {error}")));
        lines.push(String::new());
    }
    lines
}

fn render_batch(frame: &mut Frame<'_>, app: &App) {
    let batch = app.batch().expect("batch exists");
    let (title, guidance) = match (&batch.action, app.screen()) {
        (crate::batch::BatchAction::Trash(_), Screen::BulkPreview) => (
            "Bulk Trash Preview",
            "Enter rechecks all and sends to Trash if valid. Esc Cancel; q Quit",
        ),
        (crate::batch::BatchAction::Delete, Screen::BulkPreview) => (
            "Bulk Permanent Deletion Confirmation",
            "Bypasses Trash; removes ALL directory contents. Cannot be undone. Esc Cancel",
        ),
        (crate::batch::BatchAction::Trash(_), Screen::BulkProgress) => (
            "Trashing Entries",
            "Esc stops before next entry; completed entries stay in Trash",
        ),
        (crate::batch::BatchAction::Delete, Screen::BulkProgress) => (
            "Permanently Deleting Entries",
            "Esc stops before next entry; completed deletions cannot be undone",
        ),
        (crate::batch::BatchAction::Trash(_), _) => (
            "Bulk Trash Results",
            "Enter/Esc Inbox; q Quit. Restore completed entries through your file manager",
        ),
        (crate::batch::BatchAction::Delete, _) => (
            "Bulk Permanent Deletion Results",
            "Enter/Esc Inbox; q Quit. Completed deletions cannot be replayed or undone",
        ),
        (_, Screen::BulkPreview) => (
            "Bulk Move Preview",
            if batch.valid() {
                "Enter moves the entire reviewed set and changes the filesystem. Esc Back; q Quit"
            } else {
                "Blocked. Enter rechecks all and moves if valid (changes filesystem). Esc Back; q Quit"
            },
        ),
        (_, Screen::BulkProgress) => (
            "Moving Entries",
            "Esc Stop before the next entry; completed moves are kept",
        ),
        _ => (
            "Bulk Move Results",
            "Enter/Esc Inbox; q Quit. Completed moves cannot be replayed",
        ),
    };
    let areas = Layout::vertical([
        Constraint::Length(5),
        Constraint::Fill(1),
        Constraint::Length(
            if batch.action == crate::batch::BatchAction::Delete
                && app.screen() == Screen::BulkPreview
            {
                7
            } else {
                4
            },
        ),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new(format!(
            "{} entries; {}\n{}",
            batch.entries.len(),
            batch.summary(),
            app.notice()
                .unwrap_or(if batch.action == crate::batch::BatchAction::Move {
                    "Basenames preserved; execution order: source path bytes"
                } else {
                    "Execution order: source path bytes; stop on first failure, no rollback"
                })
        ))
        .wrap(ratatui::widgets::Wrap { trim: false })
        .block(Block::bordered().title(title)),
        areas[0],
    );
    let lines = batch_lines(app);
    let max_scroll = lines
        .len()
        .saturating_sub(usize::from(areas[1].height.saturating_sub(2)));
    let scroll = if app.screen() == Screen::BulkProgress {
        batch
            .entries
            .iter()
            .take_while(|item| item.outcome == crate::batch::EntryOutcome::Completed)
            .count()
            * 4
    } else {
        app.batch_scroll.0
    }
    .min(max_scroll);
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .skip(scroll)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .scroll((0, app.batch_scroll.1))
        .block(Block::bordered()),
        areas[1],
    );
    let controls = if batch.action == crate::batch::BatchAction::Delete
        && app.screen() == Screen::BulkPreview
    {
        format!(
            "Type exactly delete, then Enter: {:?}\nBackspace Edit; Ctrl+u Clear; Arrows Scroll/pan; PgUp/PgDn Page; Home Start",
            app.delete_confirmation
        )
    } else {
        "j/k ↑/↓ Scroll; PgUp/PgDn Page; h/l ←/→ Pan paths; Home Start".into()
    };
    frame.render_widget(
        Paragraph::new(format!("{guidance}\n{controls}"))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(Block::bordered()),
        areas[2],
    );
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
            "j/k ↑/↓ Navigate  gg Top  G Bottom  Enter Choose  r Rename  t Trash  q Quit\nSpace Mark  V Visual  a All  c Clear  Esc Clear\ni Ignore  I Ignored entries  C Configuration  R Refresh  D Delete permanently"
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
            Block::default().borders(Borders::ALL).title(
                if app.screen() == Screen::FavoriteDestinationBrowser {
                    "Choose Favorite Destination"
                } else {
                    "Destination Browser"
                },
            ),
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
            "{error}j/k Navigate  gg Top  G Bottom  Enter/l Open  h/Backspace Parent  d {}  Esc Back  q Quit",
            if app.screen() == Screen::FavoriteDestinationBrowser { "Save" } else { "Choose" },
        ))
        .alignment(Alignment::Right)
        .block(Block::default().borders(Borders::ALL)),
        areas[2],
    );
}

fn render_configuration(frame: &mut Frame<'_>, app: &App) {
    let areas = Layout::vertical([
        Constraint::Length(
            if app.favorites_warning().is_some() || app.notice().is_some() {
                5
            } else {
                3
            },
        ),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .split(frame.area());
    let status = app
        .favorites_warning()
        .or(app.notice())
        .map(|message| format!("\n{message}"))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(format!(
            "{} Favorite Destinations{status}",
            app.favorites().len()
        ))
        .wrap(ratatui::widgets::Wrap { trim: false })
        .block(Block::bordered().title("Configuration")),
        areas[0],
    );
    let items = app
        .favorites()
        .iter()
        .map(|favorite| {
            let available = app.favorites_warning().is_none() && app.favorite_available(favorite);
            let suffix = if available { "" } else { " (unavailable)" };
            ListItem::new(format!(
                "{}: {:?}{suffix}",
                favorite.name(),
                favorite.path()
            ))
            .style(if available {
                Style::default()
            } else {
                Style::default().fg(Color::Yellow)
            })
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut state = ListState::default();
    state.select(app.favorite_selected());
    frame.render_stateful_widget(list, areas[1], &mut state);
    frame.render_widget(
        Paragraph::new(
            "j/k ↑/↓ Navigate  gg Top  G Bottom  a Add  Enter/e Edit  x Delete  Esc Inbox  q Quit",
        )
        .alignment(Alignment::Right)
        .block(Block::bordered()),
        areas[2],
    );
}

fn render_favorite_name_editor(frame: &mut Frame<'_>, app: &App) {
    let name = app.favorite_name().expect("favorite editor has a name");
    let mut lines = vec![
        Line::from(format!("Name: {name:?}")),
        Line::from("Type a unique, case-sensitive Favorite name."),
        Line::from("Backspace removes the last character; Ctrl+u clears."),
        Line::default(),
        Line::from("Enter Choose Destination    Esc Cancel"),
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
            .block(Block::bordered().title("Favorite Destination Name")),
        frame.area(),
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

    #[test]
    fn delete_confirmation_renders_exact_path_warning_and_empty_consent() {
        let fixture = TestDirectory::new();
        fs::create_dir(fixture.0.join("Downloads")).unwrap();
        let source = fixture.0.join("Downloads/review me.txt");
        fs::write(&source, b"keep").unwrap();
        let mut app = App::new(
            crate::inbox::scan_inbox(&fixture.0.join("Downloads")).unwrap(),
            fixture.0.clone(),
        );
        assert!(rendered(&app, 110, 18).contains("D Delete permanently"));
        press(&mut app, KeyCode::Char('D'));
        let output = rendered(&app, 150, 18);
        assert!(output.contains("Permanent Deletion Confirmation"));
        assert!(output.contains("Permanently delete 1 entry. Bypasses Trash."));
        assert!(output.contains("Removes ALL contents of selected directories. Cannot be undone."));
        assert!(output.contains(&format!("Source: {source:?}")));
        assert!(output.contains("Type exactly delete, then Enter: \"\""));
        assert!(output.contains("Esc Cancel"));
        press(&mut app, KeyCode::Char('q'));
        assert!(rendered(&app, 150, 18).contains("Enter: \"q\""));
        assert!(source.exists());
        press(&mut app, KeyCode::Esc);
        assert!(source.exists());
    }

    #[test]
    fn trash_preview_shows_exact_path_and_separate_authorization() {
        let fixture = TestDirectory::new();
        fs::create_dir(fixture.0.join("Downloads")).unwrap();
        let source = fixture.0.join("Downloads/review me.txt");
        fs::write(&source, b"review").unwrap();
        let mut app = App::new(
            crate::inbox::scan_inbox(&fixture.0.join("Downloads")).unwrap(),
            fixture.0.clone(),
        );
        assert!(rendered(&app, 110, 16).contains("t Trash"));
        press(&mut app, KeyCode::Char('t'));
        let output = rendered(&app, 150, 16);
        assert!(output.contains("Trash Preview"));
        assert!(output.contains(&format!("Source: {source:?}")));
        assert!(output.contains("Enter Send to Trash (changes filesystem)"));
        assert!(output.contains("Esc Cancel"));
        assert!(source.exists());
        press(&mut app, KeyCode::Esc);
        assert!(source.exists());
    }

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
    #[test]
    fn removal_review_scrolls_all_paths_and_shows_action_confirmation_and_results() {
        for delete in [false, true] {
            let root = TestDirectory::new();
            let downloads = root.0.join("Downloads");
            fs::create_dir(&downloads).unwrap();
            for index in 0..20 {
                fs::write(downloads.join(format!("entry-{index:02}")), b"data").unwrap();
            }
            let mut app = App::new(
                crate::inbox::scan_inbox(&downloads).unwrap(),
                root.0.clone(),
            );
            app.trash_root = root.0.join("Trash");
            press(&mut app, KeyCode::Char('a'));
            press(&mut app, KeyCode::Char(if delete { 'D' } else { 't' }));
            let output = rendered(&app, 150, 22);
            assert!(output.contains(if delete {
                "Bulk Permanent Deletion Confirmation"
            } else {
                "Bulk Trash Preview"
            }));
            assert!(output.contains("20 entries"));
            assert!(output.contains("entry-00"));
            if delete {
                assert!(
                    output.contains(
                        "Bypasses Trash; removes ALL directory contents. Cannot be undone."
                    )
                );
                assert!(output.contains("Type exactly delete, then Enter:"));
            } else {
                assert!(output.contains("Enter rechecks all and sends to Trash if valid"));
            }
            for _ in 0..8 {
                press(&mut app, KeyCode::PageDown);
            }
            assert!(
                rendered(&app, 150, 22)
                    .contains(&format!("From: {:?}", downloads.join("entry-19")))
            );
            press(&mut app, KeyCode::Home);
            assert!(rendered(&app, 150, 22).contains("entry-00"));
            if delete {
                for character in "delete".chars() {
                    press(&mut app, KeyCode::Char(character));
                }
            }
            press(&mut app, KeyCode::Enter);
            assert!(rendered(&app, 150, 22).contains(if delete {
                "Permanently Deleting Entries"
            } else {
                "Trashing Entries"
            }));
            press(&mut app, KeyCode::Esc);
            let output = rendered(&app, 150, 22);
            assert!(output.contains(if delete {
                "Bulk Permanent Deletion Results"
            } else {
                "Bulk Trash Results"
            }));
            assert!(output.contains("0 completed, 0 failed, 20 unattempted"));
            assert!(!app.trash_root.exists());
        }
    }

    #[test]
    fn bulk_review_scrolls_every_exact_path_and_shows_blocking_reason() {
        let root = TestDirectory::new();
        let downloads = root.0.join("Downloads");
        fs::create_dir(&downloads).unwrap();
        for index in 0..20 {
            fs::write(downloads.join(format!("entry-{index:02}")), b"data").unwrap();
        }
        fs::write(root.0.join("entry-19"), b"occupied").unwrap();
        let mut app = App::new(
            crate::inbox::scan_inbox(&downloads).unwrap(),
            root.0.clone(),
        );
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        let text = rendered(&app, 140, 18);
        assert!(text.contains("Bulk Move Preview"));
        assert!(text.contains("20 entries"));
        assert!(text.contains("entry-00"));
        assert!(!text.contains("entry-19"));
        assert!(text.contains("Blocked. Enter rechecks all and moves if valid"));
        for _ in 0..8 {
            press(&mut app, KeyCode::PageDown);
        }
        let text = rendered(&app, 140, 18);
        assert!(text.contains("20. Blocked"));
        assert!(text.contains(&format!("From: {:?}", downloads.join("entry-19"))));
        assert!(text.contains(&format!("To:   {:?}", root.0.join("entry-19"))));
        assert!(text.contains("already exists"));
        press(&mut app, KeyCode::Home);
        assert!(rendered(&app, 140, 18).contains("entry-00"));
        for _ in 0..10 {
            press(&mut app, KeyCode::Right);
        }
        assert_ne!(rendered(&app, 50, 18), {
            press(&mut app, KeyCode::Home);
            rendered(&app, 50, 18)
        });
        assert_eq!(crate::inbox::scan_inbox(&downloads).unwrap().len(), 20);
    }

    #[test]
    fn bulk_progress_and_cancelled_results_explain_outcomes() {
        let root = TestDirectory::new();
        let downloads = root.0.join("Downloads");
        fs::create_dir(&downloads).unwrap();
        for name in ["a", "b"] {
            fs::write(downloads.join(name), b"data").unwrap();
        }
        let mut app = App::new(
            crate::inbox::scan_inbox(&downloads).unwrap(),
            root.0.clone(),
        );
        for code in [
            KeyCode::Char('a'),
            KeyCode::Enter,
            KeyCode::Char('d'),
            KeyCode::Enter,
        ] {
            press(&mut app, code);
        }
        let text = rendered(&app, 140, 20);
        assert!(text.contains("Moving Entries"));
        assert!(text.contains("Esc Stop before the next entry"));
        press(&mut app, KeyCode::Esc);
        let text = rendered(&app, 140, 20);
        assert!(text.contains("Bulk Move Results"));
        assert!(text.contains("0 completed, 0 failed, 2 unattempted"));
        assert!(text.contains("stopped"));
        assert!(text.contains("Completed moves cannot be replayed"));
        assert!(text.contains("1. Unattempted"));
        assert!(text.contains("2. Unattempted"));
    }
}
