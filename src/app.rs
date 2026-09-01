use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::{Result, inbox::InboxEntry, terminal::TerminalSession, ui};

pub struct App {
    entries: Vec<InboxEntry>,
    selection: Selection,
    should_quit: bool,
}

impl App {
    pub fn new(entries: Vec<InboxEntry>) -> Self {
        let selection = Selection::new(entries.len());
        Self {
            entries,
            selection,
            should_quit: false,
        }
    }

    pub fn entries(&self) -> &[InboxEntry] {
        &self.entries
    }

    pub fn selected(&self) -> Option<usize> {
        self.selection.index()
    }

    pub fn run(mut self, terminal: &mut TerminalSession) -> Result<()> {
        while !self.should_quit {
            terminal
                .draw(|frame| ui::render(frame, &self))
                .map_err(|error| contextual_io_error("failed to render interface", error))?;

            let event = event::read()
                .map_err(|error| contextual_io_error("failed to read terminal event", error))?;
            self.handle_event(event);
        }

        Ok(())
    }

    fn handle_event(&mut self, event: Event) {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('j') | KeyCode::Down => {
                    self.selection.move_down(self.entries.len());
                }
                KeyCode::Char('k') | KeyCode::Up => self.selection.move_up(),
                _ => {}
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Selection {
    index: Option<usize>,
}

impl Selection {
    fn new(entry_count: usize) -> Self {
        Self {
            index: (entry_count > 0).then_some(0),
        }
    }

    fn index(&self) -> Option<usize> {
        self.index
    }

    fn move_down(&mut self, entry_count: usize) {
        if let Some(index) = self.index {
            self.index = Some((index + 1).min(entry_count.saturating_sub(1)));
        }
    }

    fn move_up(&mut self) {
        if let Some(index) = self.index {
            self.index = Some(index.saturating_sub(1));
        }
    }
}

fn contextual_io_error(context: &'static str, source: io::Error) -> io::Error {
    io::Error::new(source.kind(), format!("{context}: {source}"))
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use crate::inbox::InboxEntry;

    use super::{App, Selection};

    fn app_with_entries(entry_count: usize) -> App {
        App::new(
            (0..entry_count)
                .map(|index| InboxEntry::test_file(&format!("entry-{index}")))
                .collect(),
        )
    }

    #[test]
    fn q_requests_quit() {
        let mut app = App::new(Vec::new());

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )));

        assert!(app.should_quit);
    }

    #[test]
    fn other_events_do_not_request_quit() {
        let mut app = App::new(Vec::new());

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        )));

        assert!(!app.should_quit);
    }

    #[test]
    fn empty_list_has_no_selection_and_navigation_is_safe() {
        let mut selection = Selection::new(0);

        selection.move_down(0);
        selection.move_up();

        assert_eq!(selection.index(), None);
    }

    #[test]
    fn one_entry_stays_selected_for_all_navigation() {
        let mut selection = Selection::new(1);

        selection.move_down(1);
        selection.move_up();

        assert_eq!(selection.index(), Some(0));
    }

    #[test]
    fn selection_moves_down() {
        let mut selection = Selection::new(3);

        selection.move_down(3);

        assert_eq!(selection.index(), Some(1));
    }

    #[test]
    fn selection_moves_up() {
        let mut selection = Selection::new(3);
        selection.move_down(3);
        selection.move_down(3);

        selection.move_up();

        assert_eq!(selection.index(), Some(1));
    }

    #[test]
    fn selection_stops_at_upper_boundary() {
        let mut selection = Selection::new(3);

        selection.move_up();

        assert_eq!(selection.index(), Some(0));
    }

    #[test]
    fn selection_stops_at_lower_boundary() {
        let mut selection = Selection::new(3);
        selection.move_down(3);
        selection.move_down(3);
        selection.move_down(3);

        assert_eq!(selection.index(), Some(2));
    }

    #[test]
    fn navigation_keys_update_selection() {
        let mut app = app_with_entries(3);

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        )));
        app.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        app.handle_event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('k'),
            KeyModifiers::NONE,
        )));

        assert_eq!(app.selected(), Some(0));
    }

    #[test]
    fn non_key_events_leave_state_unchanged() {
        let mut app = App::new(Vec::new());

        app.handle_event(Event::Resize(120, 40));

        assert_eq!(app.selected(), None);
        assert!(!app.should_quit);
    }
}
