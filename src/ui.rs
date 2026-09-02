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
        Screen::MovePreview => render_preview(frame, app),
    }
}

fn render_inbox(frame: &mut Frame<'_>, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(format!("~/Downloads    {} entries", app.entries().len())).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Downloads Janitor"),
        ),
        areas[0],
    );

    let items = app
        .entries()
        .iter()
        .map(|entry| ListItem::new(entry.display_name()))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT))
        .highlight_symbol("> ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    let mut list_state = ListState::default();
    list_state.select(app.selected());
    frame.render_stateful_widget(list, areas[1], &mut list_state);

    frame.render_widget(
        Paragraph::new("j/k or ↑/↓ Navigate    Enter Choose    q Quit")
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
            "{error}j/k Navigate  Enter/l Open  h/Backspace Parent  d Choose  Esc Back  q Quit"
        ))
        .alignment(Alignment::Right)
        .block(Block::default().borders(Borders::ALL)),
        areas[2],
    );
}

fn render_preview(frame: &mut Frame<'_>, app: &App) {
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
            Span::raw(proposal.source().to_string_lossy()),
        ]),
        Line::from(vec![
            Span::styled(
                "Destination: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(proposal.destination().to_string_lossy()),
        ]),
        Line::from(vec![
            Span::styled("To:   ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(proposal.resulting_path().to_string_lossy()),
        ]),
        Line::default(),
    ];
    if proposal.is_valid() {
        lines.push(Line::from(Span::styled(
            "Preview only — no files will be changed",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
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
    lines.push(Line::default());
    lines.push(Line::from("Esc Back    q Quit"));
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Move Preview")),
        frame.area(),
    );
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
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
        for _ in 0..19 {
            press(&mut app, KeyCode::Down);
        }

        let output = rendered(&app, 80, 10);

        assert!(output.contains("> directory-19/"));
        assert!(!output.contains("directory-00/"));
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
        assert!(valid.contains("Preview only — no files will be changed"));
        assert!(valid.contains(source.to_string_lossy().as_ref()));
        assert!(valid.contains(destination.join("source.txt").to_string_lossy().as_ref()));
        assert!(valid.contains("Esc Back    q Quit"));
        assert!(!valid.contains("d Choose"));

        press(&mut app, KeyCode::Esc);
        fs::remove_file(&source).unwrap();
        press(&mut app, KeyCode::Char('d'));
        let invalid = rendered(&app, 100, 16);
        assert!(invalid.contains("Invalid proposal"));
        assert!(invalid.contains("source no longer exists"));
        assert!(!invalid.contains("no files will be changed"));
    }
}
