use std::{
    io,
    path::{Path, PathBuf},
};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::{
    Result,
    destination::{DestinationBrowser, DestinationEntry},
    inbox::{self, InboxEntry},
    move_execution::{self, SourceIdentity},
    proposed_move::ProposedMove,
    terminal::TerminalSession,
    ui,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Inbox,
    DestinationBrowser,
    MovePreview,
    Confirmation,
}

pub struct App {
    entries: Vec<InboxEntry>,
    selection: Selection,
    screen: Screen,
    destination_browser: DestinationBrowser,
    proposed_move: Option<ProposedMove>,
    source_identity: Option<SourceIdentity>,
    notice: Option<String>,
    move_error: Option<String>,
    inbox_path: PathBuf,
    should_quit: bool,
}

impl App {
    pub fn new(entries: Vec<InboxEntry>, home: PathBuf) -> Self {
        let selection = Selection::new(entries.len());
        let inbox_path = home.join("Downloads");
        Self {
            entries,
            selection,
            screen: Screen::Inbox,
            destination_browser: DestinationBrowser::new(home),
            proposed_move: None,
            source_identity: None,
            notice: None,
            move_error: None,
            inbox_path,
            should_quit: false,
        }
    }

    pub fn entries(&self) -> &[InboxEntry] {
        &self.entries
    }

    pub fn selected(&self) -> Option<usize> {
        self.selection.index()
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn destination(&self) -> Option<&Path> {
        (self.screen != Screen::Inbox).then_some(self.destination_browser.current())
    }

    pub fn destination_entries(&self) -> &[DestinationEntry] {
        self.destination_browser.entries()
    }

    pub fn destination_selected(&self) -> Option<usize> {
        self.destination_browser.selected()
    }

    pub fn destination_error(&self) -> Option<&str> {
        self.destination_browser.error()
    }

    pub fn proposed_move(&self) -> Option<&ProposedMove> {
        self.proposed_move.as_ref()
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn move_error(&self) -> Option<&str> {
        self.move_error.as_deref()
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

    pub(crate) fn handle_event(&mut self, event: Event) {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            self.notice = None;
            match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Enter if self.screen == Screen::Inbox && self.selected().is_some() => {
                    self.destination_browser.refresh();
                    self.screen = Screen::DestinationBrowser;
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right
                    if self.screen == Screen::DestinationBrowser =>
                {
                    self.destination_browser.enter_selected();
                }
                KeyCode::Char('d') if self.screen == Screen::DestinationBrowser => {
                    self.proposed_move = self.selected().and_then(|index| {
                        ProposedMove::new(&self.entries[index], self.destination_browser.current())
                    });
                    if self.proposed_move.is_some() {
                        self.screen = Screen::MovePreview;
                    }
                }
                KeyCode::Esc if self.screen == Screen::DestinationBrowser => {
                    self.screen = Screen::Inbox;
                }
                KeyCode::Esc if self.screen == Screen::MovePreview => {
                    self.screen = Screen::DestinationBrowser;
                }
                KeyCode::Char('m')
                    if self.screen == Screen::MovePreview
                        && self
                            .proposed_move
                            .as_ref()
                            .is_some_and(ProposedMove::is_valid) =>
                {
                    let identity = self
                        .proposed_move
                        .as_ref()
                        .and_then(|proposal| SourceIdentity::capture(proposal).ok());
                    if let Some(identity) = identity {
                        self.source_identity = Some(identity);
                        self.move_error = None;
                        self.screen = Screen::Confirmation;
                    }
                }
                KeyCode::Enter if self.screen == Screen::Confirmation => self.attempt_move(),
                KeyCode::Esc if self.screen == Screen::Confirmation => {
                    self.return_to_refreshed_preview();
                }
                KeyCode::Char('j') | KeyCode::Down if self.screen == Screen::DestinationBrowser => {
                    self.destination_browser.move_down();
                }
                KeyCode::Char('k') | KeyCode::Up if self.screen == Screen::DestinationBrowser => {
                    self.destination_browser.move_up();
                }
                KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace
                    if self.screen == Screen::DestinationBrowser =>
                {
                    self.destination_browser.enter_parent();
                }
                KeyCode::Char('j') | KeyCode::Down if self.screen == Screen::Inbox => {
                    self.selection.move_down(self.entries.len());
                }
                KeyCode::Char('k') | KeyCode::Up if self.screen == Screen::Inbox => {
                    self.selection.move_up();
                }
                _ => {}
            }
        }
    }

    fn attempt_move(&mut self) {
        let Some(index) = self.selection.index() else {
            return;
        };
        let Some(identity) = self.source_identity else {
            self.move_error = Some("the reviewed source identity is unavailable".to_owned());
            return;
        };
        let result = self.proposed_move.as_ref().map_or_else(
            || Err("the reviewed move is unavailable".to_owned()),
            |proposal| {
                move_execution::execute_move(proposal, &self.entries[index], identity)
                    .map_err(|error| error.to_string())
            },
        );
        if let Err(error) = result {
            self.move_error = Some(error);
            return;
        }

        match inbox::scan_inbox(&self.inbox_path) {
            Ok(entries) => {
                self.entries = entries;
                self.selection
                    .repair_after_removal(index, self.entries.len());
                self.screen = Screen::Inbox;
                self.proposed_move = None;
                self.source_identity = None;
                self.move_error = None;
                self.notice = Some("Move completed successfully".to_owned());
            }
            Err(error) => {
                self.move_error = Some(format!(
                    "move completed, but the Downloads inbox could not be refreshed: {error}"
                ));
            }
        }
    }

    fn return_to_refreshed_preview(&mut self) {
        self.proposed_move = self.selection.index().and_then(|index| {
            ProposedMove::new(&self.entries[index], self.destination_browser.current())
        });
        self.source_identity = None;
        self.move_error = None;
        self.screen = Screen::MovePreview;
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

    fn repair_after_removal(&mut self, former_index: usize, entry_count: usize) {
        self.index = if entry_count == 0 {
            None
        } else {
            Some(former_index.min(entry_count - 1))
        };
    }
}

fn contextual_io_error(context: &'static str, source: io::Error) -> io::Error {
    io::Error::new(source.kind(), format!("{context}: {source}"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use crate::inbox::InboxEntry;

    use crate::proposed_move::{ProposedEntryType, ProposedMove};

    use super::{App, Screen, Selection};

    fn app_with_entries(entry_count: usize) -> App {
        App::new(
            (0..entry_count)
                .map(|index| InboxEntry::test_file(&format!("entry-{index}")))
                .collect(),
            "/home/tester".into(),
        )
    }

    #[test]
    fn q_requests_quit() {
        let mut app = App::new(Vec::new(), "/home/tester".into());

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
        )));

        assert!(app.should_quit);
    }

    #[test]
    fn other_events_do_not_request_quit() {
        let mut app = App::new(Vec::new(), "/home/tester".into());

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
        let mut app = App::new(Vec::new(), "/home/tester".into());

        app.handle_event(Event::Resize(120, 40));

        assert_eq!(app.selected(), None);
        assert!(!app.should_quit);
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    #[test]
    fn enter_and_escape_traverse_screens_without_losing_inbox_selection() {
        let mut app = app_with_entries(2);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::DestinationBrowser);
        assert_eq!(
            app.destination(),
            Some(std::path::Path::new("/home/tester"))
        );

        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.screen(), Screen::MovePreview);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::DestinationBrowser);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(app.selected(), Some(1));
    }

    #[test]
    fn empty_inbox_cannot_advance() {
        let mut app = App::new(Vec::new(), "/home/tester".into());
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
    }

    #[test]
    fn q_requests_quit_from_every_screen() {
        for initial_screen in [
            Screen::Inbox,
            Screen::DestinationBrowser,
            Screen::MovePreview,
            Screen::Confirmation,
        ] {
            let mut app = app_with_entries(1);
            app.screen = initial_screen;
            press(&mut app, KeyCode::Char('q'));
            assert!(app.should_quit, "quit from {initial_screen:?}");
        }
    }

    #[test]
    fn proposed_moves_preserve_basename_and_symlink_identity() {
        let entries = [
            InboxEntry::test_file("file.txt"),
            InboxEntry::test_directory("folder"),
            InboxEntry::test_symlink("link", crate::inbox::EntryKind::Directory),
        ];

        for (entry, expected_type) in entries.iter().zip([
            ProposedEntryType::File,
            ProposedEntryType::Directory,
            ProposedEntryType::Symlink,
        ]) {
            let proposal = ProposedMove::new(entry, std::path::Path::new("/destination"))
                .expect("test entry has a basename");
            assert_eq!(proposal.entry_type(), expected_type);
            assert_eq!(proposal.source(), entry.path());
            assert_eq!(proposal.destination(), std::path::Path::new("/destination"));
            assert_eq!(
                proposal.resulting_path(),
                std::path::Path::new("/destination").join(entry.path().file_name().unwrap())
            );
        }
    }

    #[test]
    fn reopening_preview_revalidates_the_source() {
        let root = std::env::temp_dir().join(format!(
            "downloads-janitor-app-validation-{}",
            std::process::id()
        ));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::File::create(&source).unwrap();
        let entry = InboxEntry::test_entry(source.clone(), crate::inbox::EntryKind::File, false);
        let mut app = App::new(vec![entry], root.clone());

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert!(app.proposed_move().unwrap().is_valid());

        press(&mut app, KeyCode::Esc);
        fs::remove_file(&source).unwrap();
        press(&mut app, KeyCode::Char('d'));

        assert!(
            app.proposed_move()
                .unwrap()
                .failures()
                .contains(&crate::proposed_move::ValidationFailure::SourceMissing)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn valid_preview_opens_confirmation_and_back_preserves_proposal() {
        let root = std::env::temp_dir().join(format!(
            "downloads-janitor-app-confirmation-{}",
            std::process::id()
        ));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::File::create(&source).unwrap();
        let entry = InboxEntry::test_entry(source, crate::inbox::EntryKind::File, false);
        let mut app = App::new(vec![entry], root.clone());

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        let source = app.proposed_move().unwrap().source().to_path_buf();
        let resulting_path = app.proposed_move().unwrap().resulting_path().to_path_buf();
        let entry_type = app.proposed_move().unwrap().entry_type();
        press(&mut app, KeyCode::Char('m'));
        assert_eq!(app.screen(), Screen::Confirmation);

        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::MovePreview);
        assert_eq!(app.proposed_move().unwrap().source(), source);
        assert_eq!(
            app.proposed_move().unwrap().resulting_path(),
            resulting_path
        );
        assert_eq!(app.proposed_move().unwrap().entry_type(), entry_type);
        assert!(fs::symlink_metadata(source).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_preview_refuses_confirmation() {
        let mut app = app_with_entries(1);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));

        assert!(!app.proposed_move().unwrap().is_valid());
        press(&mut app, KeyCode::Char('m'));

        assert_eq!(app.screen(), Screen::MovePreview);
    }

    #[test]
    fn completed_move_refreshes_inbox_repairs_selection_and_shows_notice() {
        let home =
            std::env::temp_dir().join(format!("downloads-janitor-app-move-{}", std::process::id()));
        let downloads = home.join("Downloads");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&downloads).unwrap();
        fs::write(downloads.join("alpha.txt"), b"alpha").unwrap();
        fs::write(downloads.join("beta.txt"), b"beta").unwrap();
        let entries = crate::inbox::scan_inbox(&downloads).unwrap();
        let mut app = App::new(entries, home.clone());

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('m'));
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(app.entries().len(), 1);
        assert_eq!(app.selected(), Some(0));
        assert_eq!(app.notice(), Some("Move completed successfully"));
        assert_eq!(fs::read(home.join("beta.txt")).unwrap(), b"beta");

        press(&mut app, KeyCode::Down);
        assert_eq!(app.notice(), None);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn moving_the_only_entry_leaves_an_empty_valid_selection() {
        let home = std::env::temp_dir().join(format!(
            "downloads-janitor-app-last-move-{}",
            std::process::id()
        ));
        let downloads = home.join("Downloads");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&downloads).unwrap();
        fs::write(downloads.join("only.txt"), b"only").unwrap();
        let entries = crate::inbox::scan_inbox(&downloads).unwrap();
        let mut app = App::new(entries, home.clone());

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('m'));
        press(&mut app, KeyCode::Enter);

        assert!(app.entries().is_empty());
        assert_eq!(app.selected(), None);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn failed_collision_preserves_context_and_retry_can_succeed() {
        let home = std::env::temp_dir().join(format!(
            "downloads-janitor-app-retry-{}",
            std::process::id()
        ));
        let downloads = home.join("Downloads");
        let source = downloads.join("source.txt");
        let result = home.join("source.txt");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&downloads).unwrap();
        fs::write(&source, b"source").unwrap();
        let entries = crate::inbox::scan_inbox(&downloads).unwrap();
        let mut app = App::new(entries, home.clone());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('m'));
        fs::write(&result, b"collision").unwrap();

        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::Confirmation);
        assert!(app.move_error().unwrap().contains("resulting path"));
        assert_eq!(app.proposed_move().unwrap().source(), source);
        assert_eq!(app.proposed_move().unwrap().resulting_path(), result);
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert_eq!(fs::read(&result).unwrap(), b"collision");

        fs::remove_file(&result).unwrap();
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(fs::read(&result).unwrap(), b"source");
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn back_after_failure_rebuilds_and_revalidates_preview() {
        let root = std::env::temp_dir().join(format!(
            "downloads-janitor-app-failed-back-{}",
            std::process::id()
        ));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(&source, b"source").unwrap();
        let entry = InboxEntry::test_entry(source.clone(), crate::inbox::EntryKind::File, false);
        let mut app = App::new(vec![entry], root.clone());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('m'));
        fs::remove_file(&source).unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Confirmation);

        press(&mut app, KeyCode::Esc);

        assert_eq!(app.screen(), Screen::MovePreview);
        assert!(
            app.proposed_move()
                .unwrap()
                .failures()
                .contains(&crate::proposed_move::ValidationFailure::SourceMissing)
        );
        assert_eq!(app.move_error(), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_source_is_refused_and_quit_still_works_after_failure() {
        let root = std::env::temp_dir().join(format!(
            "downloads-janitor-app-changed-source-{}",
            std::process::id()
        ));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(&source, b"original").unwrap();
        let entry = InboxEntry::test_entry(source.clone(), crate::inbox::EntryKind::File, false);
        let mut app = App::new(vec![entry], root.clone());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('m'));
        fs::rename(&source, root.join("original.txt")).unwrap();
        fs::write(&source, b"replacement").unwrap();

        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::Confirmation);
        assert!(app.move_error().unwrap().contains("source identity"));
        assert_eq!(fs::read(&source).unwrap(), b"replacement");
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit);
        fs::remove_dir_all(root).unwrap();
    }
}
