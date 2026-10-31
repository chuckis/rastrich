use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use nostr_sdk::prelude::*;
use ratatui::widgets::ListState;
use ratatui_textarea::{Input, TextArea};
use tokio::sync::mpsc;

use crate::config::AccountMode;
use crate::model::feed;
use crate::nostr::commands::{AccountInfo, Command, RelayInfo};
use crate::nostr::updates::Update;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    Main,
}

pub struct LoginState {
    pub mode: AccountMode,
    pub input: String,
    pub busy: bool,
    pub error: Option<String>,
}

impl LoginState {
    fn new() -> Self {
        Self {
            mode: AccountMode::ReadOnly,
            input: String::new(),
            busy: false,
            error: None,
        }
    }
}

pub struct ThreadView {
    pub root: Event,
    pub focus: Event,
    pub replies: Vec<Event>,
    pub state: ListState,
}

impl ThreadView {
    fn new(root: Event, focus: Event, replies: Vec<Event>) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            root,
            focus,
            replies,
            state,
        }
    }

    pub fn events(&self) -> Vec<&Event> {
        let mut events = Vec::with_capacity(self.replies.len() + 2);
        if self.root.id != self.focus.id {
            events.push(&self.root);
        }
        events.push(&self.focus);
        events.extend(self.replies.iter());
        events
    }

    pub fn selected_event(&self) -> Option<&Event> {
        self.events()
            .get(self.state.selected().unwrap_or(0))
            .copied()
    }

    fn select(&mut self, index: usize) {
        let len = self.events().len();
        if len == 0 {
            self.state.select(None);
        } else {
            self.state.select(Some(index.min(len - 1)));
        }
    }
}

pub struct ProfileView {
    pub public_key: PublicKey,
    pub metadata: Option<Metadata>,
    pub notes: Vec<Event>,
    pub state: ListState,
}

impl ProfileView {
    fn new(public_key: PublicKey) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            public_key,
            metadata: None,
            notes: Vec::new(),
            state,
        }
    }

    pub fn selected_note(&self) -> Option<&Event> {
        self.notes.get(self.state.selected().unwrap_or(0))
    }

    fn select(&mut self, index: usize) {
        if self.notes.is_empty() {
            self.state.select(None);
        } else {
            self.state.select(Some(index.min(self.notes.len() - 1)));
        }
    }
}

pub struct Composer {
    pub textarea: TextArea<'static>,
    pub reply_to: Option<Event>,
}

impl Composer {
    fn new(reply_to: Option<Event>) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text("Write a note...");
        Self { textarea, reply_to }
    }

    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.text().trim().is_empty()
    }
}

pub struct RelayManager {
    pub selected: usize,
    pub input: String,
    pub adding: bool,
}

impl RelayManager {
    fn new() -> Self {
        Self {
            selected: 0,
            input: String::new(),
            adding: false,
        }
    }
}

pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    pub login: LoginState,
    pub account: Option<AccountInfo>,
    pub events: Vec<Event>,
    pub profiles: HashMap<PublicKey, Metadata>,
    pub timeline_state: ListState,
    pub thread: Option<ThreadView>,
    pub profile: Option<ProfileView>,
    pub composer: Option<Composer>,
    pub relay_manager: Option<RelayManager>,
    pub relays: Vec<RelayInfo>,
    pub status: Option<(String, Instant)>,
    pub show_help: bool,
    commands: mpsc::Sender<Command>,
}

impl App {
    pub fn new(commands: mpsc::Sender<Command>) -> Self {
        let mut timeline_state = ListState::default();
        timeline_state.select(Some(0));
        Self {
            screen: Screen::Login,
            should_quit: false,
            login: LoginState::new(),
            account: None,
            events: Vec::new(),
            profiles: HashMap::new(),
            timeline_state,
            thread: None,
            profile: None,
            composer: None,
            relay_manager: None,
            relays: Vec::new(),
            status: None,
            show_help: false,
            commands,
        }
    }

    pub fn set_restoring(&mut self, npub: String, mode: AccountMode) {
        self.screen = Screen::Login;
        self.login.mode = mode;
        self.login.input = npub;
        self.login.busy = true;
        self.login.error = Some("Restoring session...".to_string());
    }

    pub fn handle_terminal_event(&mut self, event: TerminalEvent) {
        match event {
            TerminalEvent::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            TerminalEvent::Paste(text) => self.handle_paste(text),
            _ => {}
        }
    }

    fn handle_paste(&mut self, text: String) {
        if self.screen == Screen::Login {
            self.login.input.push_str(&text);
        } else if let Some(composer) = self.composer.as_mut() {
            composer.textarea.insert_str(text);
        } else if let Some(relay) = self.relay_manager.as_mut()
            && relay.adding
        {
            relay.input.push_str(&text);
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        if self.show_help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                self.show_help = false;
            }
            return;
        }

        if self.screen == Screen::Login {
            self.handle_login_key(key);
            return;
        }

        if self.relay_manager.is_some() {
            self.handle_relay_key(key);
            return;
        }
        if self.profile.is_some() {
            self.handle_profile_key(key);
            return;
        }
        if self.composer.is_some() {
            self.handle_composer_key(key);
            return;
        }
        if self.thread.is_some() {
            self.handle_thread_key(key);
            return;
        }

        self.handle_timeline_key(key);
    }

    fn handle_login_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                if !self.login.busy {
                    self.login.mode = self.login.mode.next();
                    self.login.error = None;
                }
            }
            KeyCode::BackTab => {
                if !self.login.busy {
                    self.login.mode = match self.login.mode {
                        AccountMode::ReadOnly => AccountMode::Bunker,
                        AccountMode::Local => AccountMode::ReadOnly,
                        AccountMode::Bunker => AccountMode::Local,
                    };
                    self.login.error = None;
                }
            }
            KeyCode::Enter => {
                if !self.login.busy {
                    if self.login.input.trim().is_empty() {
                        self.login.error = Some("Enter a key or URI first.".to_string());
                        return;
                    }
                    self.login.busy = true;
                    self.login.error = None;
                    self.send(Command::Login {
                        mode: self.login.mode,
                        input: self.login.input.clone(),
                    });
                }
            }
            KeyCode::Backspace => {
                if !self.login.busy {
                    self.login.input.pop();
                }
            }
            KeyCode::Char(c) => {
                if !self.login.busy && !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.login.input.push(c);
                }
            }
            KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_timeline_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_timeline(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_timeline(-1),
            KeyCode::Char('g') | KeyCode::Home => self.select_timeline(0),
            KeyCode::Char('G') | KeyCode::End => {
                if !self.events.is_empty() {
                    self.select_timeline(self.events.len() - 1);
                }
            }
            KeyCode::Enter => {
                if let Some(event) = self.selected_event() {
                    self.send(Command::OpenThread(event.id));
                }
            }
            KeyCode::Char('c') => self.open_composer(None),
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.send(Command::LoadTimeline);
            }
            KeyCode::Char('r') => {
                if let Some(event) = self.selected_event().cloned() {
                    self.open_composer(Some(event));
                }
            }
            KeyCode::Char('l') | KeyCode::Char('+') => self.react_to_selected(),
            KeyCode::Char('p') => {
                if let Some(event) = self.selected_event() {
                    self.open_profile(event.pubkey);
                }
            }
            KeyCode::Char('R') => self.relay_manager = Some(RelayManager::new()),
            _ => {}
        }
    }

    fn handle_thread_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Esc => self.thread = None,
            KeyCode::Char('j') | KeyCode::Down => self.move_thread(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_thread(-1),
            KeyCode::Char('g') | KeyCode::Home => self.select_thread(0),
            KeyCode::Char('G') | KeyCode::End => {
                if let Some(view) = self.thread.as_ref() {
                    let len = view.events().len();
                    if len > 0 {
                        self.select_thread(len - 1);
                    }
                }
            }
            KeyCode::Char('r') => {
                if let Some(event) = self
                    .thread
                    .as_ref()
                    .and_then(ThreadView::selected_event)
                    .cloned()
                {
                    self.open_composer(Some(event));
                }
            }
            KeyCode::Char('l') | KeyCode::Char('+') => {
                if let Some(event) = self.thread.as_ref().and_then(ThreadView::selected_event) {
                    self.send(Command::React {
                        event: event.id,
                        reaction: "+".to_string(),
                    });
                }
            }
            KeyCode::Char('p') => {
                if let Some(event) = self.thread.as_ref().and_then(ThreadView::selected_event) {
                    self.open_profile(event.pubkey);
                }
            }
            _ => {}
        }
    }

    fn handle_profile_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Esc => self.profile = None,
            KeyCode::Char('j') | KeyCode::Down => self.move_profile(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_profile(-1),
            KeyCode::Char('g') | KeyCode::Home => self.select_profile(0),
            KeyCode::Char('G') | KeyCode::End => {
                if let Some(profile) = self.profile.as_ref()
                    && !profile.notes.is_empty()
                {
                    self.select_profile(profile.notes.len() - 1);
                }
            }
            KeyCode::Enter => {
                if let Some(event) = self.profile.as_ref().and_then(ProfileView::selected_note) {
                    let id = event.id;
                    self.profile = None;
                    self.send(Command::OpenThread(id));
                }
            }
            KeyCode::Char('r') => {
                if let Some(event) = self
                    .profile
                    .as_ref()
                    .and_then(ProfileView::selected_note)
                    .cloned()
                {
                    self.profile = None;
                    self.open_composer(Some(event));
                }
            }
            KeyCode::Char('l') | KeyCode::Char('+') => {
                if let Some(event) = self.profile.as_ref().and_then(ProfileView::selected_note) {
                    self.send(Command::React {
                        event: event.id,
                        reaction: "+".to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    fn handle_composer_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.composer = None;
            return;
        }

        if key.code == KeyCode::Enter {
            let newline = key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT);
            if newline {
                if let Some(composer) = self.composer.as_mut() {
                    composer.textarea.insert_str("\n");
                }
            } else {
                self.publish_composer();
            }
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            self.publish_composer();
            return;
        }

        if let Some(composer) = self.composer.as_mut() {
            composer.textarea.input(Input::from(key));
        }
    }

    fn handle_relay_key(&mut self, key: KeyEvent) {
        let Some(manager) = self.relay_manager.as_mut() else {
            return;
        };

        if manager.adding {
            match key.code {
                KeyCode::Enter => {
                    let url = manager.input.trim().to_string();
                    if url.is_empty() {
                        manager.adding = false;
                    } else {
                        manager.adding = false;
                        manager.input.clear();
                        self.send(Command::AddRelay { url });
                    }
                }
                KeyCode::Esc => {
                    manager.adding = false;
                    manager.input.clear();
                }
                KeyCode::Backspace => {
                    manager.input.pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    manager.input.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Esc => self.relay_manager = None,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.relays.is_empty() {
                    manager.selected = (manager.selected + 1).min(self.relays.len() - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                manager.selected = manager.selected.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => manager.selected = 0,
            KeyCode::Char('G') | KeyCode::End => {
                manager.selected = self.relays.len().saturating_sub(1);
            }
            KeyCode::Char('a') => manager.adding = true,
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(relay) = self.relays.get(manager.selected) {
                    self.send(Command::RemoveRelay {
                        url: relay.url.clone(),
                    });
                }
            }
            KeyCode::Char('r') => self.send(Command::RefreshRelays),
            _ => {}
        }
    }

    fn open_profile(&mut self, public_key: PublicKey) {
        self.profile = Some(ProfileView::new(public_key));
        self.send(Command::OpenProfile(public_key));
    }

    fn open_composer(&mut self, reply_to: Option<Event>) {
        if self
            .account
            .as_ref()
            .is_some_and(|account| matches!(account.mode, AccountMode::ReadOnly))
        {
            self.set_status("This account is read-only.");
            return;
        }
        self.composer = Some(Composer::new(reply_to));
    }

    fn publish_composer(&mut self) {
        let Some(composer) = self.composer.as_ref() else {
            return;
        };
        if composer.is_empty() {
            self.set_status("Cannot publish an empty note.");
            return;
        }
        let content = composer.text();
        let reply_to = composer.reply_to.as_ref().map(|event| event.id);
        self.composer = None;
        self.send(Command::PublishNote { content, reply_to });
        self.set_status("Publishing...");
    }

    fn react_to_selected(&mut self) {
        if let Some(event) = self.selected_event() {
            self.send(Command::React {
                event: event.id,
                reaction: "+".to_string(),
            });
        }
    }

    pub fn apply_update(&mut self, update: Update) {
        match update {
            Update::Ready { relays } => self.relays = relays,
            Update::RelayStatus(relays) => {
                self.relays = relays;
                if let Some(manager) = self.relay_manager.as_mut() {
                    manager.selected = manager.selected.min(self.relays.len().saturating_sub(1));
                }
            }
            Update::LoginRequired {
                mode,
                npub,
                message,
            } => {
                self.screen = Screen::Login;
                self.login.mode = mode;
                self.login.input = npub.unwrap_or_default();
                self.login.busy = false;
                self.login.error = (!message.is_empty()).then_some(message);
            }
            Update::LoginSuccess { account, relays } => {
                self.screen = Screen::Main;
                self.account = Some(account);
                self.relays = relays;
                self.login.busy = false;
                self.login.input.clear();
                self.login.error = None;
                self.events.clear();
                self.profiles.clear();
                self.thread = None;
                self.profile = None;
                self.composer = None;
                self.relay_manager = None;
                self.timeline_state.select(Some(0));
                self.set_status("Connected.");
            }
            Update::LoginFailed(message) => {
                self.login.busy = false;
                self.login.error = Some(message);
            }
            Update::Timeline { events } => self.merge_timeline(events),
            Update::NewEvent(event) => {
                self.merge_timeline([event]);
            }
            Update::Profiles(profiles) => {
                self.profiles.extend(profiles);
            }
            Update::ThreadLoaded {
                root,
                focus,
                replies,
            } => {
                let selected_id = self
                    .thread
                    .as_ref()
                    .and_then(ThreadView::selected_event)
                    .map(|event| event.id);
                let mut view = ThreadView::new(root, focus, replies);
                if let Some(id) = selected_id
                    && let Some(index) = view.events().iter().position(|event| event.id == id)
                {
                    view.select(index);
                }
                self.thread = Some(view);
            }
            Update::ProfileLoaded {
                public_key,
                metadata,
                notes,
            } => match self.profile.as_mut() {
                Some(profile) if profile.public_key == public_key => {
                    let selected_id = profile.selected_note().map(|event| event.id);
                    if metadata.is_some() {
                        profile.metadata = metadata;
                    }
                    profile.notes = notes;
                    feed::sort_events(&mut profile.notes);
                    if let Some(id) = selected_id {
                        if let Some(index) = profile.notes.iter().position(|event| event.id == id) {
                            profile.select(index);
                        } else {
                            profile.select(0);
                        }
                    } else {
                        profile.select(0);
                    }
                }
                Some(_) => {}
                None => {
                    let mut profile = ProfileView::new(public_key);
                    profile.metadata = metadata;
                    profile.notes = notes;
                    feed::sort_events(&mut profile.notes);
                    profile.select(0);
                    self.profile = Some(profile);
                }
            },
            Update::Published {
                event_id,
                success,
                failed,
            } => {
                let short = &event_id[..event_id.len().min(10)];
                self.set_status(format!(
                    "Published {short} to {success} relay(s), {failed} failed."
                ));
            }
            Update::Status(message) => self.set_status(message),
            Update::Error(message) => self.set_status(format!("Error: {message}")),
            Update::Shutdown => self.should_quit = true,
        }
    }

    fn merge_timeline(&mut self, incoming: impl IntoIterator<Item = Event>) {
        let selected_id = self.selected_event().map(|event| event.id);
        feed::merge_events(&mut self.events, incoming);

        if let Some(id) = selected_id
            && let Some(index) = self.events.iter().position(|event| event.id == id)
        {
            self.timeline_state.select(Some(index));
            return;
        }
        if self.events.is_empty() {
            self.timeline_state.select(None);
        } else {
            self.timeline_state.select(Some(0));
        }
    }

    pub fn tick(&mut self) {
        if let Some((_, created)) = self.status.as_ref()
            && created.elapsed() > Duration::from_secs(6)
        {
            self.status = None;
        }
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
    }

    pub fn selected_event(&self) -> Option<&Event> {
        self.events.get(self.timeline_state.selected().unwrap_or(0))
    }

    fn select_timeline(&mut self, index: usize) {
        if self.events.is_empty() {
            self.timeline_state.select(None);
        } else {
            self.timeline_state
                .select(Some(index.min(self.events.len() - 1)));
        }
    }

    fn move_timeline(&mut self, delta: i32) {
        let current = self.timeline_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).max(0) as usize;
        self.select_timeline(next);
    }

    fn select_thread(&mut self, index: usize) {
        if let Some(view) = self.thread.as_mut() {
            view.select(index);
        }
    }

    fn move_thread(&mut self, delta: i32) {
        if let Some(view) = self.thread.as_mut() {
            let current = view.state.selected().unwrap_or(0) as i32;
            view.select((current + delta).max(0) as usize);
        }
    }

    fn select_profile(&mut self, index: usize) {
        if let Some(profile) = self.profile.as_mut() {
            profile.select(index);
        }
    }

    fn move_profile(&mut self, delta: i32) {
        if let Some(profile) = self.profile.as_mut() {
            let current = profile.state.selected().unwrap_or(0) as i32;
            profile.select((current + delta).max(0) as usize);
        }
    }

    fn send(&mut self, command: Command) {
        if let Err(err) = self.commands.try_send(command) {
            self.set_status(format!("Could not queue command: {err}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(mode: AccountMode) -> AccountInfo {
        let keys = Keys::generate();
        let public_key = keys.public_key();
        AccountInfo {
            public_key,
            npub: public_key.to_bech32().unwrap(),
            mode,
        }
    }

    fn note(content: &str, created_at: u64) -> Event {
        EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from_secs(created_at))
            .finalize(&Keys::generate())
            .unwrap()
    }

    fn app() -> App {
        let (commands, _rx) = mpsc::channel(8);
        App::new(commands)
    }

    fn press(app: &mut App, code: KeyCode) {
        press_with(app, code, KeyModifiers::NONE);
    }

    fn press_with(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_terminal_event(TerminalEvent::Key(KeyEvent::new(code, modifiers)));
    }

    #[test]
    fn login_success_enters_main_screen() {
        let mut app = app();
        app.apply_update(Update::LoginSuccess {
            account: account(AccountMode::ReadOnly),
            relays: Vec::new(),
        });

        assert_eq!(app.screen, Screen::Main);
        assert!(app.account.is_some());
    }

    #[test]
    fn timeline_updates_merge_and_sort() {
        let mut app = app();
        app.apply_update(Update::LoginSuccess {
            account: account(AccountMode::ReadOnly),
            relays: Vec::new(),
        });

        let older = note("older", 10);
        let newer = note("newer", 20);
        app.apply_update(Update::Timeline {
            events: vec![older.clone(), newer.clone()],
        });
        app.apply_update(Update::NewEvent(older));

        assert_eq!(app.events.len(), 2);
        assert_eq!(app.events[0].id, newer.id);
    }

    #[test]
    fn read_only_account_cannot_compose() {
        let mut app = app();
        app.apply_update(Update::LoginSuccess {
            account: account(AccountMode::ReadOnly),
            relays: Vec::new(),
        });

        press(&mut app, KeyCode::Char('c'));

        assert!(app.composer.is_none());
        assert!(
            app.status
                .as_ref()
                .is_some_and(|(message, _)| { message.contains("read-only") })
        );
    }

    #[test]
    fn local_account_can_compose() {
        let mut app = app();
        app.apply_update(Update::LoginSuccess {
            account: account(AccountMode::Local),
            relays: Vec::new(),
        });

        press(&mut app, KeyCode::Char('c'));

        assert!(app.composer.is_some());
    }

    #[test]
    fn enter_sends_composer_note() {
        let (commands, mut rx) = mpsc::channel(8);
        let mut app = App::new(commands);
        app.apply_update(Update::LoginSuccess {
            account: account(AccountMode::Local),
            relays: Vec::new(),
        });

        press(&mut app, KeyCode::Char('c'));
        app.composer
            .as_mut()
            .unwrap()
            .textarea
            .insert_str("hello nostr");
        press(&mut app, KeyCode::Enter);

        assert!(app.composer.is_none());
        match rx.try_recv().unwrap() {
            Command::PublishNote { content, reply_to } => {
                assert_eq!(content, "hello nostr");
                assert!(reply_to.is_none());
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn alt_enter_inserts_newline() {
        let mut app = app();
        app.apply_update(Update::LoginSuccess {
            account: account(AccountMode::Local),
            relays: Vec::new(),
        });

        press(&mut app, KeyCode::Char('c'));
        press_with(&mut app, KeyCode::Enter, KeyModifiers::ALT);

        assert!(app.composer.is_some());
        assert_eq!(app.composer.as_ref().unwrap().text(), "\n");
    }
}
