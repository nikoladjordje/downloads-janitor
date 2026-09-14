use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    Result,
    batch::{Batch, BatchAction, EntryOutcome},
    destination::{DestinationBrowser, DestinationEntry},
    favorites::{FavoriteDestination, Favorites, Rule, RuleKind},
    filename_editor::FilenameEditor,
    ignored_entries::IgnoredEntries,
    inbox::{self, InboxEntry},
    inbox_marks::InboxMarks,
    move_execution::{self, SourceIdentity},
    proposed_move::ProposedMove,
    terminal::TerminalSession,
    ui,
};

type InboxScanner = fn(&Path) -> Result<Vec<InboxEntry>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Inbox,
    TrashPreview,
    DeleteConfirmation,
    DestinationBrowser,
    MovePreview,
    RenameEditor,
    MoveNameEditor,
    RenamePreview,
    BulkPreview,
    BulkProgress,
    BulkResult,
    Configuration,
    FavoriteNameEditor,
    FavoriteDestinationBrowser,
    RulePatternEditor,
    RuleKindPicker,
    RuleFavoritePicker,
}

pub struct App {
    pub(crate) delete_review: Option<crate::permanent_delete::DeleteReview>,
    pub(crate) delete_confirmation: String,
    pub(crate) trash_review: Option<crate::trash::TrashReview>,
    pub(crate) trash_root: PathBuf,
    batch: Option<Batch>,
    bulk_targets: Vec<InboxEntry>,
    pub(crate) batch_scroll: (usize, u16),
    entries: Vec<InboxEntry>,
    other_entries: Vec<InboxEntry>,
    ignored: IgnoredEntries,
    favorites: Favorites,
    viewing_ignored: bool,
    selection: Selection,
    configuration_selection: Selection,
    rule_selection: Selection,
    rule_kind_selection: Selection,
    rule_favorite_selection: Selection,
    rule_matches: Vec<crate::rule_match::RuleMatch>,
    marks: InboxMarks,
    screen: Screen,
    destination_browser: DestinationBrowser,
    proposed_move: Option<ProposedMove>,
    source_identity: Option<SourceIdentity>,
    rename_editor: Option<FilenameEditor>,
    favorite_name_editor: Option<FilenameEditor>,
    favorite_edit_index: Option<usize>,
    rule_pattern_editor: Option<FilenameEditor>,
    rule_edit_index: Option<usize>,
    rule_kind: Option<RuleKind>,
    move_basename: Option<OsString>,
    notice: Option<String>,
    move_error: Option<String>,
    inbox_path: PathBuf,
    inbox_scanner: InboxScanner,
    pending_g: bool,
    should_quit: bool,
}

impl App {
    pub fn new(entries: Vec<InboxEntry>, home: PathBuf) -> Self {
        Self::with_inbox_scanner(entries, home, inbox::scan_inbox)
    }

    pub(crate) fn with_inbox_scanner(
        entries: Vec<InboxEntry>,
        home: PathBuf,
        inbox_scanner: InboxScanner,
    ) -> Self {
        let ignored = IgnoredEntries::load(&home);
        let (other_entries, entries): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|entry| ignored.contains(entry));
        let selection = Selection::new(entries.len());
        let favorites = Favorites::load(home.clone());
        let inbox_path = home.join("Downloads");
        let mut app = Self {
            delete_review: None,
            delete_confirmation: String::new(),
            trash_review: None,
            trash_root: crate::trash::home_trash(
                &home,
                std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            ),
            batch: None,
            bulk_targets: Vec::new(),
            batch_scroll: (0, 0),
            entries,
            other_entries,
            ignored,
            configuration_selection: Selection::new(favorites.entries().len()),
            rule_selection: Selection::new(favorites.rules().len()),
            rule_kind_selection: Selection::new(RuleKind::ALL.len()),
            rule_favorite_selection: Selection::new(favorites.entries().len()),
            rule_matches: Vec::new(),
            favorites,
            viewing_ignored: false,
            selection,
            marks: InboxMarks::default(),
            screen: Screen::Inbox,
            destination_browser: DestinationBrowser::new(home),
            proposed_move: None,
            source_identity: None,
            rename_editor: None,
            favorite_name_editor: None,
            favorite_edit_index: None,
            rule_pattern_editor: None,
            rule_edit_index: None,
            rule_kind: None,
            move_basename: None,
            notice: None,
            move_error: None,
            inbox_path,
            inbox_scanner,
            pending_g: false,
            should_quit: false,
        };
        app.evaluate_rule_matches();
        app
    }

    pub fn viewing_ignored(&self) -> bool {
        self.viewing_ignored
    }

    pub fn ignored_warning(&self) -> Option<&str> {
        self.ignored.warning()
    }

    pub fn favorites(&self) -> &[FavoriteDestination] {
        self.favorites.entries()
    }

    pub fn favorite_selected(&self) -> Option<usize> {
        self.configuration_selection.index()
    }

    pub fn favorite_name(&self) -> Option<OsString> {
        self.favorite_name_editor.as_ref().map(FilenameEditor::name)
    }

    pub fn favorites_warning(&self) -> Option<&str> {
        self.favorites.warning()
    }

    pub fn favorite_available(&self, favorite: &FavoriteDestination) -> bool {
        self.favorites.available(favorite)
    }

    pub fn rules(&self) -> &[Rule] {
        self.favorites.rules()
    }
    pub fn rule_selected(&self) -> Option<usize> {
        self.rule_selection.index()
    }
    pub fn rule_pattern(&self) -> Option<OsString> {
        self.rule_pattern_editor.as_ref().map(FilenameEditor::name)
    }
    pub fn rule_kind_selected(&self) -> Option<RuleKind> {
        self.rule_kind_selection
            .index()
            .map(|index| RuleKind::ALL[index])
    }
    pub fn rule_favorite_selected(&self) -> Option<usize> {
        self.rule_favorite_selection.index()
    }
    pub fn rule_has_available_favorite(&self, rule: &Rule) -> bool {
        self.favorites.rule_has_available_favorite(rule)
    }

    #[allow(dead_code)] // Read by the next presentation slice; retained now as explicit state.
    pub fn rule_match(&self, index: usize) -> Option<&crate::rule_match::RuleMatch> {
        self.rule_matches.get(index)
    }

    pub fn entries(&self) -> &[InboxEntry] {
        &self.entries
    }

    pub fn selected(&self) -> Option<usize> {
        self.selection.index()
    }

    pub fn marked(&self, entry: &InboxEntry) -> bool {
        self.marks.contains(entry)
    }

    pub fn marked_count(&self) -> usize {
        self.marks.count()
    }

    pub fn visual_selection(&self) -> bool {
        self.marks.visual()
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

    pub fn rename_name(&self) -> Option<std::ffi::OsString> {
        self.rename_editor.as_ref().map(FilenameEditor::name)
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

            if self.screen == Screen::BulkProgress {
                // Render progress before polling input and processing the next entry.
                // Drain queued navigation/Enter events so they cannot mask a queued Esc.
                while event::poll(std::time::Duration::ZERO)? {
                    self.handle_event(event::read()?);
                    if self.screen != Screen::BulkProgress {
                        break;
                    }
                }
                if self.screen == Screen::BulkProgress {
                    self.advance_batch();
                }
                continue;
            }
            let event = event::read()
                .map_err(|error| contextual_io_error("failed to read terminal event", error))?;
            self.handle_event(event);
        }

        Ok(())
    }

    pub(crate) fn handle_event(&mut self, event: Event) {
        // Ignore reported repeat/release events, including Enter in either Preview.
        // Move Preview requires d, so repeated navigation presses cannot execute.
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            if matches!(
                self.screen,
                Screen::BulkPreview | Screen::BulkProgress | Screen::BulkResult
            ) {
                self.handle_batch_key(key);
                return;
            }
            if self.screen == Screen::DeleteConfirmation {
                self.handle_delete_key(key);
                return;
            }
            self.notice = None;
            if matches!(self.screen, Screen::RenameEditor | Screen::MoveNameEditor) {
                self.handle_rename_editor(key);
                return;
            }
            if self.screen == Screen::FavoriteNameEditor {
                self.handle_favorite_name_editor(key);
                return;
            }
            if self.screen == Screen::RulePatternEditor {
                self.handle_rule_pattern_editor(key);
                return;
            }
            if key.code != KeyCode::Char('g') {
                self.pending_g = false;
            }
            match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('C') if self.screen == Screen::Inbox => {
                    self.marks.exit_visual();
                    self.configuration_selection = Selection::new(self.favorites.entries().len());
                    self.screen = Screen::Configuration;
                }
                KeyCode::Char('a') if self.screen == Screen::Configuration => {
                    self.start_favorite_name_edit(None);
                }
                KeyCode::Char('e') | KeyCode::Enter if self.screen == Screen::Configuration => {
                    self.start_favorite_name_edit(self.favorite_selected());
                }
                KeyCode::Char('x') if self.screen == Screen::Configuration => {
                    self.remove_favorite();
                }
                KeyCode::Char('A') if self.screen == Screen::Configuration => {
                    self.start_rule_edit(None)
                }
                KeyCode::Char('E') if self.screen == Screen::Configuration => {
                    self.start_rule_edit(self.rule_selected())
                }
                KeyCode::Char('X') if self.screen == Screen::Configuration => self.remove_rule(),
                KeyCode::Char(']') if self.screen == Screen::Configuration => self.move_rule(1),
                KeyCode::Char('[') if self.screen == Screen::Configuration => self.move_rule(-1),
                KeyCode::Char('j') | KeyCode::Down if self.screen == Screen::Configuration => {
                    self.configuration_selection
                        .move_down(self.favorites.entries().len());
                }
                KeyCode::Char('k') | KeyCode::Up if self.screen == Screen::Configuration => {
                    self.configuration_selection.move_up();
                }
                KeyCode::Char('G') if self.screen == Screen::Configuration => {
                    self.configuration_selection
                        .move_to_last(self.favorites.entries().len());
                }
                KeyCode::Char('J') if self.screen == Screen::Configuration => {
                    self.rule_selection.move_down(self.rules().len())
                }
                KeyCode::Char('K') if self.screen == Screen::Configuration => {
                    self.rule_selection.move_up()
                }
                KeyCode::Enter if self.screen == Screen::RuleKindPicker => {
                    self.rule_kind = self.rule_kind_selected();
                    self.screen = Screen::RuleFavoritePicker;
                }
                KeyCode::Esc if self.screen == Screen::RuleKindPicker => {
                    self.screen = Screen::RulePatternEditor
                }
                KeyCode::Char('j') | KeyCode::Down if self.screen == Screen::RuleKindPicker => {
                    self.rule_kind_selection.move_down(RuleKind::ALL.len())
                }
                KeyCode::Char('k') | KeyCode::Up if self.screen == Screen::RuleKindPicker => {
                    self.rule_kind_selection.move_up()
                }
                KeyCode::Enter if self.screen == Screen::RuleFavoritePicker => self.save_rule(),
                KeyCode::Esc if self.screen == Screen::RuleFavoritePicker => {
                    self.screen = Screen::RuleKindPicker
                }
                KeyCode::Char('j') | KeyCode::Down if self.screen == Screen::RuleFavoritePicker => {
                    self.rule_favorite_selection
                        .move_down(self.favorites.entries().len())
                }
                KeyCode::Char('k') | KeyCode::Up if self.screen == Screen::RuleFavoritePicker => {
                    self.rule_favorite_selection.move_up()
                }
                KeyCode::Esc if self.screen == Screen::Configuration => self.screen = Screen::Inbox,
                KeyCode::Char('D') if self.screen == Screen::Inbox && !self.viewing_ignored => {
                    if self.marks.count() > 1 {
                        self.start_removal_batch(BatchAction::Delete);
                        return;
                    }
                    if self.prepare_individual_action() {
                        match crate::permanent_delete::DeleteReview::new(
                            &self.entries[self.selected().unwrap()],
                        ) {
                            Ok(review) => {
                                self.delete_review = Some(review);
                                self.delete_confirmation.clear();
                                self.move_error = None;
                                self.batch_scroll = (0, 0);
                                self.screen = Screen::DeleteConfirmation;
                            }
                            Err(error) => self.notice = Some(format!("Cannot delete: {error}")),
                        }
                    }
                }
                KeyCode::Char('t') if self.screen == Screen::Inbox && !self.viewing_ignored => {
                    if self.marks.count() > 1 {
                        self.start_removal_batch(BatchAction::Trash(self.trash_root.clone()));
                        return;
                    }
                    if self.prepare_individual_action() {
                        match crate::trash::TrashReview::new(
                            &self.entries[self.selected().unwrap()],
                        ) {
                            Ok(review) => {
                                self.trash_review = Some(review);
                                self.move_error = None;
                                self.batch_scroll = (0, 0);
                                self.screen = Screen::TrashPreview;
                            }
                            Err(error) => self.notice = Some(format!("Cannot trash: {error}")),
                        }
                    }
                }
                KeyCode::Esc if self.screen == Screen::TrashPreview => {
                    self.trash_review = None;
                    self.move_error = None;
                    self.screen = Screen::Inbox;
                }
                KeyCode::Enter if self.screen == Screen::TrashPreview => self.attempt_trash(),
                KeyCode::Char('j') | KeyCode::Down if self.screen == Screen::TrashPreview => {
                    self.batch_scroll.0 = self.batch_scroll.0.saturating_add(1);
                }
                KeyCode::Char('k') | KeyCode::Up if self.screen == Screen::TrashPreview => {
                    self.batch_scroll.0 = self.batch_scroll.0.saturating_sub(1);
                }
                KeyCode::Char('l') | KeyCode::Right if self.screen == Screen::TrashPreview => {
                    self.batch_scroll.1 = self.batch_scroll.1.saturating_add(8);
                }
                KeyCode::Char('h') | KeyCode::Left if self.screen == Screen::TrashPreview => {
                    self.batch_scroll.1 = self.batch_scroll.1.saturating_sub(8);
                }
                KeyCode::Home if self.screen == Screen::TrashPreview => self.batch_scroll = (0, 0),
                KeyCode::Char('I') if self.screen == Screen::Inbox => self.toggle_ignored_view(),
                KeyCode::Char('i') if self.screen == Screen::Inbox && !self.viewing_ignored => {
                    self.change_ignored(true)
                }
                KeyCode::Char('u') if self.screen == Screen::Inbox && self.viewing_ignored => {
                    self.change_ignored(false)
                }
                KeyCode::Char(' ') if self.screen == Screen::Inbox => {
                    if let Some(index) = self.selected() {
                        self.marks.toggle(&self.entries[index]);
                    }
                }
                KeyCode::Char('a') if self.screen == Screen::Inbox => {
                    self.marks.toggle_all(&self.entries)
                }
                KeyCode::Char('c') | KeyCode::Esc if self.screen == Screen::Inbox => {
                    self.marks.clear()
                }
                KeyCode::Char('V') if self.screen == Screen::Inbox => {
                    self.marks.toggle_visual(self.selected(), &self.entries)
                }
                KeyCode::Char('R') if self.screen == Screen::Inbox => self.refresh_inbox(),
                KeyCode::Char('r') if self.screen == Screen::Inbox && !self.viewing_ignored => {
                    if self.prepare_individual_action() {
                        self.start_rename();
                    }
                }
                KeyCode::Char('r') if self.screen == Screen::MovePreview => {
                    self.start_move_name_edit()
                }
                KeyCode::Esc if self.screen == Screen::RenamePreview => self.cancel_rename(),
                KeyCode::Enter if self.screen == Screen::RenamePreview => self.attempt_rename(),
                KeyCode::Enter
                    if self.screen == Screen::Inbox
                        && !self.viewing_ignored
                        && self.selected().is_some() =>
                {
                    if self.marks.count() > 1 {
                        self.marks.exit_visual();
                        self.bulk_targets = self
                            .entries
                            .iter()
                            .filter(|entry| self.marked(entry))
                            .cloned()
                            .collect();
                        self.destination_browser.refresh();
                        self.screen = Screen::DestinationBrowser;
                        return;
                    }
                    self.bulk_targets.clear();
                    self.batch = None;
                    if !self.prepare_individual_action() {
                        return;
                    }
                    self.move_basename = None;
                    self.source_identity = if self.marks.count() == 1 {
                        self.entries[self.selected().expect("action has an entry")].identity()
                    } else {
                        None
                    };
                    self.move_error = None;
                    self.destination_browser.refresh();
                    self.screen = Screen::DestinationBrowser;
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right
                    if matches!(
                        self.screen,
                        Screen::DestinationBrowser | Screen::FavoriteDestinationBrowser
                    ) =>
                {
                    self.destination_browser.enter_selected();
                }
                KeyCode::Char('d') if self.screen == Screen::FavoriteDestinationBrowser => {
                    self.save_favorite();
                }
                KeyCode::Char('d') if self.screen == Screen::DestinationBrowser => {
                    if !self.bulk_targets.is_empty() {
                        self.batch = Some(Batch::new(
                            self.bulk_targets.clone(),
                            self.destination_browser.current(),
                            self.inbox_path.parent().expect("Inbox has HOME parent"),
                        ));
                        self.batch_scroll = (0, 0);
                        self.screen = Screen::BulkPreview;
                        return;
                    }
                    self.proposed_move = self.build_move_proposal();
                    if self.proposed_move.is_some() {
                        self.screen = Screen::MovePreview;
                    }
                }
                KeyCode::Esc if self.screen == Screen::DestinationBrowser => {
                    self.screen = Screen::Inbox;
                }
                KeyCode::Esc if self.screen == Screen::FavoriteDestinationBrowser => {
                    self.screen = Screen::FavoriteNameEditor;
                }
                KeyCode::Esc if self.screen == Screen::MovePreview => {
                    if self.move_error.is_some() {
                        self.return_to_refreshed_preview();
                    }
                    self.screen = Screen::DestinationBrowser;
                }
                KeyCode::Enter
                    if self.screen == Screen::MovePreview
                        && self
                            .proposed_move
                            .as_ref()
                            .is_some_and(ProposedMove::is_valid) =>
                {
                    self.move_error = None;
                    if self.source_identity.is_none() {
                        match self.proposed_move.as_ref().map(SourceIdentity::capture) {
                            Some(Ok(identity)) => self.source_identity = Some(identity),
                            Some(Err(error)) => self.move_error = Some(error.to_string()),
                            None => {
                                self.move_error =
                                    Some("the reviewed move is unavailable".to_owned());
                            }
                        }
                    }
                    if self.source_identity.is_some() {
                        self.attempt_move();
                    }
                }
                KeyCode::Char('j') | KeyCode::Down
                    if matches!(
                        self.screen,
                        Screen::DestinationBrowser | Screen::FavoriteDestinationBrowser
                    ) =>
                {
                    self.destination_browser.move_down();
                }
                KeyCode::Char('k') | KeyCode::Up
                    if matches!(
                        self.screen,
                        Screen::DestinationBrowser | Screen::FavoriteDestinationBrowser
                    ) =>
                {
                    self.destination_browser.move_up();
                }
                KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace
                    if matches!(
                        self.screen,
                        Screen::DestinationBrowser | Screen::FavoriteDestinationBrowser
                    ) =>
                {
                    self.destination_browser.enter_parent();
                }
                KeyCode::Char('j') | KeyCode::Down if self.screen == Screen::Inbox => {
                    self.selection.move_down(self.entries.len());
                }
                KeyCode::Char('k') | KeyCode::Up if self.screen == Screen::Inbox => {
                    self.selection.move_up();
                }
                KeyCode::Char('g')
                    if matches!(
                        self.screen,
                        Screen::Inbox
                            | Screen::DestinationBrowser
                            | Screen::FavoriteDestinationBrowser
                            | Screen::Configuration
                    ) =>
                {
                    if self.pending_g {
                        match self.screen {
                            Screen::Inbox => self.selection.move_to_first(self.entries.len()),
                            Screen::DestinationBrowser => {
                                self.destination_browser.move_to_first();
                            }
                            Screen::FavoriteDestinationBrowser => {
                                self.destination_browser.move_to_first()
                            }
                            Screen::Configuration => self
                                .configuration_selection
                                .move_to_first(self.favorites.entries().len()),
                            Screen::DeleteConfirmation
                            | Screen::TrashPreview
                            | Screen::MovePreview
                            | Screen::RenameEditor
                            | Screen::MoveNameEditor
                            | Screen::RenamePreview
                            | Screen::BulkPreview
                            | Screen::BulkProgress
                            | Screen::BulkResult
                            | Screen::FavoriteNameEditor
                            | Screen::RulePatternEditor
                            | Screen::RuleKindPicker
                            | Screen::RuleFavoritePicker => {}
                        }
                        self.pending_g = false;
                    } else {
                        self.pending_g = true;
                    }
                }
                KeyCode::Char('G') if self.screen == Screen::Inbox => {
                    self.selection.move_to_last(self.entries.len());
                }
                KeyCode::Char('G') if self.screen == Screen::DestinationBrowser => {
                    self.destination_browser.move_to_last();
                }
                KeyCode::Char('G') if self.screen == Screen::FavoriteDestinationBrowser => {
                    self.destination_browser.move_to_last();
                }
                _ => {}
            }
            if self.screen == Screen::Inbox
                && matches!(
                    key.code,
                    KeyCode::Char('j' | 'k' | 'g' | 'G') | KeyCode::Down | KeyCode::Up
                )
            {
                self.marks.extend(self.selected(), &self.entries);
            }
        }
    }

    pub(crate) fn batch(&self) -> Option<&Batch> {
        self.batch.as_ref()
    }

    fn start_favorite_name_edit(&mut self, index: Option<usize>) {
        let name = index
            .and_then(|selected| self.favorites.entries().get(selected))
            .map(|favorite| OsString::from(favorite.name()))
            .unwrap_or_default();
        self.favorite_name_editor = Some(FilenameEditor::new(&name));
        self.favorite_edit_index = index;
        self.move_error = None;
        self.screen = Screen::FavoriteNameEditor;
    }

    fn handle_favorite_name_editor(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.favorite_name_editor = None;
                self.favorite_edit_index = None;
                self.move_error = None;
                self.screen = Screen::Configuration;
            }
            KeyCode::Enter => {
                let Some(name) = self
                    .favorite_name()
                    .and_then(|name| name.into_string().ok())
                else {
                    self.move_error = Some("Favorite names must be valid Unicode text".into());
                    return;
                };
                if name.is_empty() {
                    self.move_error = Some("Favorite name cannot be empty".into());
                    return;
                }
                self.move_error = None;
                self.destination_browser.refresh();
                self.screen = Screen::FavoriteDestinationBrowser;
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                self.favorite_name_editor
                    .as_mut()
                    .expect("favorite editor exists")
                    .clear();
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.favorite_name_editor
                    .as_mut()
                    .expect("favorite editor exists")
                    .append(character);
                self.move_error = None;
            }
            KeyCode::Backspace => {
                self.favorite_name_editor
                    .as_mut()
                    .expect("favorite editor exists")
                    .backspace();
                self.move_error = None;
            }
            _ => {}
        }
    }

    fn save_favorite(&mut self) {
        let Some(name) = self
            .favorite_name()
            .and_then(|name| name.into_string().ok())
        else {
            self.move_error = Some("Favorite names must be valid Unicode text".into());
            self.screen = Screen::FavoriteNameEditor;
            return;
        };
        let path = self.destination_browser.current().to_path_buf();
        let edited_index = self.favorite_edit_index;
        let result = match self.favorite_edit_index {
            Some(index) => self.favorites.replace(index, name, path),
            None => self.favorites.add(name, path),
        };
        match result {
            Ok(()) => {
                let count = self.favorites.entries().len();
                self.configuration_selection.index = edited_index.or(count.checked_sub(1));
                self.favorite_name_editor = None;
                self.favorite_edit_index = None;
                self.move_error = None;
                self.notice = Some("Favorite Destination saved".into());
                self.evaluate_rule_matches();
                self.screen = Screen::Configuration;
            }
            Err(error) => {
                self.move_error = Some(error.to_string());
                self.screen = Screen::FavoriteNameEditor;
            }
        }
    }

    fn remove_favorite(&mut self) {
        let Some(index) = self.favorite_selected() else {
            return;
        };
        match self.favorites.remove(index) {
            Ok(favorite) => {
                self.configuration_selection
                    .repair_after_removal(index, self.favorites.entries().len());
                self.notice = Some(format!(
                    "Removed Favorite Destination {:?}",
                    favorite.name()
                ));
                self.evaluate_rule_matches();
            }
            Err(error) => {
                self.notice = Some(format!("Cannot remove Favorite Destination: {error}"))
            }
        }
    }

    fn start_rule_edit(&mut self, index: Option<usize>) {
        let rule = index.and_then(|index| self.rules().get(index));
        let pattern = rule
            .map(|rule| OsString::from(rule.pattern()))
            .unwrap_or_default();
        let kind = rule.map(Rule::kind);
        let favorite_name = rule.map(|rule| rule.favorite_name().to_owned());
        self.rule_pattern_editor = Some(FilenameEditor::new(&pattern));
        self.rule_edit_index = index;
        self.rule_kind = kind;
        self.rule_kind_selection = Selection::new(RuleKind::ALL.len());
        self.rule_favorite_selection = Selection::new(self.favorites.entries().len());
        if let Some(kind) = self.rule_kind {
            self.rule_kind_selection.index = RuleKind::ALL
                .iter()
                .position(|candidate| *candidate == kind);
        }
        if let Some(name) = favorite_name.as_deref() {
            self.rule_favorite_selection.index = self
                .favorites
                .entries()
                .iter()
                .position(|favorite| favorite.name() == name);
        }
        self.move_error = None;
        self.screen = Screen::RulePatternEditor;
    }

    fn handle_rule_pattern_editor(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.rule_pattern_editor = None;
                self.rule_edit_index = None;
                self.move_error = None;
                self.screen = Screen::Configuration;
            }
            KeyCode::Enter => {
                let Some(pattern) = self
                    .rule_pattern()
                    .and_then(|pattern| pattern.into_string().ok())
                else {
                    self.move_error = Some("Rule patterns must be valid Unicode text".into());
                    return;
                };
                if pattern.is_empty() {
                    self.move_error = Some("Rule basename pattern cannot be empty".into());
                    return;
                }
                self.move_error = None;
                self.screen = Screen::RuleKindPicker;
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => self
                .rule_pattern_editor
                .as_mut()
                .expect("rule editor exists")
                .clear(),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.rule_pattern_editor
                    .as_mut()
                    .expect("rule editor exists")
                    .append(character);
                self.move_error = None;
            }
            KeyCode::Backspace => {
                self.rule_pattern_editor
                    .as_mut()
                    .expect("rule editor exists")
                    .backspace();
                self.move_error = None;
            }
            _ => {}
        }
    }

    fn save_rule(&mut self) {
        let Some(pattern) = self
            .rule_pattern()
            .and_then(|pattern| pattern.into_string().ok())
        else {
            return;
        };
        let Some(kind) = self.rule_kind else {
            return;
        };
        let Some(favorite) = self
            .rule_favorite_selection
            .index()
            .and_then(|index| self.favorites.entries().get(index))
        else {
            self.move_error = Some("Choose a Favorite Destination for this Rule".into());
            return;
        };
        let result = match self.rule_edit_index {
            Some(index) => {
                self.favorites
                    .replace_rule(index, pattern, kind, favorite.name().into())
            }
            None => self
                .favorites
                .add_rule(pattern, kind, favorite.name().into()),
        };
        match result {
            Ok(()) => {
                let count = self.rules().len();
                self.rule_selection.index = self.rule_edit_index.or(count.checked_sub(1));
                self.rule_pattern_editor = None;
                self.rule_edit_index = None;
                self.rule_kind = None;
                self.notice = Some("Rule saved".into());
                self.evaluate_rule_matches();
                self.screen = Screen::Configuration;
            }
            Err(error) => {
                self.move_error = Some(error.to_string());
                self.screen = Screen::RulePatternEditor;
            }
        }
    }

    fn remove_rule(&mut self) {
        let Some(index) = self.rule_selected() else {
            return;
        };
        match self.favorites.remove_rule(index) {
            Ok(_) => {
                self.rule_selection
                    .repair_after_removal(index, self.rules().len());
                self.notice = Some("Removed Rule".into());
                self.evaluate_rule_matches();
            }
            Err(error) => self.notice = Some(format!("Cannot remove Rule: {error}")),
        }
    }

    fn move_rule(&mut self, direction: isize) {
        let Some(index) = self.rule_selected() else {
            return;
        };
        match self.favorites.move_rule(index, direction) {
            Ok(destination) => {
                self.rule_selection.index = Some(destination);
                self.evaluate_rule_matches();
            }
            Err(error) => self.notice = Some(format!("Cannot reorder Rule: {error}")),
        }
    }

    fn start_removal_batch(&mut self, action: BatchAction) {
        self.marks.exit_visual();
        let targets = self
            .entries
            .iter()
            .filter(|entry| self.marked(entry))
            .cloned()
            .collect();
        self.batch = Some(Batch::removal(targets, action));
        self.batch_scroll = (0, 0);
        self.delete_confirmation.clear();
        self.screen = Screen::BulkPreview;
    }

    fn handle_batch_key(&mut self, key: KeyEvent) {
        let code = key.code;
        if self.screen == Screen::BulkProgress {
            if code == KeyCode::Esc {
                self.batch.as_mut().expect("batch exists").stop();
                self.finish_batch();
            }
            return;
        }
        let deleting = self
            .batch
            .as_ref()
            .is_some_and(|batch| batch.action == BatchAction::Delete);
        if self.screen == Screen::BulkPreview && deleting {
            match code {
                KeyCode::Enter if self.delete_confirmation != "delete" => {
                    self.notice = Some("Type exactly delete, then Enter; Esc cancels".into());
                    return;
                }
                KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                    self.delete_confirmation.clear();
                    return;
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.delete_confirmation.push(character);
                    return;
                }
                KeyCode::Backspace => {
                    self.delete_confirmation.pop();
                    return;
                }
                _ => {}
            }
        }
        match code {
            KeyCode::Enter if self.screen == Screen::BulkPreview => {
                self.delete_confirmation.clear();
                self.notice = None;
                let batch = self.batch.as_mut().expect("batch exists");
                batch.authorize();
                self.batch_scroll = (0, 0);
                if batch.running {
                    self.screen = Screen::BulkProgress;
                }
            }
            KeyCode::Esc if self.screen == Screen::BulkPreview => {
                self.delete_confirmation.clear();
                if self.batch.as_ref().expect("batch exists").action == BatchAction::Move {
                    self.screen = Screen::DestinationBrowser;
                } else {
                    self.screen = Screen::Inbox;
                    self.batch = None;
                }
            }
            KeyCode::Enter | KeyCode::Esc if self.screen == Screen::BulkResult => {
                self.screen = Screen::Inbox;
                self.batch = None;
                self.bulk_targets.clear();
            }
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                self.batch_scroll.0 = self.batch_scroll.0.saturating_add(1)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.batch_scroll.0 = self.batch_scroll.0.saturating_sub(1)
            }
            KeyCode::PageDown => self.batch_scroll.0 = self.batch_scroll.0.saturating_add(10),
            KeyCode::PageUp => self.batch_scroll.0 = self.batch_scroll.0.saturating_sub(10),
            KeyCode::Char('h') | KeyCode::Left => {
                self.batch_scroll.1 = self.batch_scroll.1.saturating_sub(8)
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.batch_scroll.1 = self.batch_scroll.1.saturating_add(8)
            }
            KeyCode::Home => self.batch_scroll = (0, 0),
            _ => {}
        }
    }

    fn advance_batch(&mut self) {
        let batch = self.batch.as_mut().expect("batch exists");
        batch.step();
        if !batch.running {
            self.finish_batch();
        }
    }

    fn finish_batch(&mut self) {
        let batch = self.batch.as_ref().expect("batch exists");
        let summary = batch.summary();
        let former_index = self.selected().unwrap_or(0);
        // Consume successful marks even if a path reappears before the refresh.
        for item in &batch.entries {
            let current_identity = std::fs::symlink_metadata(item.entry.path())
                .ok()
                .map(|metadata| SourceIdentity::from_metadata(&metadata));
            if item.outcome == EntryOutcome::Completed || current_identity != item.entry.identity()
            {
                self.marks.remove(&item.entry);
            }
        }
        match (self.inbox_scanner)(&self.inbox_path) {
            Ok(entries) => self.replace_inbox_entries(entries),
            Err(error) => {
                self.entries.retain(|entry| {
                    !batch.entries.iter().any(|item| {
                        item.outcome == EntryOutcome::Completed
                            && item.entry.path() == entry.path()
                            && item.entry.identity() == entry.identity()
                    })
                });
                self.notice = Some(format!(
                    "{summary}; Inbox refresh failed: {error}. Remaining entries may be stale; R retries from Inbox"
                ));
            }
        }
        self.marks.retain_present(&self.entries);
        self.selection
            .repair_after_removal(former_index, self.entries.len());
        if self.notice.is_none() {
            self.notice = Some(summary);
        }
        self.screen = Screen::BulkResult;
        self.batch_scroll = (0, 0);
    }

    fn toggle_ignored_view(&mut self) {
        self.viewing_ignored = !self.viewing_ignored;
        std::mem::swap(&mut self.entries, &mut self.other_entries);
        self.selection = Selection::new(self.entries.len());
        self.marks.clear();
        self.evaluate_rule_matches();
    }

    fn replace_inbox_entries(&mut self, entries: Vec<InboxEntry>) {
        (self.entries, self.other_entries) = entries
            .into_iter()
            .partition(|entry| self.ignored.contains(entry) == self.viewing_ignored);
        self.evaluate_rule_matches();
    }

    fn change_ignored(&mut self, ignore: bool) {
        let indices = if self.marks.count() == 0 {
            self.selected().into_iter().collect::<Vec<_>>()
        } else {
            self.entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| self.marked(entry).then_some(index))
                .collect()
        };
        if indices.is_empty() {
            return;
        }
        let targets = indices
            .iter()
            .map(|index| &self.entries[*index])
            .collect::<Vec<_>>();
        let durability_warning = match self.ignored.update(&targets, ignore) {
            Ok(warning) => warning,
            Err(error) => {
                self.notice = Some(format!(
                    "{} failed; no entries changed: {error}",
                    if ignore { "Ignore" } else { "Restore" }
                ));
                return;
            }
        };
        let count = indices.len();
        let former_index = self.selected().unwrap_or(0);
        let mut retained = Vec::new();
        for (index, entry) in self.entries.drain(..).enumerate() {
            if indices.binary_search(&index).is_ok() {
                self.other_entries.push(entry);
            } else {
                retained.push(entry);
            }
        }
        self.entries = retained;
        inbox::sort_entries(&mut self.other_entries);
        self.marks.clear();
        self.evaluate_rule_matches();
        self.selection
            .repair_after_removal(former_index, self.entries.len());
        self.notice = Some(format!(
            "{} {count} entries; files were not moved or deleted{}",
            if ignore { "Ignored" } else { "Restored" },
            durability_warning
                .map(|warning| format!("; {warning}"))
                .unwrap_or_default()
        ));
    }

    fn evaluate_rule_matches(&mut self) {
        self.rule_matches = self
            .entries
            .iter()
            .map(|entry| self.favorites.suggested_match(entry))
            .collect();
    }

    fn prepare_individual_action(&mut self) -> bool {
        if self.marks.count() > 1 {
            self.notice =
                Some("This action supports one entry; clear marks or select just one".to_owned());
            return false;
        }
        if self.marks.count() == 1 {
            let Some(index) = self
                .entries
                .iter()
                .position(|entry| self.marks.contains(entry))
            else {
                self.notice =
                    Some("The marked entry is unavailable; press R to refresh".to_owned());
                return false;
            };
            let entry = &self.entries[index];
            let current = std::fs::symlink_metadata(entry.path())
                .ok()
                .map(|metadata| SourceIdentity::from_metadata(&metadata));
            if current.is_none() || current != entry.identity() {
                self.notice = Some(
                    "The marked entry changed or is unavailable; press R to refresh".to_owned(),
                );
                return false;
            }
            self.selection.index = Some(index);
        }
        self.marks.exit_visual();
        self.selected().is_some()
    }

    fn refresh_inbox(&mut self) {
        self.marks.exit_visual();
        let former_index = self.selected().unwrap_or(0);
        let former = self.selected().map(|index| {
            let entry = &self.entries[index];
            (entry.path().to_path_buf(), entry.identity())
        });
        match (self.inbox_scanner)(&self.inbox_path) {
            Ok(entries) => {
                self.ignored =
                    IgnoredEntries::load(self.inbox_path.parent().expect("Inbox has HOME parent"));
                self.replace_inbox_entries(entries);
                self.marks.retain_present(&self.entries);
                self.selection.index = former.and_then(|(path, identity)| {
                    self.entries
                        .iter()
                        .position(|entry| entry.path() == path && entry.identity() == identity)
                });
                if self.selected().is_none() {
                    self.selection
                        .repair_after_removal(former_index, self.entries.len());
                }
                self.notice = Some("Inbox refreshed".to_owned());
            }
            Err(error) => {
                self.notice = Some(format!(
                    "Inbox refresh failed; entries may be stale: {error}. Press R to retry"
                ))
            }
        }
    }

    fn start_rename(&mut self) {
        let Some(index) = self.selected() else { return };
        let entry = &self.entries[index];
        let Some(proposal) = ProposedMove::new(entry, &self.inbox_path) else {
            return;
        };
        match SourceIdentity::capture(&proposal) {
            Ok(identity) => {
                self.rename_editor = entry.path().file_name().map(FilenameEditor::new);
                self.source_identity = Some(identity);
                self.proposed_move = None;
                self.move_error = None;
                self.pending_g = false;
                self.screen = Screen::RenameEditor;
            }
            Err(error) => self.notice = Some(format!("Cannot rename: {error}")),
        }
    }

    fn start_move_name_edit(&mut self) {
        let proposal = self
            .proposed_move
            .as_ref()
            .expect("Move Preview has a proposal");
        if self.source_identity.is_none() {
            match SourceIdentity::capture(proposal) {
                Ok(identity) => self.source_identity = Some(identity),
                Err(error) => {
                    self.move_error = Some(error.to_string());
                    return;
                }
            }
        }
        self.rename_editor = proposal
            .resulting_path()
            .file_name()
            .map(FilenameEditor::new);
        self.move_error = None;
        self.screen = Screen::MoveNameEditor;
    }

    fn build_move_proposal(&self) -> Option<ProposedMove> {
        let entry = &self.entries[self.selected()?];
        let destination = self.destination_browser.current();
        match &self.move_basename {
            Some(name) => ProposedMove::with_basename(entry, destination, name).ok(),
            None => ProposedMove::new(entry, destination),
        }
    }

    fn handle_rename_editor(&mut self, key: KeyEvent) {
        let editing_move = self.screen == Screen::MoveNameEditor;
        match key.code {
            KeyCode::Esc if editing_move => {
                self.rename_editor = None;
                self.move_error = None;
                self.screen = Screen::MovePreview;
            }
            KeyCode::Esc => self.cancel_rename(),
            KeyCode::Enter => {
                let index = self.selected().expect("rename has a selected entry");
                let name = self.rename_name().expect("rename has an editor");
                let destination = if editing_move {
                    self.proposed_move
                        .as_ref()
                        .expect("move editor has a proposal")
                        .destination()
                } else {
                    &self.inbox_path
                };
                match ProposedMove::with_basename(&self.entries[index], destination, &name) {
                    Ok(proposal) => {
                        self.proposed_move = Some(proposal);
                        self.move_error = None;
                        self.screen = if editing_move {
                            self.move_basename = Some(name);
                            self.rename_editor = None;
                            Screen::MovePreview
                        } else {
                            Screen::RenamePreview
                        };
                    }
                    Err(error) => self.move_error = Some(error),
                }
            }
            _ => {
                let editor = self.rename_editor.as_mut().expect("rename has an editor");
                match key.code {
                    KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => editor.clear(),
                    KeyCode::Char(character)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        editor.append(character);
                    }
                    KeyCode::Backspace => editor.backspace(),
                    _ => return,
                }
                self.move_error = None;
            }
        }
    }

    fn cancel_rename(&mut self) {
        self.screen = Screen::Inbox;
        self.rename_editor = None;
        self.proposed_move = None;
        self.source_identity = None;
        self.move_error = None;
    }

    fn handle_delete_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.delete_review = None;
                self.delete_confirmation.clear();
                self.move_error = None;
                self.screen = Screen::Inbox;
            }
            KeyCode::Enter if self.delete_confirmation == "delete" => {
                // Every attempt, including retry after partial failure, needs fresh typed consent.
                self.delete_confirmation.clear();
                self.attempt_delete();
            }
            KeyCode::Enter => {
                if self.move_error.is_none() {
                    self.move_error = Some("Type exactly delete, then Enter; Esc cancels".into());
                }
            }
            KeyCode::Backspace => {
                self.delete_confirmation.pop();
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                self.delete_confirmation.clear()
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.delete_confirmation.push(character);
            }
            KeyCode::Down => self.batch_scroll.0 = self.batch_scroll.0.saturating_add(1),
            KeyCode::Up => self.batch_scroll.0 = self.batch_scroll.0.saturating_sub(1),
            KeyCode::Right => self.batch_scroll.1 = self.batch_scroll.1.saturating_add(8),
            KeyCode::Left => self.batch_scroll.1 = self.batch_scroll.1.saturating_sub(8),
            KeyCode::Home => self.batch_scroll = (0, 0),
            _ => {}
        }
    }

    fn attempt_delete(&mut self) {
        let review = self
            .delete_review
            .as_ref()
            .expect("Delete confirmation has a review");
        if let Err(error) = review.execute() {
            self.move_error = Some(format!("Deletion failed: {error}"));
            return;
        }
        let source = review.source.clone();
        let index = self.selected().unwrap_or(0);
        let notice = match (self.inbox_scanner)(&self.inbox_path) {
            Ok(entries) => {
                self.replace_inbox_entries(entries);
                "Permanently deleted 1 entry".to_owned()
            }
            Err(error) => {
                self.entries.retain(|entry| entry.path() != source);
                format!(
                    "Permanently deleted 1 entry; Inbox refresh failed: {error}. Remaining entries may be stale; R retries refresh"
                )
            }
        };
        self.selection
            .repair_after_removal(index, self.entries.len());
        self.marks.clear();
        self.delete_review = None;
        self.move_error = None;
        self.screen = Screen::Inbox;
        self.notice = Some(notice);
    }

    fn attempt_trash(&mut self) {
        let review = self
            .trash_review
            .as_ref()
            .expect("Trash preview has a review");
        if let Err(error) = review.execute(&self.trash_root) {
            self.move_error = Some(format!("Trash failed: {error}"));
            return;
        }
        let source = review.source.clone();
        let index = self.selected().unwrap_or(0);
        let notice = match (self.inbox_scanner)(&self.inbox_path) {
            Ok(entries) => {
                self.replace_inbox_entries(entries);
                "Sent to Trash; restore through your file manager".to_owned()
            }
            Err(error) => {
                self.entries.retain(|entry| entry.path() != source);
                format!(
                    "Sent to Trash; Inbox refresh failed: {error}. Remaining entries may be stale; R retries refresh"
                )
            }
        };
        self.selection
            .repair_after_removal(index, self.entries.len());
        self.marks.clear();
        self.trash_review = None;
        self.move_error = None;
        self.screen = Screen::Inbox;
        self.notice = Some(notice);
    }

    fn attempt_rename(&mut self) {
        let index = self.selected().expect("rename has a selected entry");
        let proposal = self.proposed_move.as_ref().expect("rename has a proposal");
        if !proposal.is_valid() {
            return;
        }
        let identity = self
            .source_identity
            .expect("rename captured source identity");
        if let Err(error) = move_execution::execute_move(proposal, &self.entries[index], identity) {
            self.move_error = Some(error.to_string());
            return;
        }

        let result = proposal.resulting_path().to_path_buf();
        let notice = match (self.inbox_scanner)(&self.inbox_path) {
            Ok(entries) => {
                self.replace_inbox_entries(entries);
                "Rename completed successfully".to_owned()
            }
            Err(error) => {
                self.entries[index].set_path(result.clone());
                inbox::sort_entries(&mut self.entries);
                format!(
                    "Rename completed successfully; Inbox refresh failed: {error}. Remaining entries may be stale"
                )
            }
        };
        self.selection.index = self.entries.iter().position(|entry| entry.path() == result);
        if self.selection.index.is_none() {
            self.selection
                .repair_after_removal(index, self.entries.len());
        }
        self.cancel_rename();
        self.notice = Some(notice);
        self.marks.clear();
    }

    fn attempt_move(&mut self) {
        let Some(index) = self.selection.index() else {
            return;
        };
        let Some(identity) = self.source_identity else {
            self.move_error = Some("the reviewed source identity is unavailable".to_owned());
            return;
        };
        let Some(proposal) = self.proposed_move.as_ref() else {
            self.move_error = Some("the reviewed move is unavailable".to_owned());
            return;
        };
        let moved_source = proposal.source().to_path_buf();
        if let Err(error) = move_execution::execute_move(proposal, &self.entries[index], identity) {
            self.move_error = Some(error.to_string());
            return;
        }

        let refresh = (self.inbox_scanner)(&self.inbox_path);
        let notice = match refresh {
            Ok(entries) => {
                self.replace_inbox_entries(entries);
                "Move completed successfully".to_owned()
            }
            Err(error) => {
                self.entries.retain(|entry| entry.path() != moved_source);
                format!(
                    "Move completed successfully; Inbox refresh failed: {error}. Remaining entries may be stale"
                )
            }
        };

        self.selection
            .repair_after_removal(index, self.entries.len());
        self.screen = Screen::Inbox;
        self.proposed_move = None;
        self.source_identity = None;
        self.move_error = None;
        self.notice = Some(notice);
        self.marks.clear();
        self.move_basename = None;
    }

    fn return_to_refreshed_preview(&mut self) {
        self.proposed_move = self.build_move_proposal();
        self.source_identity = if self.marks.count() == 1 {
            self.selected()
                .and_then(|index| self.entries[index].identity())
        } else {
            None
        };
        self.move_error = None;
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

    fn move_to_first(&mut self, entry_count: usize) {
        self.index = (entry_count > 0).then_some(0);
    }

    fn move_to_last(&mut self, entry_count: usize) {
        self.index = entry_count.checked_sub(1);
    }
}

fn contextual_io_error(context: &'static str, source: io::Error) -> io::Error {
    io::Error::new(source.kind(), format!("{context}: {source}"))
}

#[cfg(test)]
mod tests {
    use std::{fs, io, path::Path};

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use crate::inbox::InboxEntry;

    use crate::proposed_move::{ProposedEntryType, ProposedMove};

    use super::{App, RuleKind, Screen, Selection};
    use crate::rule_match::RuleMatch;

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
    fn vim_jumps_handle_empty_single_and_long_inbox_lists() {
        let mut empty = app_with_entries(0);
        press(&mut empty, KeyCode::Char('g'));
        press(&mut empty, KeyCode::Char('g'));
        press(&mut empty, KeyCode::Char('G'));
        assert_eq!(empty.selected(), None);

        let mut single = app_with_entries(1);
        press(&mut single, KeyCode::Char('G'));
        press(&mut single, KeyCode::Char('g'));
        press(&mut single, KeyCode::Char('g'));
        assert_eq!(single.selected(), Some(0));

        let mut long = app_with_entries(20);
        press(&mut long, KeyCode::Char('G'));
        assert_eq!(long.selected(), Some(19));
        press(&mut long, KeyCode::Char('g'));
        assert_eq!(long.selected(), Some(19));
        press(&mut long, KeyCode::Char('g'));
        assert_eq!(long.selected(), Some(0));
    }

    #[test]
    fn vim_jumps_work_in_destination_browser_and_sequences_are_cancelled() {
        let root = std::env::temp_dir().join(format!(
            "downloads-janitor-app-vim-jumps-{}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        for index in 0..12 {
            fs::create_dir(root.join(format!("directory-{index:02}"))).unwrap();
        }
        let mut app = app_with_entries(2);
        app.destination_browser = crate::destination::DestinationBrowser::new(root.clone());

        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.destination_selected(), Some(0));

        press(&mut app, KeyCode::Char('G'));
        assert_eq!(app.destination_selected(), Some(11));
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.destination_selected(), Some(11));
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.destination_selected(), Some(0));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configuration_adds_edits_deletes_and_reloads_favorites_without_affecting_inbox() {
        let home = std::env::temp_dir().join(format!(
            "downloads-janitor-app-favorites-{}",
            std::process::id()
        ));
        let projects = home.join("Projects");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&projects).unwrap();
        let mut app = App::new(Vec::new(), home.clone());

        press(&mut app, KeyCode::Char('C'));
        assert_eq!(app.screen(), Screen::Configuration);
        press(&mut app, KeyCode::Char('a'));
        for character in "Projects".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::FavoriteDestinationBrowser);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.screen(), Screen::Configuration);
        assert_eq!(app.favorites()[0].name(), "Projects");
        assert_eq!(app.favorites()[0].path(), projects);

        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Char('2'));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.favorites()[0].name(), "Projects2");
        assert_eq!(
            App::new(Vec::new(), home.clone()).favorites()[0].name(),
            "Projects2"
        );

        press(&mut app, KeyCode::Char('x'));
        assert!(app.favorites().is_empty());
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::Inbox);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn configuration_manages_ordered_rules_that_reference_favorites() {
        let home = std::env::temp_dir().join(format!(
            "downloads-janitor-app-rules-{}",
            std::process::id()
        ));
        fs::create_dir(&home).unwrap();
        fs::create_dir(home.join("Archive")).unwrap();
        let mut app = App::new(Vec::new(), home.clone());
        press(&mut app, KeyCode::Char('C'));
        press(&mut app, KeyCode::Char('a'));
        for character in "archive".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.favorites().len(), 1);

        press(&mut app, KeyCode::Char('A'));
        for character in "*.rs".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.rules().len(), 1);
        assert_eq!(app.rules()[0].pattern(), "*.rs");
        assert_eq!(app.rules()[0].kind(), RuleKind::File);
        assert_eq!(app.rules()[0].favorite_name(), "archive");
        assert_eq!(
            App::new(Vec::new(), home.clone()).rules()[0].pattern(),
            "*.rs"
        );

        press(&mut app, KeyCode::Char('X'));
        assert!(app.rules().is_empty());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn rule_matches_use_first_ordered_basename_kind_match_and_leave_raw_names_manual() {
        use std::os::unix::ffi::OsStringExt;

        let home = std::env::temp_dir().join(format!(
            "downloads-janitor-app-rule-matches-{}",
            std::process::id()
        ));
        let projects = home.join("Projects");
        let archive = home.join("Archive");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&projects).unwrap();
        fs::create_dir(&archive).unwrap();
        let raw_path = home.join(std::ffi::OsString::from_vec(b"raw-\xff".to_vec()));
        fs::write(&raw_path, b"raw").unwrap();
        let entries = vec![
            InboxEntry::test_file("main.rs"),
            InboxEntry::test_directory("notes"),
            InboxEntry::test_symlink("source-link", crate::inbox::EntryKind::File),
            InboxEntry::test_entry(raw_path, crate::inbox::EntryKind::File, false),
        ];
        let mut app = App::new(entries, home.clone());
        app.favorites
            .add("projects".into(), projects.clone())
            .unwrap();
        app.favorites.add("archive".into(), archive).unwrap();
        app.favorites
            .add_rule("*.rs".into(), RuleKind::File, "projects".into())
            .unwrap();
        app.favorites
            .add_rule("main.*".into(), RuleKind::Any, "archive".into())
            .unwrap();
        app.favorites
            .add_rule("notes".into(), RuleKind::Directory, "archive".into())
            .unwrap();
        app.favorites
            .add_rule("source-*".into(), RuleKind::Symlink, "projects".into())
            .unwrap();
        app.evaluate_rule_matches();

        let RuleMatch::Suggested(suggestion) = app.rule_match(0).unwrap() else {
            panic!("expected suggestion")
        };
        assert_eq!(suggestion.rule_index(), 0, "first matching Rule wins");
        assert_eq!(suggestion.path(), projects);
        assert!(matches!(app.rule_match(1), Some(RuleMatch::Suggested(_))));
        let RuleMatch::Suggested(symlink) = app.rule_match(2).unwrap() else {
            panic!("expected symlink suggestion")
        };
        assert_eq!(symlink.rule_index(), 3);
        assert_eq!(app.rule_match(3), Some(&RuleMatch::Unmatched));
        fs::remove_dir_all(home).unwrap();
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
    fn q_requests_quit_from_every_non_editing_screen() {
        for initial_screen in [
            Screen::Inbox,
            Screen::DestinationBrowser,
            Screen::MovePreview,
            Screen::RenamePreview,
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
    fn valid_preview_executes_directly() {
        let root = std::env::temp_dir().join(format!(
            "downloads-janitor-app-direct-move-{}",
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
        let resulting_path = app.proposed_move().unwrap().resulting_path().to_path_buf();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
        assert!(fs::symlink_metadata(resulting_path).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_preview_refuses_move() {
        let mut app = app_with_entries(1);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));

        assert!(!app.proposed_move().unwrap().is_valid());
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::MovePreview);
        assert!(app.source_identity.is_none());
        assert_eq!(app.move_error(), None);
    }

    #[test]
    fn preview_requires_explicit_enter_and_ignores_repeats_and_old_binding() {
        let home = std::env::temp_dir().join(format!(
            "downloads-janitor-app-enter-boundary-{}",
            std::process::id()
        ));
        let downloads = home.join("Downloads");
        fs::create_dir_all(&downloads).unwrap();
        let source = downloads.join("source.txt");
        let result = home.join("source.txt");
        fs::write(&source, b"source").unwrap();
        fs::write(downloads.join("remaining.txt"), b"remaining").unwrap();
        let mut app = App::new(crate::inbox::scan_inbox(&downloads).unwrap(), home.clone());
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);

        // Legacy terminals may report held Enter as more Press events. These
        // can only navigate: the separate d binding is the review boundary.
        for _ in 0..4 {
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::DestinationBrowser);
            assert_eq!(fs::read(&source).unwrap(), b"source");
            assert!(!result.exists());
        }
        press(&mut app, KeyCode::Char('d'));
        assert!(app.proposed_move().unwrap().is_valid());
        press(&mut app, KeyCode::Char('m'));
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            app.handle_event(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                kind,
            )));
        }
        assert_eq!(app.screen(), Screen::MovePreview);
        assert!(app.source_identity.is_none());
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert!(!result.exists());

        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::DestinationBrowser);
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert!(!result.exists());

        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
        assert!(!source.exists());
        assert_eq!(fs::read(&result).unwrap(), b"source");
        app.handle_event(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )));
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(app.notice(), Some("Move completed successfully"));
        assert_eq!(
            fs::read(downloads.join("remaining.txt")).unwrap(),
            b"remaining"
        );
        fs::remove_dir_all(home).unwrap();
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
        fs::write(&result, b"collision").unwrap();

        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::MovePreview);
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
        fs::remove_file(&source).unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::MovePreview);

        press(&mut app, KeyCode::Esc);

        assert_eq!(app.screen(), Screen::DestinationBrowser);
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
    fn changed_source_type_is_refused_and_quit_still_works_after_failure() {
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
        fs::rename(&source, root.join("original.txt")).unwrap();
        fs::create_dir(&source).unwrap();

        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::MovePreview);
        assert!(app.move_error().unwrap().contains("entry type"));
        assert!(source.is_dir());
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit);
        fs::remove_dir_all(root).unwrap();
    }

    fn fail_refresh(_: &Path) -> crate::Result<Vec<InboxEntry>> {
        Err(io::Error::other("injected refresh failure").into())
    }

    #[test]
    fn completed_move_with_refresh_failure_uses_stale_safe_fallback() {
        let home = std::env::temp_dir().join(format!(
            "downloads-janitor-app-refresh-failure-{}",
            std::process::id()
        ));
        let downloads = home.join("Downloads");
        let source = downloads.join("alpha.txt");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&downloads).unwrap();
        fs::write(&source, b"alpha").unwrap();
        fs::write(downloads.join("beta.txt"), b"beta").unwrap();
        let entries = crate::inbox::scan_inbox(&downloads).unwrap();
        let mut app = App::with_inbox_scanner(entries, home.clone(), fail_refresh);

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::Inbox);
        assert!(!source.exists());
        assert_eq!(fs::read(home.join("alpha.txt")).unwrap(), b"alpha");
        assert_eq!(app.entries().len(), 1);
        assert_eq!(app.entries()[0].path(), downloads.join("beta.txt"));
        assert_eq!(app.selected(), Some(0));
        let notice = app.notice().unwrap();
        assert!(notice.contains("Move completed successfully"));
        assert!(notice.contains("Inbox refresh failed"));
        assert!(notice.contains("may be stale"));
        assert_eq!(app.move_error(), None);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn refresh_failure_after_last_move_leaves_empty_selection() {
        let home = std::env::temp_dir().join(format!(
            "downloads-janitor-app-empty-refresh-failure-{}",
            std::process::id()
        ));
        let downloads = home.join("Downloads");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&downloads).unwrap();
        fs::write(downloads.join("only.txt"), b"only").unwrap();
        let entries = crate::inbox::scan_inbox(&downloads).unwrap();
        let mut app = App::with_inbox_scanner(entries, home.clone(), fail_refresh);

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::Inbox);
        assert!(app.entries().is_empty());
        assert_eq!(app.selected(), None);
        fs::remove_dir_all(home).unwrap();
    }
    struct RenameFixture(std::path::PathBuf);

    impl RenameFixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let home = std::env::temp_dir().join(format!(
                "downloads-janitor-rename-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&home).unwrap();
            fs::create_dir(home.join("Downloads")).unwrap();
            Self(home)
        }

        fn path(&self, name: impl AsRef<Path>) -> std::path::PathBuf {
            self.0.join("Downloads").join(name)
        }

        fn app(&self) -> App {
            App::new(
                crate::inbox::scan_inbox(&self.path("")).unwrap(),
                self.0.clone(),
            )
        }
    }

    impl Drop for RenameFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn milestone_four_mixed_workflow_survives_actions_and_restart() {
        let fixture = RenameFixture::new();
        fs::create_dir(fixture.0.join("Archive")).unwrap();
        fs::create_dir(fixture.path("01-bundle")).unwrap();
        fs::write(fixture.path("01-bundle/contents"), b"nested").unwrap();
        for name in ["02-rename", "03-move", "04-ignore", "05-trash", "06-delete"] {
            fs::write(fixture.path(name), name.as_bytes()).unwrap();
        }
        let target = fixture.0.join("sentinel");
        fs::write(&target, b"untouched").unwrap();
        std::os::unix::fs::symlink(&target, fixture.path("07-link")).unwrap();
        let mut app = fixture.app();

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('r'));
        enter_rename_name(&mut app, "02-renamed-q");
        press(&mut app, KeyCode::Enter);
        assert!(fixture.path("02-rename").exists()); // preview is read-only
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.entries()[app.selected().unwrap()].path(),
            fixture.path("02-renamed-q")
        );
        assert!(!app.should_quit);

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter); // first HOME directory is Archive
        press(&mut app, KeyCode::Char('d'));
        edit_move_name(&mut app, "03-moved-q");
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            fs::read(fixture.0.join("Archive/03-moved-q")).unwrap(),
            b"03-move"
        );
        assert_eq!(
            app.entries()[app.selected().unwrap()].path(),
            fixture.path("04-ignore")
        );

        for code in [
            KeyCode::Char('g'),
            KeyCode::Char('g'),
            KeyCode::Char('V'),
            KeyCode::Down,
            KeyCode::Char('V'),
            KeyCode::Char('G'),
            KeyCode::Char(' '),
        ] {
            press(&mut app, code);
        }
        assert_eq!(app.marked_count(), 3);
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.screen(), Screen::Inbox); // rename remains individual
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.destination(), Some(fixture.0.join("Archive").as_path()));
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Enter);
        for _ in 0..3 {
            app.advance_batch();
        }
        assert_eq!(
            app.batch().unwrap().summary(),
            "3 completed, 0 failed, 0 unattempted"
        );
        assert_eq!(
            fs::read(fixture.0.join("Archive/01-bundle/contents")).unwrap(),
            b"nested"
        );
        assert_eq!(
            fs::read(fixture.0.join("Archive/02-renamed-q")).unwrap(),
            b"02-rename"
        );
        assert_eq!(
            fs::read_link(fixture.0.join("Archive/07-link")).unwrap(),
            target
        );
        assert_eq!(app.marked_count(), 0);
        press(&mut app, KeyCode::Enter);

        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('i'));
        drop(app);
        let mut app = fixture.app();
        app.trash_root = fixture.0.join(".local/share/Trash");
        assert_eq!(app.entries().len(), 2);
        press(&mut app, KeyCode::Char('I'));
        assert_eq!(app.entries().len(), 1);
        assert_eq!(app.entries()[0].path(), fixture.path("04-ignore"));
        press(&mut app, KeyCode::Char('u'));
        press(&mut app, KeyCode::Char('I'));
        assert_eq!(app.entries().len(), 3);

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('t'));
        assert!(fixture.path("05-trash").exists());
        press(&mut app, KeyCode::Enter);
        let payload = fs::read_dir(app.trash_root.join("files"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(payload.path()).unwrap(), b"05-trash");
        let metadata = fs::read_dir(app.trash_root.join("info"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert!(
            fs::read_to_string(metadata.path())
                .unwrap()
                .contains("/Downloads/05-trash")
        );
        assert_eq!(
            app.entries()[app.selected().unwrap()].path(),
            fixture.path("06-delete")
        );
        press(&mut app, KeyCode::Char('D'));
        press(&mut app, KeyCode::Enter);
        assert!(fixture.path("06-delete").exists());
        confirm_delete(&mut app);
        assert!(!fixture.path("06-delete").exists());
        assert_eq!(app.entries().len(), 1);
        assert_eq!(
            app.entries()[app.selected().unwrap()].path(),
            fixture.path("04-ignore")
        );
        assert_eq!(fixture.app().entries().len(), 1);
        assert_eq!(fs::read(target).unwrap(), b"untouched");
    }

    fn confirm_delete(app: &mut App) {
        for character in "delete".chars() {
            press(app, KeyCode::Char(character));
        }
        press(app, KeyCode::Enter);
    }

    #[test]
    fn delete_requires_exact_typed_confirmation_and_escape_preserves_mark() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("a"), b"keep").unwrap();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char('D'));
        assert_eq!(app.screen(), Screen::DeleteConfirmation);
        for input in ["", "Delete", "delete ", "deletex", "q"] {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::CONTROL,
            )));
            for character in input.chars() {
                press(&mut app, KeyCode::Char(character));
            }
            press(&mut app, KeyCode::Enter);
            assert!(fixture.path("a").exists());
            assert!(!app.should_quit);
        }
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(app.marked_count(), 1);
        press(&mut app, KeyCode::Char('D'));
        assert!(app.delete_confirmation.is_empty());
        for character in "delete".chars() {
            app.handle_event(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char(character),
                KeyModifiers::NONE,
                KeyEventKind::Repeat,
            )));
        }
        press(&mut app, KeyCode::Enter);
        assert!(fixture.path("a").exists());
        for character in "deletex".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Backspace);
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            app.handle_event(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                kind,
            )));
        }
        assert!(fixture.path("a").exists());
        press(&mut app, KeyCode::Enter);
        assert!(!fixture.path("a").exists());
        assert_eq!(app.selected(), None);
        assert_eq!(app.marked_count(), 0);
    }

    #[test]
    fn delete_success_repairs_cursor_and_preserves_success_on_refresh_failure() {
        for failure in [false, true] {
            let (fixture, mut app) = trash_fixture();
            press(&mut app, KeyCode::Down);
            press(&mut app, KeyCode::Char(' '));
            press(&mut app, KeyCode::Down);
            if failure {
                app.inbox_scanner = fail_refresh;
            }
            press(&mut app, KeyCode::Char('D'));
            assert_eq!(
                app.delete_review.as_ref().unwrap().source,
                fixture.path("b")
            );
            confirm_delete(&mut app);
            assert_eq!(app.screen(), Screen::Inbox);
            assert!(!fixture.path("b").exists());
            assert_eq!(
                app.entries()[app.selected().unwrap()].path(),
                fixture.path("c")
            );
            assert_eq!(app.marked_count(), 0);
            assert!(
                app.notice()
                    .unwrap()
                    .contains("Permanently deleted 1 entry")
            );
            assert_eq!(app.notice().unwrap().contains("refresh failed"), failure);
            assert!(!app.trash_root.exists());
        }
    }

    #[test]
    fn delete_refuses_replaced_or_missing_reviewed_source() {
        for replacement in [None, Some(false), Some(true)] {
            let fixture = RenameFixture::new();
            fs::write(fixture.path("a"), b"original").unwrap();
            let mut app = fixture.app();
            press(&mut app, KeyCode::Char('D'));
            fs::rename(fixture.path("a"), fixture.path("original")).unwrap();
            if let Some(directory) = replacement {
                if directory {
                    fs::create_dir(fixture.path("a")).unwrap();
                } else {
                    fs::write(fixture.path("a"), b"replacement").unwrap();
                }
            }
            confirm_delete(&mut app);
            assert_eq!(app.screen(), Screen::DeleteConfirmation);
            assert!(app.move_error().unwrap().contains("Deletion failed"));
            assert!(app.delete_confirmation.is_empty());
            assert_eq!(fs::read(fixture.path("original")).unwrap(), b"original");
            if replacement.is_some() {
                assert!(fixture.path("a").exists());
            }
        }
    }

    #[test]
    fn delete_nonempty_directory_never_follows_nested_symlinks() {
        let fixture = RenameFixture::new();
        let target = fixture.0.join("outside");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"keep").unwrap();
        fs::create_dir_all(fixture.path("folder/nested")).unwrap();
        fs::write(fixture.path("folder/nested/remove"), b"remove").unwrap();
        std::os::unix::fs::symlink(&target, fixture.path("folder/link")).unwrap();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('D'));
        confirm_delete(&mut app);
        assert!(!fixture.path("folder").exists());
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"keep");
        assert!(app.entries().is_empty());
    }

    #[test]
    fn delete_symlink_entries_preserves_file_and_directory_targets() {
        for directory in [false, true] {
            let fixture = RenameFixture::new();
            let target = fixture.0.join("target");
            if directory {
                fs::create_dir(&target).unwrap();
            } else {
                fs::write(&target, b"keep").unwrap();
            }
            std::os::unix::fs::symlink(&target, fixture.path("link")).unwrap();
            let mut app = fixture.app();
            press(&mut app, KeyCode::Char('D'));
            confirm_delete(&mut app);
            assert!(fs::symlink_metadata(fixture.path("link")).is_err());
            assert!(target.exists());
        }
    }

    #[test]
    fn delete_partial_recursive_failure_is_truthful_and_retry_needs_new_consent() {
        use std::os::unix::fs::PermissionsExt;
        // An ordinary user can remove contents but cannot unlink the root from this parent.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let fixture = RenameFixture::new();
        fs::create_dir(fixture.path("folder")).unwrap();
        fs::write(fixture.path("folder/child"), b"remove").unwrap();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char('D'));
        fs::set_permissions(fixture.path(""), fs::Permissions::from_mode(0o500)).unwrap();
        confirm_delete(&mut app);
        fs::set_permissions(fixture.path(""), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(fixture.path("folder").exists());
        assert!(!fixture.path("folder/child").exists());
        assert_eq!(app.screen(), Screen::DeleteConfirmation);
        assert!(
            app.move_error()
                .unwrap()
                .contains("some directory contents may already have been permanently deleted")
        );
        assert!(app.delete_confirmation.is_empty());
        assert_eq!(app.marked_count(), 1);
        press(&mut app, KeyCode::Enter);
        assert!(fixture.path("folder").exists());
        assert!(
            app.move_error()
                .unwrap()
                .contains("some directory contents")
        );
        confirm_delete(&mut app);
        assert!(!fixture.path("folder").exists());
        assert_eq!(app.screen(), Screen::Inbox);
    }

    #[test]
    fn delete_reviews_multiple_marks_and_refuses_ignored_entries() {
        let (fixture, mut app) = trash_fixture();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('D'));
        assert_eq!(app.screen(), Screen::BulkPreview);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('i'));
        press(&mut app, KeyCode::Char('I'));
        press(&mut app, KeyCode::Char('D'));
        assert_eq!(app.screen(), Screen::Inbox);
        assert!(fixture.path("a").exists());
    }

    fn removal_batch_fixture(delete: bool) -> (RenameFixture, App) {
        let (fixture, mut app) = trash_fixture();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('G'));
        press(&mut app, KeyCode::Char(if delete { 'D' } else { 't' }));
        assert_eq!(app.screen(), Screen::BulkPreview);
        (fixture, app)
    }

    fn authorize_removal(app: &mut App, delete: bool) {
        if delete {
            confirm_delete(app);
        } else {
            press(app, KeyCode::Enter);
        }
    }

    #[test]
    fn removal_batches_preflight_entire_set_and_require_new_consent_after_block() {
        for delete in [false, true] {
            let (fixture, mut app) = removal_batch_fixture(delete);
            assert!(!app.trash_root.exists()); // even preflight creates no Trash storage
            fs::rename(fixture.path("c"), fixture.0.join("original-c")).unwrap();
            fs::write(fixture.path("c"), b"replacement").unwrap();
            authorize_removal(&mut app, delete);
            assert_eq!(app.screen(), Screen::BulkPreview);
            assert!(!app.batch().unwrap().entries[2].problems.is_empty());
            for name in ["a", "b", "c"] {
                assert!(fixture.path(name).exists());
            }
            assert!(app.delete_confirmation.is_empty());
            assert!(!app.trash_root.exists());
            press(&mut app, KeyCode::Esc);
            assert_eq!(app.marked_count(), 3);
        }
    }

    #[test]
    fn removal_batches_succeed_in_order_and_cannot_replay_results() {
        for delete in [false, true] {
            let (fixture, mut app) = removal_batch_fixture(delete);
            authorize_removal(&mut app, delete);
            assert_eq!(app.screen(), Screen::BulkProgress);
            for name in ["a", "b", "c"] {
                assert!(fixture.path(name).exists());
                app.advance_batch();
                assert!(!fixture.path(name).exists());
            }
            assert_eq!(app.screen(), Screen::BulkResult);
            assert_eq!(
                app.batch().unwrap().summary(),
                "3 completed, 0 failed, 0 unattempted"
            );
            assert_eq!(app.marked_count(), 0);
            assert_eq!(app.selected(), None);
            app.batch.as_mut().unwrap().authorize();
            assert!(!app.batch().unwrap().running);
            if !delete {
                assert_eq!(
                    fs::read_dir(app.trash_root.join("files")).unwrap().count(),
                    3
                );
                assert_eq!(
                    fs::read_dir(app.trash_root.join("info")).unwrap().count(),
                    3
                );
            }
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::Inbox);
            assert!(app.batch().is_none());
        }
    }

    #[test]
    fn removal_batches_stop_between_entries_and_keep_remaining_marks_on_refresh_failure() {
        for delete in [false, true] {
            for completed in [0, 1] {
                for refresh_failure in [false, true] {
                    let (fixture, mut app) = removal_batch_fixture(delete);
                    if refresh_failure {
                        app.inbox_scanner = fail_refresh;
                    }
                    authorize_removal(&mut app, delete);
                    if completed == 1 {
                        app.advance_batch();
                    }
                    press(&mut app, KeyCode::Esc);
                    assert_eq!(app.screen(), Screen::BulkResult);
                    assert_eq!(app.marked_count(), 3 - completed);
                    assert_eq!(app.entries().len(), 3 - completed);
                    assert_eq!(fixture.path("a").exists(), completed == 0);
                    assert!(fixture.path("b").exists());
                    assert!(fixture.path("c").exists());
                    assert_eq!(
                        app.entries()[app.selected().unwrap()].path(),
                        fixture.path("c")
                    );
                    assert!(app.batch().unwrap().summary().contains("stopped"));
                    assert_eq!(
                        app.notice().unwrap().contains("refresh failed"),
                        refresh_failure
                    );
                }
            }
        }
    }

    #[test]
    fn removal_batches_revalidate_middle_entry_and_retain_only_matching_marks() {
        for delete in [false, true] {
            let (fixture, mut app) = removal_batch_fixture(delete);
            authorize_removal(&mut app, delete);
            app.advance_batch();
            fs::rename(fixture.path("b"), fixture.0.join("original-b")).unwrap();
            fs::write(fixture.path("b"), b"replacement").unwrap();
            app.advance_batch();
            assert_eq!(app.screen(), Screen::BulkResult);
            assert_eq!(
                app.batch().unwrap().summary(),
                "1 completed, 1 failed, 1 unattempted"
            );
            assert_eq!(fs::read(fixture.path("b")).unwrap(), b"replacement");
            assert!(fixture.path("c").exists());
            assert_eq!(app.marked_count(), 1);
            assert!(
                app.marked(
                    app.entries()
                        .iter()
                        .find(|e| e.path() == fixture.path("c"))
                        .unwrap()
                )
            );
        }
    }

    #[test]
    fn bulk_delete_confirmation_is_exact_cancellable_and_ignores_repeats() {
        let (fixture, mut app) = removal_batch_fixture(true);
        for text in ["", "Delete", "delete ", "q"] {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('u'),
                KeyModifiers::CONTROL,
            )));
            for character in text.chars() {
                press(&mut app, KeyCode::Char(character));
            }
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::BulkPreview);
            assert!(!app.should_quit);
        }
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.marked_count(), 3);
        press(&mut app, KeyCode::Char('D'));
        assert!(app.delete_confirmation.is_empty());
        for character in "deletex".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Backspace);
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            app.handle_event(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                kind,
            )));
        }
        assert_eq!(app.screen(), Screen::BulkPreview);
        assert!(fixture.path("a").exists());
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::BulkProgress);
        press(&mut app, KeyCode::Esc);
        assert!(fixture.path("a").exists());
    }

    #[test]
    fn removal_preflight_blocks_unwritable_parent_and_unavailable_trash() {
        use std::os::unix::fs::PermissionsExt;
        for delete in [false, true] {
            if unsafe { libc::geteuid() } == 0 {
                continue;
            }
            let (fixture, mut app) = removal_batch_fixture(delete);
            fs::set_permissions(fixture.path(""), fs::Permissions::from_mode(0o500)).unwrap();
            authorize_removal(&mut app, delete);
            fs::set_permissions(fixture.path(""), fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(app.screen(), Screen::BulkPreview);
            assert!(
                app.batch()
                    .unwrap()
                    .entries
                    .iter()
                    .all(|item| !item.problems.is_empty())
            );
            assert!(fixture.path("a").exists());
        }
        let (fixture, mut app) = removal_batch_fixture(false);
        fs::write(&app.trash_root, b"blocked").unwrap();
        authorize_removal(&mut app, false);
        assert_eq!(app.screen(), Screen::BulkPreview);
        assert!(fixture.path("a").exists());
    }

    #[test]
    fn removal_runtime_failure_keeps_partial_results_and_surviving_marks() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        for delete in [false, true] {
            let fixture = RenameFixture::new();
            fs::write(fixture.path("a"), b"a").unwrap();
            fs::create_dir(fixture.path("b")).unwrap();
            fs::write(fixture.path("b/child"), b"child").unwrap();
            fs::write(fixture.path("c"), b"c").unwrap();
            let mut app = fixture.app();
            app.trash_root = fixture.0.join("Trash");
            press(&mut app, KeyCode::Char('a'));
            press(&mut app, KeyCode::Char(if delete { 'D' } else { 't' }));
            authorize_removal(&mut app, delete);
            app.advance_batch(); // a first, despite directories sorting first in Inbox
            let denied = if delete {
                fixture.path("")
            } else {
                app.trash_root.join("info")
            };
            fs::set_permissions(&denied, fs::Permissions::from_mode(0o500)).unwrap();
            app.advance_batch();
            fs::set_permissions(&denied, fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(app.screen(), Screen::BulkResult);
            assert_eq!(
                app.batch().unwrap().summary(),
                "1 completed, 1 failed, 1 unattempted"
            );
            assert!(fixture.path("b").exists());
            assert_eq!(fixture.path("b/child").exists(), !delete);
            assert!(fixture.path("c").exists());
            assert_eq!(app.marked_count(), 2);
            let crate::batch::EntryOutcome::Failed(error) =
                &app.batch().unwrap().entries[1].outcome
            else {
                panic!("expected failure")
            };
            assert!(error.contains(if delete {
                "some directory contents"
            } else {
                "Trash metadata"
            }));
        }
    }

    #[test]
    fn removal_batches_preserve_symlink_targets_and_handle_nonempty_directories() {
        for delete in [false, true] {
            let fixture = RenameFixture::new();
            let target = fixture.0.join("outside");
            fs::create_dir(&target).unwrap();
            fs::write(target.join("keep"), b"keep").unwrap();
            fs::create_dir(fixture.path("folder")).unwrap();
            fs::write(fixture.path("folder/child"), b"child").unwrap();
            std::os::unix::fs::symlink(&target, fixture.path("link")).unwrap();
            fs::write(fixture.path("file"), b"file").unwrap();
            let mut app = fixture.app();
            app.trash_root = fixture.0.join("Trash");
            press(&mut app, KeyCode::Char('a'));
            press(&mut app, KeyCode::Char(if delete { 'D' } else { 't' }));
            authorize_removal(&mut app, delete);
            while app.screen() == Screen::BulkProgress {
                app.advance_batch();
            }
            assert_eq!(
                app.batch().unwrap().summary(),
                "3 completed, 0 failed, 0 unattempted"
            );
            assert_eq!(fs::read(target.join("keep")).unwrap(), b"keep");
            assert!(app.entries().is_empty());
            if !delete {
                let payloads = fs::read_dir(app.trash_root.join("files"))
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .collect::<Vec<_>>();
                assert!(
                    payloads
                        .iter()
                        .any(|p| fs::symlink_metadata(p).unwrap().is_symlink())
                );
                assert!(
                    payloads
                        .iter()
                        .any(|p| fs::read(p.join("child")).ok().as_deref() == Some(b"child"))
                );
            }
        }
    }

    fn trash_fixture() -> (RenameFixture, App) {
        let fixture = RenameFixture::new();
        for name in ["a", "b", "c"] {
            fs::write(fixture.path(name), name).unwrap();
        }
        let mut app = fixture.app();
        // Never let the host's XDG_DATA_HOME direct tests into personal Trash.
        app.trash_root = fixture.0.join("Trash");
        (fixture, app)
    }

    #[test]
    fn trash_review_requires_separate_enter_and_cancellation_preserves_marks() {
        let (fixture, mut app) = trash_fixture();
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.screen(), Screen::TrashPreview);
        assert_eq!(app.trash_review.as_ref().unwrap().source, fixture.path("a"));
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            app.handle_event(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Enter,
                KeyModifiers::NONE,
                kind,
            )));
        }
        for _ in 0..5 {
            press(&mut app, KeyCode::Char('t'));
        }
        assert!(fixture.path("a").exists());
        assert!(!app.trash_root.exists());
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(app.marked_count(), 1);
        assert!(fixture.path("a").exists());
    }

    #[test]
    fn trash_success_uses_mark_and_repairs_cursor_even_when_refresh_fails() {
        for failed_refresh in [false, true] {
            let (fixture, mut app) = trash_fixture();
            press(&mut app, KeyCode::Down);
            press(&mut app, KeyCode::Char(' ')); // b marked
            press(&mut app, KeyCode::Down); // cursor c
            if failed_refresh {
                app.inbox_scanner = fail_refresh;
            }
            press(&mut app, KeyCode::Char('t'));
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::Inbox);
            assert!(!fixture.path("b").exists());
            assert!(fixture.path("a").exists());
            assert!(fixture.path("c").exists());
            assert_eq!(app.entries().len(), 2);
            assert_eq!(
                app.entries()[app.selected().unwrap()].path(),
                fixture.path("c")
            );
            assert_eq!(app.marked_count(), 0);
            let notice = app.notice().unwrap();
            assert!(notice.contains("Sent to Trash"));
            assert_eq!(notice.contains("refresh failed"), failed_refresh);
        }
    }

    #[test]
    fn trash_replacement_failure_retains_review_and_selection() {
        let (fixture, mut app) = trash_fixture();
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char('t'));
        fs::rename(fixture.path("a"), fixture.path("old-a")).unwrap();
        fs::write(fixture.path("a"), b"replacement").unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::TrashPreview);
        assert!(app.move_error().unwrap().contains("changed"));
        assert_eq!(app.marked_count(), 1);
        assert_eq!(fs::read(fixture.path("a")).unwrap(), b"replacement");
        assert!(!app.trash_root.exists());
    }

    #[test]
    fn trash_reviews_multiple_marks_and_is_disabled_in_ignored_view() {
        let (fixture, mut app) = trash_fixture();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.screen(), Screen::BulkPreview);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('i'));
        press(&mut app, KeyCode::Char('I'));
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.screen(), Screen::Inbox);
        assert!(fixture.path("a").exists());
        assert!(!app.trash_root.exists());
    }

    fn enter_rename_name(app: &mut App, name: &str) {
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        )));
        for character in name.chars() {
            press(app, KeyCode::Char(character));
        }
    }

    #[test]
    fn rename_editing_review_cancellation_and_sorted_selection() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("alpha"), b"contents").unwrap();
        fs::write(fixture.path("middle"), b"other").unwrap();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.screen(), Screen::RenameEditor);
        assert_eq!(app.rename_name().unwrap(), "alpha");
        enter_rename_name(&mut app, "qjgG é");
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.rename_name().unwrap(), "qjgG ");
        assert!(!app.should_quit);
        assert_eq!(app.selected(), Some(0));
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(fs::read(fixture.path("alpha")).unwrap(), b"contents");

        press(&mut app, KeyCode::Char('r'));
        enter_rename_name(&mut app, "zulu");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::RenamePreview);
        assert!(!fixture.path("zulu").exists());
        app.handle_event(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )));
        assert_eq!(app.screen(), Screen::RenamePreview);
        assert!(!fixture.path("zulu").exists());
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::Inbox);
        assert!(fixture.path("alpha").exists());

        press(&mut app, KeyCode::Char('r'));
        enter_rename_name(&mut app, "zulu");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(app.selected(), Some(1));
        assert_eq!(app.entries()[1].path(), fixture.path("zulu"));
        assert_eq!(fs::read(fixture.path("zulu")).unwrap(), b"contents");
        assert!(!fixture.path("alpha").exists());
        assert_eq!(app.notice(), Some("Rename completed successfully"));
    }

    #[test]
    fn rename_rejects_invalid_unchanged_and_occupied_names() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("alpha"), b"source").unwrap();
        fs::write(fixture.path("occupied"), b"other").unwrap();
        std::os::unix::fs::symlink("missing", fixture.path("broken")).unwrap();
        let mut app = fixture.app();
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "/absolute",
            "child/name",
            "nul\0name",
        ] {
            press(&mut app, KeyCode::Char('r'));
            enter_rename_name(&mut app, name);
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::RenameEditor, "{name:?}");
            assert!(app.move_error().is_some());
            press(&mut app, KeyCode::Esc);
        }
        for name in ["alpha", "occupied", "broken", &"x".repeat(256)] {
            press(&mut app, KeyCode::Char('r'));
            enter_rename_name(&mut app, name);
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::RenamePreview);
            assert!(!app.proposed_move().unwrap().is_valid(), "{name:?}");
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::RenamePreview);
            assert_eq!(fs::read(fixture.path("alpha")).unwrap(), b"source");
            assert_eq!(fs::read(fixture.path("occupied")).unwrap(), b"other");
            press(&mut app, KeyCode::Esc);
        }
    }

    #[test]
    fn rename_collision_preserves_review_and_retry_uses_fresh_validation() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("source"), b"source").unwrap();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('r'));
        enter_rename_name(&mut app, "result");
        press(&mut app, KeyCode::Enter);
        fs::write(fixture.path("result"), b"collision").unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::RenamePreview);
        assert!(app.move_error().unwrap().contains("fresh validation"));
        assert_eq!(
            app.proposed_move().unwrap().source(),
            fixture.path("source")
        );
        assert_eq!(
            app.proposed_move().unwrap().resulting_path(),
            fixture.path("result")
        );
        assert_eq!(fs::read(fixture.path("source")).unwrap(), b"source");
        assert_eq!(fs::read(fixture.path("result")).unwrap(), b"collision");
        fs::remove_file(fixture.path("result")).unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(fs::read(fixture.path("result")).unwrap(), b"source");
    }

    #[test]
    fn rename_refuses_source_replacement_during_editing_or_review() {
        for replace_with_directory in [false, true] {
            let fixture = RenameFixture::new();
            fs::write(fixture.path("source"), b"original").unwrap();
            let mut app = fixture.app();
            press(&mut app, KeyCode::Char('r'));
            enter_rename_name(&mut app, "result");
            if replace_with_directory {
                press(&mut app, KeyCode::Enter);
            }
            fs::rename(fixture.path("source"), fixture.path("original")).unwrap();
            if replace_with_directory {
                fs::create_dir(fixture.path("source")).unwrap();
            } else {
                fs::write(fixture.path("source"), b"replacement").unwrap();
                press(&mut app, KeyCode::Enter);
            }
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::RenamePreview);
            assert!(
                app.move_error()
                    .unwrap()
                    .contains("identity or entry type changed")
            );
            assert!(!fixture.path("result").exists());
            assert_eq!(fs::read(fixture.path("original")).unwrap(), b"original");
        }
    }

    #[test]
    fn rename_preserves_non_empty_directories_and_symlink_targets() {
        for symlink in [false, true] {
            let fixture = RenameFixture::new();
            fs::create_dir(fixture.path("source")).unwrap();
            fs::write(fixture.path("source/nested"), b"nested").unwrap();
            if symlink {
                std::os::unix::fs::symlink("source", fixture.path("link")).unwrap();
            }
            let mut app = fixture.app();
            // Directory symlinks sort with directories; link sorts before source.
            press(&mut app, KeyCode::Char('r'));
            enter_rename_name(&mut app, "result");
            press(&mut app, KeyCode::Enter);
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::Inbox);
            assert_eq!(fs::read(fixture.path("result/nested")).unwrap(), b"nested");
            if symlink {
                assert_eq!(
                    fs::read_link(fixture.path("result")).unwrap(),
                    Path::new("source")
                );
                assert!(fixture.path("source").exists());
                assert!(!fixture.path("link").exists());
            } else {
                assert!(!fixture.path("source").exists());
            }
        }
    }

    #[test]
    fn rename_preserves_raw_filename_bytes_through_execution() {
        use std::os::unix::ffi::OsStrExt;
        let fixture = RenameFixture::new();
        let original = std::ffi::OsStr::from_bytes(b"raw-\xff");
        let result = std::ffi::OsStr::from_bytes(b"raw-\xffq");
        fs::write(fixture.path(original), b"raw").unwrap();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.rename_name().unwrap(), original);
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.proposed_move().unwrap().resulting_path(),
            fixture.path(result)
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(fs::read(fixture.path(result)).unwrap(), b"raw");
        assert!(!fixture.path(original).exists());
        assert!(!app.should_quit);
    }

    #[test]
    fn rename_refresh_failure_keeps_success_and_repairs_sorted_selection() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("alpha"), b"source").unwrap();
        fs::write(fixture.path("middle"), b"other").unwrap();
        let mut app = fixture.app();
        app.inbox_scanner = fail_refresh;
        press(&mut app, KeyCode::Char('r'));
        enter_rename_name(&mut app, "zulu");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(app.selected(), Some(1));
        assert_eq!(app.entries()[1].path(), fixture.path("zulu"));
        assert_eq!(app.entries()[1].display_name(), "zulu");
        assert_eq!(fs::read(fixture.path("zulu")).unwrap(), b"source");
        assert!(
            app.notice()
                .unwrap()
                .contains("Rename completed successfully; Inbox refresh failed")
        );
        assert!(app.notice().unwrap().contains("may be stale"));
        assert_eq!(app.move_error(), None);
    }
    fn open_move_preview(app: &mut App) {
        press(app, KeyCode::Enter);
        press(app, KeyCode::Char('d'));
        assert_eq!(app.screen(), Screen::MovePreview);
    }

    fn edit_move_name(app: &mut App, name: &str) {
        press(app, KeyCode::Char('r'));
        assert_eq!(app.screen(), Screen::MoveNameEditor);
        enter_rename_name(app, name);
        press(app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::MovePreview);
    }

    #[test]
    fn edited_move_goes_directly_to_destination_without_inbox_rename() {
        use std::os::unix::fs::MetadataExt;
        let fixture = RenameFixture::new();
        let source = fixture.path("alpha");
        fs::write(&source, b"source").unwrap();
        fs::write(fixture.path("renamed"), b"keep").unwrap();
        let original_inode = fs::symlink_metadata(&source).unwrap().ino();
        let mut app = fixture.app();
        open_move_preview(&mut app);
        edit_move_name(&mut app, "renamed");

        let result = fixture.0.join("renamed");
        assert_eq!(app.proposed_move().unwrap().source(), source);
        assert_eq!(app.proposed_move().unwrap().resulting_path(), result);
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert!(!result.exists());
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(fs::read(&result).unwrap(), b"source");
        assert_eq!(fs::symlink_metadata(&result).unwrap().ino(), original_inode);
        assert!(!source.exists());
        assert_eq!(fs::read(fixture.path("renamed")).unwrap(), b"keep");
        assert_eq!(app.notice(), Some("Move completed successfully"));
    }

    #[test]
    fn move_name_edit_cancellation_and_default_name_are_read_only_until_execution() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("source"), b"source").unwrap();
        let mut app = fixture.app();
        open_move_preview(&mut app);
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.rename_name().unwrap(), "source");
        enter_rename_name(&mut app, "q-cancelled");
        assert!(!app.should_quit);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::MovePreview);
        assert_eq!(
            app.proposed_move().unwrap().resulting_path(),
            fixture.0.join("source")
        );

        // Accepting the default name is valid when the Destination differs.
        press(&mut app, KeyCode::Char('r'));
        press(&mut app, KeyCode::Enter);
        assert!(app.proposed_move().unwrap().is_valid());
        app.handle_event(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )));
        assert!(fixture.path("source").exists());
        assert!(!fixture.0.join("source").exists());
        press(&mut app, KeyCode::Enter);
        assert_eq!(fs::read(fixture.0.join("source")).unwrap(), b"source");
        assert!(!fixture.0.join("q-cancelled").exists());
    }

    #[test]
    fn move_name_validation_and_collision_can_be_corrected_in_preview() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("source"), b"source").unwrap();
        fs::write(fixture.0.join("taken"), b"collision").unwrap();
        let mut app = fixture.app();
        open_move_preview(&mut app);
        press(&mut app, KeyCode::Char('r'));
        for name in ["", ".", "..", "../escape", "/absolute", "nul\0name"] {
            enter_rename_name(&mut app, name);
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.screen(), Screen::MoveNameEditor);
            assert!(app.move_error().is_some());
        }
        enter_rename_name(&mut app, "taken");
        press(&mut app, KeyCode::Enter);
        assert!(!app.proposed_move().unwrap().is_valid());
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::MovePreview);
        assert_eq!(fs::read(fixture.0.join("taken")).unwrap(), b"collision");
        edit_move_name(&mut app, "available");
        assert!(app.proposed_move().unwrap().is_valid());
        press(&mut app, KeyCode::Enter);
        assert_eq!(fs::read(fixture.0.join("available")).unwrap(), b"source");
    }

    #[test]
    fn failed_edited_move_retains_name_for_retry_and_browser_return() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("source"), b"source").unwrap();
        let mut app = fixture.app();
        open_move_preview(&mut app);
        edit_move_name(&mut app, "renamed");
        fs::write(fixture.0.join("renamed"), b"collision").unwrap();
        press(&mut app, KeyCode::Enter);
        assert!(app.move_error().unwrap().contains("fresh validation"));
        assert_eq!(
            app.proposed_move().unwrap().resulting_path(),
            fixture.0.join("renamed")
        );
        assert_eq!(fs::read(fixture.path("source")).unwrap(), b"source");

        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen(), Screen::DestinationBrowser);
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(
            app.proposed_move().unwrap().resulting_path(),
            fixture.0.join("renamed")
        );
        assert!(!app.proposed_move().unwrap().is_valid());
        fs::remove_file(fixture.0.join("renamed")).unwrap();
        // Re-accept the reviewed name to refresh an invalid proposal.
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.rename_name().unwrap(), "renamed");
        press(&mut app, KeyCode::Enter);
        fs::write(fixture.0.join("renamed"), b"second collision").unwrap();
        press(&mut app, KeyCode::Enter);
        assert!(app.move_error().is_some());
        fs::remove_file(fixture.0.join("renamed")).unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
        assert_eq!(fs::read(fixture.0.join("renamed")).unwrap(), b"source");
    }

    #[test]
    fn move_name_edit_keeps_source_identity_and_destination_restrictions() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("source"), b"source").unwrap();
        let mut app = fixture.app();
        open_move_preview(&mut app);
        edit_move_name(&mut app, "renamed");
        fs::rename(fixture.path("source"), fixture.path("original")).unwrap();
        fs::write(fixture.path("source"), b"replacement").unwrap();
        press(&mut app, KeyCode::Enter);
        assert!(
            app.move_error()
                .unwrap()
                .contains("identity or entry type changed")
        );
        assert!(!fixture.0.join("renamed").exists());
        assert_eq!(fs::read(fixture.path("source")).unwrap(), b"replacement");

        let directory = RenameFixture::new();
        fs::create_dir(directory.path("folder")).unwrap();
        let mut app = directory.app();
        press(&mut app, KeyCode::Enter); // HOME
        press(&mut app, KeyCode::Enter); // Downloads
        press(&mut app, KeyCode::Down); // folder, after ..
        press(&mut app, KeyCode::Enter); // folder
        press(&mut app, KeyCode::Char('d'));
        edit_move_name(&mut app, "renamed");
        assert!(
            app.proposed_move()
                .unwrap()
                .failures()
                .contains(&crate::proposed_move::ValidationFailure::DirectoryInsideItself)
        );
        press(&mut app, KeyCode::Enter);
        assert!(directory.path("folder").is_dir());
        assert!(!directory.path("folder/renamed").exists());
    }

    #[test]
    fn edited_name_does_not_leak_to_next_inbox_entry() {
        let fixture = RenameFixture::new();
        fs::write(fixture.path("alpha"), b"first").unwrap();
        fs::write(fixture.path("beta"), b"second").unwrap();
        let mut app = fixture.app();
        open_move_preview(&mut app);
        edit_move_name(&mut app, "renamed");
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Down);
        open_move_preview(&mut app);
        assert_eq!(
            app.proposed_move().unwrap().resulting_path(),
            fixture.0.join("beta")
        );
        edit_move_name(&mut app, "second");
        press(&mut app, KeyCode::Enter);
        open_move_preview(&mut app);
        assert_eq!(
            app.proposed_move().unwrap().resulting_path(),
            fixture.0.join("alpha")
        );
    }
    fn selection_fixture() -> RenameFixture {
        let fixture = RenameFixture::new();
        for name in ["a", "b", "c", "d", "e"] {
            fs::write(fixture.path(name), name).unwrap();
        }
        fixture
    }

    fn marked_names(app: &App) -> Vec<String> {
        app.entries()
            .iter()
            .filter(|entry| app.marked(entry))
            .map(|entry| entry.display_name())
            .collect()
    }

    #[test]
    fn space_and_visual_ranges_preserve_baseline_while_shrinking_and_jumping() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char(' ')); // a baseline
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down); // c anchor
        press(&mut app, KeyCode::Char('V'));
        press(&mut app, KeyCode::Char('G'));
        assert_eq!(marked_names(&app), ["a", "c", "d", "e"]);
        press(&mut app, KeyCode::Up);
        assert_eq!(marked_names(&app), ["a", "c", "d"]);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        assert_eq!(marked_names(&app), ["a", "b", "c"]);
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.selected(), Some(0));
        assert_eq!(marked_names(&app), ["a", "b", "c"]);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.selected(), Some(0));
        press(&mut app, KeyCode::Char('V'));
        assert!(!app.visual_selection());
        press(&mut app, KeyCode::Char('G'));
        assert_eq!(marked_names(&app), ["a", "b", "c"]);
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(marked_names(&app), ["a", "b", "c", "e"]);
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(marked_names(&app), ["a", "b", "c"]);
    }

    #[test]
    fn all_clear_escape_and_empty_selection_controls_are_safe() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('a'));
        assert_eq!(app.marked_count(), 5);
        press(&mut app, KeyCode::Char('a'));
        assert_eq!(app.marked_count(), 0);
        press(&mut app, KeyCode::Char('V'));
        press(&mut app, KeyCode::Char('G'));
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.marked_count(), 0);
        assert!(!app.visual_selection());
        press(&mut app, KeyCode::Char('V'));
        press(&mut app, KeyCode::Char('c'));
        assert_eq!(app.marked_count(), 0);
        assert!(!app.visual_selection());
        let empty = RenameFixture::new();
        let mut app = empty.app();
        for code in [
            KeyCode::Char(' '),
            KeyCode::Char('V'),
            KeyCode::Char('G'),
            KeyCode::Char('g'),
            KeyCode::Char('g'),
            KeyCode::Char('a'),
            KeyCode::Char('c'),
            KeyCode::Esc,
            KeyCode::Char('R'),
        ] {
            press(&mut app, code);
        }
        assert_eq!(app.selected(), None);
        assert_eq!(app.marked_count(), 0);
        assert!(!app.visual_selection());
    }

    #[test]
    fn refresh_retains_only_matching_path_and_identity_and_repairs_cursor() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Down); // b cursor
        press(&mut app, KeyCode::Char('V'));
        fs::rename(fixture.path("a"), fixture.0.join("old-a")).unwrap();
        fs::write(fixture.path("a"), b"replacement").unwrap();
        fs::write(fixture.path("b"), b"same inode changed contents").unwrap();
        fs::remove_file(fixture.path("c")).unwrap();
        fs::rename(fixture.path("d"), fixture.path("z")).unwrap();
        fs::write(fixture.path("0"), b"new").unwrap();
        // Navigation does not rescan or reorder the list.
        press(&mut app, KeyCode::Down);
        assert_eq!(app.entries().len(), 5);
        assert_eq!(app.entries()[2].display_name(), "c");
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Char('R'));
        assert!(!app.visual_selection());
        assert_eq!(marked_names(&app), ["b", "e"]);
        assert_eq!(app.entries()[app.selected().unwrap()].display_name(), "b");
        assert_eq!(app.selected(), Some(2));
    }

    #[test]
    fn refresh_failure_retains_list_marks_and_reports_stale_state() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('V'));
        press(&mut app, KeyCode::Down);
        app.inbox_scanner = fail_refresh;
        press(&mut app, KeyCode::Char('R'));
        assert_eq!(app.entries().len(), 5);
        assert_eq!(app.selected(), Some(1));
        assert_eq!(marked_names(&app), ["a", "b"]);
        assert!(!app.visual_selection());
        assert!(app.notice().unwrap().contains("entries may be stale"));
        press(&mut app, KeyCode::Down);
        assert_eq!(app.selected(), Some(2));
        app.inbox_scanner = crate::inbox::scan_inbox;
        press(&mut app, KeyCode::Char('R'));
        assert_eq!(app.notice(), Some("Inbox refreshed"));
    }

    #[test]
    fn individual_actions_refuse_multiple_marks_and_target_single_mark() {
        for action in [KeyCode::Enter, KeyCode::Char('r')] {
            let fixture = selection_fixture();
            let mut app = fixture.app();
            press(&mut app, KeyCode::Char('a'));
            press(&mut app, action);
            if action == KeyCode::Enter {
                assert_eq!(app.screen(), Screen::DestinationBrowser);
                press(&mut app, KeyCode::Esc);
            } else {
                assert_eq!(app.screen(), Screen::Inbox);
                assert!(app.notice().unwrap().contains("supports one entry"));
            }
            press(&mut app, KeyCode::Char('c'));
            press(&mut app, KeyCode::Char(' ')); // a
            press(&mut app, KeyCode::Char('G')); // cursor e
            press(&mut app, action);
            assert_eq!(app.selected(), Some(0));
            if action == KeyCode::Enter {
                press(&mut app, KeyCode::Char('d'));
                assert_eq!(app.proposed_move().unwrap().source(), fixture.path("a"));
                press(&mut app, KeyCode::Enter);
                assert_eq!(fs::read(fixture.0.join("a")).unwrap(), b"a");
            } else {
                assert_eq!(app.rename_name().unwrap(), "a");
                enter_rename_name(&mut app, "renamed");
                press(&mut app, KeyCode::Enter);
                press(&mut app, KeyCode::Enter);
                assert_eq!(fs::read(fixture.path("renamed")).unwrap(), b"a");
            }
            assert_eq!(app.marked_count(), 0);
            assert!(fixture.path("e").exists());
        }
    }

    #[test]
    fn replaced_marked_entry_is_refused_before_refresh_and_unmarked_after() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char(' '));
        fs::rename(fixture.path("a"), fixture.0.join("old-a")).unwrap();
        fs::write(fixture.path("a"), b"replacement").unwrap();
        for action in [KeyCode::Enter, KeyCode::Char('r')] {
            press(&mut app, action);
            assert_eq!(app.screen(), Screen::Inbox);
            assert!(app.notice().unwrap().contains("marked entry changed"));
        }
        press(&mut app, KeyCode::Char('R'));
        assert_eq!(app.marked_count(), 0);
        assert_eq!(fs::read(fixture.path("a")).unwrap(), b"replacement");
    }

    #[test]
    fn refresh_marks_symlink_identity_not_target_identity() {
        let fixture = RenameFixture::new();
        let target = fixture.0.join("target");
        fs::write(&target, b"target").unwrap();
        std::os::unix::fs::symlink(&target, fixture.path("link")).unwrap();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char(' '));
        fs::rename(&target, fixture.0.join("old-target")).unwrap();
        fs::write(&target, b"new target").unwrap();
        press(&mut app, KeyCode::Char('R'));
        assert_eq!(app.marked_count(), 1);
        fs::rename(fixture.path("link"), fixture.0.join("old-link")).unwrap();
        std::os::unix::fs::symlink(&target, fixture.path("link")).unwrap();
        press(&mut app, KeyCode::Char('R'));
        assert_eq!(app.marked_count(), 0);
    }
    #[test]
    fn marked_move_keeps_identity_after_failure_and_return_to_browser() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char(' '));
        open_move_preview(&mut app);
        fs::write(fixture.0.join("a"), b"collision").unwrap();
        press(&mut app, KeyCode::Enter);
        assert!(app.move_error().is_some());
        fs::rename(fixture.path("a"), fixture.0.join("original")).unwrap();
        fs::write(fixture.path("a"), b"replacement").unwrap();
        fs::remove_file(fixture.0.join("a")).unwrap();
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::MovePreview);
        assert!(
            app.move_error()
                .unwrap()
                .contains("identity or entry type changed")
        );
        assert_eq!(fs::read(fixture.path("a")).unwrap(), b"replacement");
    }
    fn ignored_state_path(fixture: &RenameFixture) -> std::path::PathBuf {
        fixture.0.join(".local/state/downloads-janitor/ignored-v1")
    }

    #[test]
    fn bulk_ignore_persists_and_restore_view_only_allows_restoration() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('V'));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('i'));
        assert_eq!(app.entries().len(), 3);
        assert_eq!(app.marked_count(), 0);
        assert!(!app.visual_selection());
        assert_eq!(app.selected(), Some(1));
        assert!(app.notice().unwrap().contains("Ignored 2 entries"));
        assert_eq!(fs::read(fixture.path("a")).unwrap(), b"a");
        assert_eq!(fixture.app().entries().len(), 3);

        let mut restarted = fixture.app();
        press(&mut restarted, KeyCode::Char('I'));
        assert!(restarted.viewing_ignored());
        assert_eq!(restarted.entries().len(), 2);
        for key in [
            KeyCode::Enter,
            KeyCode::Char('r'),
            KeyCode::Char('i'),
            KeyCode::Char('d'),
        ] {
            press(&mut restarted, key);
            assert_eq!(restarted.screen(), Screen::Inbox);
            assert!(restarted.viewing_ignored());
        }
        press(&mut restarted, KeyCode::Char('a'));
        press(&mut restarted, KeyCode::Char('u'));
        assert!(restarted.entries().is_empty());
        assert_eq!(restarted.selected(), None);
        assert_eq!(restarted.marked_count(), 0);
        press(&mut restarted, KeyCode::Char('I'));
        assert_eq!(restarted.entries().len(), 5);
        assert_eq!(fixture.app().entries().len(), 5);
        assert_eq!(fs::read(fixture.path("b")).unwrap(), b"b");
    }

    #[test]
    fn ignored_identity_survives_content_changes_but_not_replacements() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('i')); // cursor a, no marks
        fs::write(fixture.path("a"), b"changed contents").unwrap();
        assert!(
            !fixture
                .app()
                .entries()
                .iter()
                .any(|entry| entry.path() == fixture.path("a"))
        );
        fs::rename(fixture.path("a"), fixture.0.join("old-a")).unwrap();
        fs::write(fixture.path("a"), b"replacement").unwrap();
        assert_eq!(fixture.app().entries().len(), 5);
        press(&mut app, KeyCode::Char('R'));
        assert_eq!(app.entries().len(), 5);
    }

    #[test]
    fn ignore_preflight_rejects_entire_batch_if_one_source_is_missing_or_replaced() {
        for missing in [false, true] {
            let fixture = selection_fixture();
            let mut app = fixture.app();
            press(&mut app, KeyCode::Char('a'));
            fs::rename(fixture.path("c"), fixture.0.join("original-c")).unwrap();
            if !missing {
                fs::write(fixture.path("c"), b"replacement").unwrap();
            }
            press(&mut app, KeyCode::Char('i'));
            assert_eq!(app.entries().len(), 5);
            assert_eq!(app.marked_count(), 5);
            assert!(app.notice().unwrap().contains("no entries changed"));
            assert!(!ignored_state_path(&fixture).exists());
        }
    }

    #[test]
    fn ignore_save_failure_keeps_visibility_marks_and_previous_state() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('i')); // persist a
        let path = ignored_state_path(&fixture);
        let previous = fs::read(&path).unwrap();
        let lock = path.parent().unwrap().join("ignored.lock");
        fs::remove_file(&lock).unwrap();
        fs::create_dir(&lock).unwrap(); // deterministic write failure even as root
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('i'));
        assert_eq!(app.entries().len(), 4);
        assert_eq!(app.marked_count(), 4);
        assert!(app.notice().unwrap().contains("no entries changed"));
        assert_eq!(fs::read(path).unwrap(), previous);
        assert_eq!(fixture.app().entries().len(), 4);
        press(&mut app, KeyCode::Char('I'));
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.entries().len(), 1);
        assert!(app.notice().unwrap().contains("no entries changed"));
    }

    #[test]
    fn unreadable_state_is_preserved_and_can_be_reloaded_after_repair() {
        let fixture = selection_fixture();
        let path = ignored_state_path(&fixture);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"corrupt state").unwrap();
        let mut app = fixture.app();
        assert_eq!(app.entries().len(), 5);
        assert!(
            app.ignored_warning()
                .unwrap()
                .contains("showing all entries")
        );
        press(&mut app, KeyCode::Char('i'));
        assert_eq!(app.entries().len(), 5);
        assert_eq!(fs::read(&path).unwrap(), b"corrupt state");
        press(&mut app, KeyCode::Down);
        assert!(app.ignored_warning().is_some());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(fixture.app().ignored_warning().is_some());
        fs::remove_dir(&path).unwrap();
        press(&mut app, KeyCode::Char('R'));
        assert!(app.ignored_warning().is_none());
        press(&mut app, KeyCode::Char('i'));
        assert_eq!(app.entries().len(), 4);
    }

    #[test]
    fn ignore_uses_marks_over_cursor_and_detects_other_sessions() {
        let fixture = selection_fixture();
        let mut first = fixture.app();
        let mut second = fixture.app();
        press(&mut first, KeyCode::Char(' ')); // a
        press(&mut first, KeyCode::Char('G')); // cursor e
        press(&mut first, KeyCode::Char('i'));
        assert!(
            !first
                .entries()
                .iter()
                .any(|entry| entry.path() == fixture.path("a"))
        );
        press(&mut second, KeyCode::Down);
        press(&mut second, KeyCode::Char('i'));
        assert_eq!(second.entries().len(), 5);
        assert!(second.notice().unwrap().contains("another session"));
        press(&mut second, KeyCode::Char('R'));
        press(&mut second, KeyCode::Char('i'));
        assert_eq!(fixture.app().entries().len(), 3);
    }

    #[test]
    fn raw_symlink_ignore_persists_link_identity_and_missing_entries_are_safe() {
        use std::os::unix::ffi::OsStrExt;
        let fixture = RenameFixture::new();
        let name = std::ffi::OsStr::from_bytes(b"link-\xff");
        let target = fixture.0.join("target");
        fs::write(&target, b"target").unwrap();
        std::os::unix::fs::symlink(&target, fixture.path(name)).unwrap();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('i'));
        fs::rename(&target, fixture.0.join("old-target")).unwrap();
        fs::write(&target, b"replacement target").unwrap();
        assert!(fixture.app().entries().is_empty());
        press(&mut app, KeyCode::Char('I'));
        assert_eq!(app.entries()[0].path(), fixture.path(name));
        fs::remove_file(fixture.path(name)).unwrap();
        press(&mut app, KeyCode::Char('u'));
        assert!(app.notice().unwrap().contains("no entries changed"));
        press(&mut app, KeyCode::Char('R'));
        assert!(app.entries().is_empty());
        assert_eq!(app.selected(), None);
    }

    #[test]
    fn completed_move_refresh_does_not_reveal_ignored_entries() {
        let fixture = selection_fixture();
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('i'));
        open_move_preview(&mut app);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.entries().len(), 3);
        assert!(
            !app.entries()
                .iter()
                .any(|entry| entry.path() == fixture.path("a"))
        );
        press(&mut app, KeyCode::Char('I'));
        assert_eq!(app.entries().len(), 1);
    }
    fn bulk_fixture() -> (RenameFixture, App) {
        let fixture = RenameFixture::new();
        for name in ["c", "a", "b"] {
            fs::write(fixture.path(name), name).unwrap();
        }
        let mut app = fixture.app();
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('G'));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.screen(), Screen::BulkPreview);
        (fixture, app)
    }

    #[test]
    fn bulk_preflight_checks_all_entries_before_any_mutation() {
        let (fixture, mut app) = bulk_fixture();
        fs::write(fixture.0.join("c"), b"occupied").unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::BulkPreview);
        assert!(!app.batch().unwrap().valid());
        assert!(
            app.batch().unwrap().entries[2]
                .problems
                .iter()
                .any(|p| p.contains("already exists"))
        );
        for name in ["a", "b", "c"] {
            assert!(fixture.path(name).exists());
        }
        assert!(!fixture.0.join("a").exists());
    }

    #[test]
    fn bulk_preflight_refuses_replaced_marked_source() {
        let (fixture, mut app) = bulk_fixture();
        fs::rename(fixture.path("c"), fixture.0.join("original")).unwrap();
        fs::write(fixture.path("c"), b"replacement").unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::BulkPreview);
        assert!(
            app.batch().unwrap().entries[2]
                .problems
                .iter()
                .any(|p| p.contains("identity"))
        );
        assert!(fixture.path("a").exists());
    }

    #[test]
    fn bulk_success_is_ordered_consumes_marks_and_cannot_replay() {
        let (fixture, mut app) = bulk_fixture();
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.screen(), Screen::BulkPreview); // no bulk basename editor
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::BulkProgress);
        assert!(fixture.path("a").exists()); // authorization does not skip progress rendering
        for name in ["a", "b", "c"] {
            app.advance_batch();
            assert_eq!(fs::read(fixture.0.join(name)).unwrap(), name.as_bytes());
            assert!(!fixture.path(name).exists());
        }
        assert_eq!(app.screen(), Screen::BulkResult);
        assert_eq!(
            app.batch().unwrap().summary(),
            "3 completed, 0 failed, 0 unattempted"
        );
        assert_eq!(app.marked_count(), 0);
        assert_eq!(app.selected(), None);
        app.batch.as_mut().unwrap().authorize();
        assert!(!app.batch().unwrap().running);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen(), Screen::Inbox);
    }

    #[test]
    fn bulk_collision_after_first_move_stops_and_retains_remaining_marks() {
        let (fixture, mut app) = bulk_fixture();
        press(&mut app, KeyCode::Enter);
        app.advance_batch();
        fs::write(fixture.0.join("b"), b"racer").unwrap();
        app.advance_batch();
        assert_eq!(app.screen(), Screen::BulkResult);
        assert_eq!(
            app.batch().unwrap().summary(),
            "1 completed, 1 failed, 1 unattempted"
        );
        assert_eq!(marked_names(&app), ["b", "c"]);
        assert_eq!(app.selected(), Some(1));
        assert_eq!(fs::read(fixture.0.join("b")).unwrap(), b"racer");
        assert_eq!(fs::read(fixture.path("b")).unwrap(), b"b");
        assert!(fixture.path("c").exists());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.batch().unwrap().entries.len(), 2);
        assert!(
            app.batch()
                .unwrap()
                .entries
                .iter()
                .all(|item| item.entry.path() != fixture.path("a"))
        );
    }

    #[test]
    fn bulk_checks_identity_again_between_entries() {
        let (fixture, mut app) = bulk_fixture();
        press(&mut app, KeyCode::Enter);
        app.advance_batch();
        fs::rename(fixture.path("b"), fixture.0.join("original")).unwrap();
        fs::write(fixture.path("b"), b"replacement").unwrap();
        app.advance_batch();
        assert_eq!(
            app.batch().unwrap().summary(),
            "1 completed, 1 failed, 1 unattempted"
        );
        assert_eq!(marked_names(&app), ["c"]);
        assert_eq!(fs::read(fixture.path("b")).unwrap(), b"replacement");
    }

    #[test]
    fn bulk_escape_stops_before_next_entry_and_keeps_completed_moves() {
        for completed in [0, 1] {
            let (fixture, mut app) = bulk_fixture();
            press(&mut app, KeyCode::Enter);
            for _ in 0..completed {
                app.advance_batch();
            }
            press(&mut app, KeyCode::Esc);
            assert_eq!(app.screen(), Screen::BulkResult);
            assert!(app.batch().unwrap().stopped);
            assert_eq!(app.marked_count(), 3 - completed);
            assert_eq!(fixture.0.join("a").exists(), completed == 1);
            assert!(fixture.path("b").exists());
            app.batch.as_mut().unwrap().step();
            assert!(fixture.path("b").exists());
        }
    }

    #[test]
    fn bulk_refresh_failure_keeps_truthful_results_and_unconsumed_marks() {
        let (fixture, mut app) = bulk_fixture();
        app.inbox_scanner = |_| Err(std::io::Error::other("scan denied").into());
        press(&mut app, KeyCode::Enter);
        app.advance_batch();
        press(&mut app, KeyCode::Esc);
        assert!(
            app.notice()
                .unwrap()
                .contains("1 completed, 0 failed, 2 unattempted")
        );
        assert!(app.notice().unwrap().contains("scan denied"));
        assert!(app.notice().unwrap().contains("may be stale"));
        assert_eq!(marked_names(&app), ["b", "c"]);
        assert_eq!(app.entries().len(), 2);
        assert_eq!(app.selected(), Some(1));
        assert!(fixture.0.join("a").exists());
    }

    #[test]
    fn bulk_refuses_destination_ancestor_replaced_by_symlink() {
        let (fixture, mut app) = bulk_fixture();
        let parent = fixture.0.join("parent");
        fs::create_dir_all(parent.join("dest")).unwrap();
        app.batch = Some(crate::batch::Batch::new(
            app.bulk_targets.clone(),
            &parent.join("dest"),
            &fixture.0,
        ));
        press(&mut app, KeyCode::Enter);
        fs::rename(&parent, fixture.0.join("old-parent")).unwrap();
        std::os::unix::fs::symlink(fixture.0.join("old-parent"), &parent).unwrap();
        app.advance_batch();
        assert_eq!(
            app.batch().unwrap().summary(),
            "0 completed, 1 failed, 2 unattempted"
        );
        assert!(fixture.path("a").exists());
    }
    #[test]
    fn bulk_moves_directories_and_symlink_entries_without_touching_targets() {
        let fixture = RenameFixture::new();
        fs::create_dir(fixture.path("z-dir")).unwrap();
        fs::write(fixture.path("z-dir/contents"), b"nested").unwrap();
        fs::write(fixture.path("a-file"), b"plain").unwrap();
        let target = fixture.0.join("target");
        fs::write(&target, b"untouched").unwrap();
        std::os::unix::fs::symlink(&target, fixture.path("m-link")).unwrap();
        let mut app = fixture.app();
        for code in [
            KeyCode::Char('a'),
            KeyCode::Enter,
            KeyCode::Char('d'),
            KeyCode::Enter,
        ] {
            press(&mut app, code);
        }
        app.advance_batch(); // byte order, even though Inbox sorts directories first
        assert!(fixture.0.join("a-file").exists());
        assert!(fixture.path("z-dir").exists());
        app.advance_batch();
        assert_eq!(fs::read_link(fixture.0.join("m-link")).unwrap(), target);
        app.advance_batch();
        assert_eq!(
            fs::read(fixture.0.join("z-dir/contents")).unwrap(),
            b"nested"
        );
        assert_eq!(fs::read(target).unwrap(), b"untouched");
        assert_eq!(
            app.batch().unwrap().summary(),
            "3 completed, 0 failed, 0 unattempted"
        );
    }
}
