use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::{Result, inbox::InboxEntry, terminal::TerminalSession, ui};

pub struct App {
    entries: Vec<InboxEntry>,
    should_quit: bool,
}

impl App {
    pub fn new(entries: Vec<InboxEntry>) -> Self {
        Self {
            entries,
            should_quit: false,
        }
    }

    pub fn entries(&self) -> &[InboxEntry] {
        &self.entries
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
            && key.code == KeyCode::Char('q')
        {
            self.should_quit = true;
        }
    }
}

fn contextual_io_error(context: &'static str, source: io::Error) -> io::Error {
    io::Error::new(source.kind(), format!("{context}: {source}"))
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use super::App;

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
}
