use std::collections::{HashMap, HashSet};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use nostr_connect::prelude::{NostrConnect, NostrConnectUri};
use nostr_sdk::prelude::*;
use nostr_sqlite::prelude::*;
use tokio::sync::mpsc;

use crate::config::{AccountConfig, AccountMode, Config, Paths};
use crate::model::{feed, thread};
use crate::nostr::commands::{AccountInfo, Command, RelayInfo};
use crate::nostr::signer::{Signer, parse_public_key};
use crate::nostr::updates::Update;
use crate::secrets;

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const LIVE_LIMIT: usize = 100;

pub async fn run(
    paths: Paths,
    config: Config,
    mut commands: mpsc::Receiver<Command>,
    updates: mpsc::Sender<Update>,
) -> Result<()> {
    let mut worker = Worker::new(paths, config, updates).await?;
    worker.spawn_tasks();
    worker
        .send(Update::Ready {
            relays: relay_snapshot(&worker.client).await,
        })
        .await;

    while let Some(command) = commands.recv().await {
        if matches!(command, Command::Shutdown) {
            break;
        }
        if let Err(err) = worker.handle(command).await {
            worker.send(Update::Error(format!("{err:#}"))).await;
        }
    }

    worker.client.shutdown().await;
    worker.send(Update::Shutdown).await;
    Ok(())
}

struct Worker {
    paths: Paths,
    config: Config,
    client: Client,
    signer: Option<Signer>,
    account: Option<AccountInfo>,
    followed: Vec<PublicKey>,
    updates: mpsc::Sender<Update>,
    requested_profiles: HashSet<PublicKey>,
}

impl Worker {
    async fn new(paths: Paths, config: Config, updates: mpsc::Sender<Update>) -> Result<Self> {
        let database = match NostrSqlite::builder()
            .in_file(&paths.database)
            .build()
            .await
        {
            Ok(database) => database,
            Err(err) => {
                tracing::warn!("failed to open sqlite database: {err}");
                NostrSqlite::builder()
                    .in_memory()
                    .build()
                    .await
                    .context("failed to open in-memory database")?
            }
        };

        let client = Client::builder().database(database).build();

        for relay in &config.relays {
            if !relay.read && !relay.write {
                continue;
            }
            if let Err(err) = client
                .add_relay(relay.url.as_str())
                .capabilities(relay_capabilities(relay.read, relay.write))
                .await
            {
                tracing::warn!("failed to add relay {}: {err}", relay.url);
            }
        }

        client.connect().and_wait(Duration::from_secs(5)).await;

        Ok(Self {
            paths,
            config,
            client,
            signer: None,
            account: None,
            followed: Vec::new(),
            updates,
            requested_profiles: HashSet::new(),
        })
    }

    fn spawn_tasks(&self) {
        let client = self.client.clone();
        let updates = self.updates.clone();
        tokio::spawn(async move {
            let mut notifications = client.notifications();
            let mut seen = HashSet::new();
            while let Some(notification) = notifications.next().await {
                match notification {
                    ClientNotification::Event { event, .. }
                        if event.kind == Kind::TextNote && seen.insert(event.id) =>
                    {
                        let event = *event;
                        let pubkey = event.pubkey;
                        updates.send(Update::NewEvent(event)).await.ok();
                        spawn_profile_fetch(client.clone(), updates.clone(), vec![pubkey]);
                    }
                    ClientNotification::Shutdown => break,
                    _ => {}
                }
            }
        });

        let client = self.client.clone();
        let updates = self.updates.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if updates
                    .send(Update::RelayStatus(relay_snapshot(&client).await))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    async fn send(&self, update: Update) {
        if self.updates.send(update).await.is_err() {
            tracing::warn!("UI update channel closed");
        }
    }

    async fn handle(&mut self, command: Command) -> Result<()> {
        match command {
            Command::AutoLogin(account) => self.auto_login(account).await,
            Command::Login { mode, input } => {
                if let Err(err) = self.login(mode, input).await {
                    self.send(Update::LoginFailed(format!("{err:#}"))).await;
                }
                Ok(())
            }
            Command::LoadTimeline => self.reload_timeline().await,
            Command::OpenThread(id) => self.open_thread(id).await,
            Command::OpenProfile(public_key) => self.open_profile(public_key).await,
            Command::PublishNote { content, reply_to } => {
                self.publish_note(content, reply_to).await
            }
            Command::React { event, reaction } => self.publish_reaction(event, reaction).await,
            Command::AddRelay { url } => self.add_relay(url).await,
            Command::RemoveRelay { url } => self.remove_relay(url).await,
            Command::RefreshRelays => {
                self.send(Update::RelayStatus(relay_snapshot(&self.client).await))
                    .await;
                Ok(())
            }
            Command::Shutdown => Ok(()),
        }
    }

    async fn auto_login(&mut self, account: Option<AccountConfig>) -> Result<()> {
        let Some(account) = account else {
            self.send(Update::LoginRequired {
                mode: AccountMode::ReadOnly,
                npub: None,
                message: String::new(),
            })
            .await;
            return Ok(());
        };

        match account.mode {
            AccountMode::ReadOnly => {
                let public_key = parse_public_key(&account.npub)?;
                self.finish_login(Signer::ReadOnly(public_key), AccountMode::ReadOnly)
                    .await
            }
            AccountMode::Local => match secrets::load_nsec(&account.npub) {
                Ok(Some(nsec)) => {
                    let keys = Keys::parse(&nsec).context("stored nsec is invalid")?;
                    self.finish_login(Signer::Local(keys), AccountMode::Local)
                        .await
                }
                Ok(None) => {
                    self.send(Update::LoginRequired {
                        mode: AccountMode::Local,
                        npub: Some(account.npub),
                        message: "No stored key found. Enter your nsec to unlock.".to_string(),
                    })
                    .await;
                    Ok(())
                }
                Err(err) => {
                    self.send(Update::LoginRequired {
                        mode: AccountMode::Local,
                        npub: Some(account.npub),
                        message: format!("Keyring unavailable: {err:#}"),
                    })
                    .await;
                    Ok(())
                }
            },
            AccountMode::Bunker => match secrets::load_bunker(&account.npub) {
                Ok(Some((uri, app_nsec))) => {
                    let uri =
                        NostrConnectUri::parse(&uri).context("stored bunker URI is invalid")?;
                    let app_keys =
                        Keys::parse(&app_nsec).context("stored bunker app key is invalid")?;
                    self.login_bunker(uri, app_keys, Some(account.npub)).await
                }
                Ok(None) => {
                    self.send(Update::LoginRequired {
                        mode: AccountMode::Bunker,
                        npub: Some(account.npub),
                        message: "No stored bunker session. Paste a bunker:// URI.".to_string(),
                    })
                    .await;
                    Ok(())
                }
                Err(err) => {
                    self.send(Update::LoginRequired {
                        mode: AccountMode::Bunker,
                        npub: Some(account.npub),
                        message: format!("Keyring unavailable: {err:#}"),
                    })
                    .await;
                    Ok(())
                }
            },
        }
    }

    async fn login(&mut self, mode: AccountMode, input: String) -> Result<()> {
        let input = input.trim().to_string();
        if input.is_empty() {
            return Err(anyhow!("enter a key or URI"));
        }

        match mode {
            AccountMode::ReadOnly => {
                let public_key = parse_public_key(&input)?;
                self.finish_login(Signer::ReadOnly(public_key), AccountMode::ReadOnly)
                    .await
            }
            AccountMode::Local => {
                let keys = Keys::parse(&input).context("invalid nsec")?;
                let public_key = keys.public_key();
                let npub = public_key.to_bech32()?;
                let nsec = keys
                    .secret_key()
                    .to_bech32()
                    .expect("bech32 encoding is infallible");
                if let Err(err) = secrets::store_nsec(&npub, &nsec) {
                    self.send(Update::Status(format!(
                        "signed in, but could not save key: {err:#}"
                    )))
                    .await;
                }
                self.finish_login(Signer::Local(keys), AccountMode::Local)
                    .await
            }
            AccountMode::Bunker => {
                let uri = NostrConnectUri::parse(&input).context("invalid bunker URI")?;
                let app_keys = Keys::generate();
                self.login_bunker(uri, app_keys, None).await
            }
        }
    }

    async fn login_bunker(
        &mut self,
        uri: NostrConnectUri,
        app_keys: Keys,
        expected_npub: Option<String>,
    ) -> Result<()> {
        let app_nsec = app_keys.secret_key().to_bech32()?;
        let connect = NostrConnect::new(uri, app_keys, Duration::from_secs(90), None)?;
        let public_key = connect
            .get_public_key_async()
            .await
            .map_err(|err| anyhow!("remote signer did not return a public key: {err}"))?;
        let npub = public_key.to_bech32()?;

        if let Some(expected) = expected_npub
            && expected != npub
        {
            return Err(anyhow!(
                "bunker returned {npub}, but the saved account is {expected}"
            ));
        }

        if let Ok(bunker_uri) = connect.bunker_uri().await
            && let Err(err) = secrets::store_bunker(&npub, &bunker_uri.to_string(), &app_nsec)
        {
            self.send(Update::Status(format!(
                "signed in, but could not save bunker session: {err:#}"
            )))
            .await;
        }

        self.finish_login(Signer::Bunker(Box::new(connect)), AccountMode::Bunker)
            .await
    }

    async fn finish_login(&mut self, signer: Signer, mode: AccountMode) -> Result<()> {
        let public_key = signer.public_key().await?;
        let npub = public_key.to_bech32()?;
        let account = AccountInfo {
            public_key,
            npub: npub.clone(),
            mode,
        };

        self.client.unsubscribe_all().await.ok();
        self.signer = Some(signer);
        self.account = Some(account.clone());
        self.requested_profiles.clear();
        self.followed = self.cached_follows(public_key).await;
        self.config.account = Some(AccountConfig { npub, mode });
        self.save_config().await;

        self.send(Update::LoginSuccess {
            account,
            relays: relay_snapshot(&self.client).await,
        })
        .await;

        self.send(Update::Status("Loading your follows…".to_string()))
            .await;
        if let Err(err) = self.refresh_follows(public_key).await {
            tracing::warn!("failed to refresh follow list: {err:#}");
            self.send(Update::Status(format!("Could not load follows: {err:#}")))
                .await;
        }

        self.load_timeline().await
    }

    async fn cached_follows(&self, public_key: PublicKey) -> Vec<PublicKey> {
        let filter = Filter::new()
            .kind(Kind::ContactList)
            .author(public_key)
            .limit(1);

        match self.client.database().query(filter).await {
            Ok(events) => {
                let latest = events.into_iter().max_by_key(|event| event.created_at);
                follow_authors(public_key, latest.as_ref())
            }
            Err(err) => {
                tracing::warn!("failed to query cached contact list: {err}");
                vec![public_key]
            }
        }
    }

    async fn refresh_follows(&mut self, public_key: PublicKey) -> Result<()> {
        let filter = Filter::new()
            .kind(Kind::ContactList)
            .author(public_key)
            .limit(1);

        let mut latest = self
            .client
            .database()
            .query(filter.clone())
            .await?
            .into_iter()
            .max_by_key(|event| event.created_at);

        match self
            .client
            .fetch_events(filter)
            .timeout(FETCH_TIMEOUT)
            .await
        {
            Ok(events) => {
                for event in events {
                    if latest
                        .as_ref()
                        .is_none_or(|current| event.created_at > current.created_at)
                    {
                        latest = Some(event);
                    }
                }
            }
            Err(err) => {
                tracing::warn!("failed to fetch contact list: {err}");
            }
        }

        if latest.is_none() {
            self.send(Update::Status(
                "No contact list found; showing your notes only.".to_string(),
            ))
            .await;
        }

        let followed = follow_authors(public_key, latest.as_ref());
        if followed.len() != self.followed.len()
            || !followed.iter().all(|key| self.followed.contains(key))
        {
            self.followed = followed;
            self.send(Update::Status(format!(
                "Showing notes from {} account(s).",
                self.followed.len()
            )))
            .await;
        }

        Ok(())
    }

    async fn reload_timeline(&mut self) -> Result<()> {
        if let Some(account) = self.account.clone()
            && let Err(err) = self.refresh_follows(account.public_key).await
        {
            tracing::warn!("failed to refresh follow list: {err:#}");
        }
        self.load_timeline().await
    }

    async fn load_timeline(&mut self) -> Result<()> {
        if self.followed.is_empty()
            && let Some(account) = &self.account
        {
            self.followed.push(account.public_key);
        }

        let authors = self.followed.clone();
        let filter = Filter::new()
            .kind(Kind::TextNote)
            .authors(authors.clone())
            .limit(LIVE_LIMIT);
        let cached: Vec<Event> = self
            .client
            .database()
            .query(filter.clone())
            .await?
            .into_iter()
            .collect();

        self.send_profiles_for(&cached).await?;
        self.send(Update::Timeline { events: cached }).await;

        match self
            .client
            .fetch_events(filter)
            .timeout(FETCH_TIMEOUT)
            .await
        {
            Ok(events) => {
                let events: Vec<Event> = events.into_iter().collect();
                self.send_profiles_for(&events).await?;
                self.send(Update::Timeline { events }).await;
            }
            Err(err) => {
                self.send(Update::Status(format!("could not fetch timeline: {err}")))
                    .await;
            }
        }

        let live = Filter::new()
            .kind(Kind::TextNote)
            .authors(authors)
            .since(Timestamp::now());
        self.client.unsubscribe_all().await.ok();
        if let Err(err) = self.client.subscribe(live).await {
            self.send(Update::Error(format!("failed to subscribe: {err}")))
                .await;
        }

        Ok(())
    }

    async fn open_thread(&mut self, id: EventId) -> Result<()> {
        let focus = self
            .fetch_event(id)
            .await?
            .ok_or_else(|| anyhow!("note not found"))?;

        let root = match thread::root_id(&focus).filter(|root| *root != focus.id) {
            Some(root_id) => self
                .fetch_event(root_id)
                .await?
                .unwrap_or_else(|| focus.clone()),
            None => focus.clone(),
        };

        let filter = Filter::new()
            .kind(Kind::TextNote)
            .event(focus.id)
            .limit(200);
        let mut replies: Vec<Event> = self
            .client
            .database()
            .query(filter.clone())
            .await?
            .into_iter()
            .collect();
        sort_replies(&mut replies);

        let mut related = vec![root.clone(), focus.clone()];
        related.extend(replies.iter().cloned());
        self.send_profiles_for(&related).await?;
        self.send(Update::ThreadLoaded {
            root: root.clone(),
            focus: focus.clone(),
            replies: replies.clone(),
        })
        .await;

        match self
            .client
            .fetch_events(filter)
            .timeout(FETCH_TIMEOUT)
            .await
        {
            Ok(events) => {
                merge_replies(&mut replies, events);
                let mut related = vec![root.clone(), focus.clone()];
                related.extend(replies.iter().cloned());
                self.send_profiles_for(&related).await?;
                self.send(Update::ThreadLoaded {
                    root,
                    focus,
                    replies,
                })
                .await;
            }
            Err(err) => {
                self.send(Update::Status(format!("could not refresh thread: {err}")))
                    .await;
            }
        }

        Ok(())
    }

    async fn open_profile(&mut self, public_key: PublicKey) -> Result<()> {
        let metadata_filter = Filter::new()
            .kind(Kind::Metadata)
            .author(public_key)
            .limit(1);
        let notes_filter = Filter::new()
            .kind(Kind::TextNote)
            .author(public_key)
            .limit(30);

        let cached_metadata: Option<Metadata> = self
            .client
            .database()
            .query(metadata_filter.clone())
            .await?
            .into_iter()
            .next()
            .and_then(metadata_from_event);
        let mut notes: Vec<Event> = self
            .client
            .database()
            .query(notes_filter.clone())
            .await?
            .into_iter()
            .collect();
        feed::sort_events(&mut notes);

        self.send(Update::ProfileLoaded {
            public_key,
            metadata: cached_metadata.clone(),
            notes: notes.clone(),
        })
        .await;

        let metadata = match self
            .client
            .fetch_events(metadata_filter)
            .timeout(FETCH_TIMEOUT)
            .await
        {
            Ok(events) => events.into_iter().find_map(metadata_from_event),
            Err(_) => None,
        };

        match self
            .client
            .fetch_events(notes_filter)
            .timeout(FETCH_TIMEOUT)
            .await
        {
            Ok(events) => {
                feed::merge_events(&mut notes, events);
                self.send(Update::ProfileLoaded {
                    public_key,
                    metadata: metadata.or(cached_metadata),
                    notes,
                })
                .await;
            }
            Err(err) => {
                self.send(Update::Status(format!("could not refresh profile: {err}")))
                    .await;
            }
        }

        Ok(())
    }

    async fn publish_note(&mut self, content: String, reply_to: Option<EventId>) -> Result<()> {
        if content.trim().is_empty() {
            return Err(anyhow!("cannot publish an empty note"));
        }
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| anyhow!("login required"))?;
        if matches!(signer, Signer::ReadOnly(_)) {
            return Err(anyhow!("read-only accounts cannot publish"));
        }

        let event = if let Some(reply_id) = reply_to {
            let reply = self
                .fetch_event(reply_id)
                .await?
                .ok_or_else(|| anyhow!("the note being replied to was not found"))?;
            let root = match thread::root_id(&reply).filter(|root| *root != reply.id) {
                Some(root_id) => self.fetch_event(root_id).await?,
                None => None,
            };

            let builder = TextNoteReplyBuilder::new(content, &reply);
            let builder = match root.as_ref() {
                Some(root) => builder.root(root),
                None => builder,
            };
            signer.sign(builder.into_event_builder()).await?
        } else {
            signer
                .sign(EventBuilder::new(Kind::TextNote, content))
                .await?
        };

        let output = self.client.send_event(&event).await?;
        self.send(Update::NewEvent(event.clone())).await;
        self.send(Update::Published {
            event_id: event.id.to_hex(),
            success: output.success.len(),
            failed: output.failed.len(),
        })
        .await;

        if let Some(reply_id) = reply_to {
            self.open_thread(reply_id).await.ok();
        }

        Ok(())
    }

    async fn publish_reaction(&mut self, event_id: EventId, reaction: String) -> Result<()> {
        let target = self
            .fetch_event(event_id)
            .await?
            .ok_or_else(|| anyhow!("note not found"))?;
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| anyhow!("login required"))?;
        if matches!(signer, Signer::ReadOnly(_)) {
            return Err(anyhow!("read-only accounts cannot react"));
        }

        let target = ReactionTarget::new(&target, None);
        let builder = ReactionBuilder::new(target, reaction);
        let event = signer.sign(builder.into_event_builder()).await?;
        let output = self.client.send_event(&event).await?;
        self.send(Update::Status(format!(
            "reaction sent to {} relay(s), {} failed",
            output.success.len(),
            output.failed.len()
        )))
        .await;
        Ok(())
    }

    async fn add_relay(&mut self, url: String) -> Result<()> {
        let mut url = url.trim().to_string();
        if url.is_empty() {
            return Err(anyhow!("relay URL is empty"));
        }
        if !url.starts_with("ws://") && !url.starts_with("wss://") {
            url = format!("wss://{url}");
        }

        self.client
            .add_relay(url.as_str())
            .capabilities(RelayCapabilities::READ | RelayCapabilities::WRITE)
            .and_connect()
            .await?;
        self.config.set_relay(&url, true, true);
        self.save_config().await;
        self.send(Update::RelayStatus(relay_snapshot(&self.client).await))
            .await;
        self.send(Update::Status(format!("added relay {url}")))
            .await;
        Ok(())
    }

    async fn remove_relay(&mut self, url: String) -> Result<()> {
        self.client.remove_relay(url.as_str()).force().await?;
        self.config.remove_relay(&url);
        self.save_config().await;
        self.send(Update::RelayStatus(relay_snapshot(&self.client).await))
            .await;
        self.send(Update::Status(format!("removed relay {url}")))
            .await;
        Ok(())
    }

    async fn save_config(&self) {
        if let Err(err) = self.config.save(&self.paths.config) {
            self.send(Update::Status(format!("could not save config: {err:#}")))
                .await;
        }
    }

    async fn fetch_event(&self, id: EventId) -> Result<Option<Event>> {
        let filter = Filter::new().id(id);
        let cached = self
            .client
            .database()
            .query(filter.clone())
            .await?
            .into_iter()
            .next();
        if cached.is_some() {
            return Ok(cached);
        }

        match self
            .client
            .fetch_events(filter)
            .timeout(FETCH_TIMEOUT)
            .await
        {
            Ok(events) => Ok(events.into_iter().next()),
            Err(err) => {
                tracing::warn!("failed to fetch event {id}: {err}");
                Ok(None)
            }
        }
    }

    async fn send_profiles_for(&mut self, events: &[Event]) -> Result<()> {
        let mut seen = HashSet::new();
        let keys: Vec<PublicKey> = events
            .iter()
            .map(|event| event.pubkey)
            .filter(|key| seen.insert(*key))
            .collect();

        if keys.is_empty() {
            return Ok(());
        }

        let filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(keys.clone())
            .limit(keys.len().saturating_mul(2).max(1));
        let cached = self.client.database().query(filter).await?;
        let profiles = metadata_map(cached);

        let missing: Vec<PublicKey> = keys
            .into_iter()
            .filter(|key| !profiles.contains_key(key) && self.requested_profiles.insert(*key))
            .collect();

        if !profiles.is_empty() {
            self.send(Update::Profiles(profiles)).await;
        }
        spawn_profile_fetch(self.client.clone(), self.updates.clone(), missing);
        Ok(())
    }
}

fn follow_authors(public_key: PublicKey, contact_list: Option<&Event>) -> Vec<PublicKey> {
    let mut authors = HashSet::new();
    authors.insert(public_key);
    if let Some(event) = contact_list {
        authors.extend(event.tags.public_keys());
    }

    let mut authors: Vec<PublicKey> = authors.into_iter().collect();
    authors.sort_by_key(|key| key.to_hex());
    authors
}

fn relay_capabilities(read: bool, write: bool) -> RelayCapabilities {
    let mut capabilities = RelayCapabilities::NONE;
    if read {
        capabilities |= RelayCapabilities::READ;
    }
    if write {
        capabilities |= RelayCapabilities::WRITE;
    }
    capabilities
}

async fn relay_snapshot(client: &Client) -> Vec<RelayInfo> {
    let mut relays: Vec<RelayInfo> = client
        .relays()
        .all()
        .await
        .into_iter()
        .map(|(url, relay)| RelayInfo {
            url: url.to_string(),
            status: relay.status().to_string(),
            read: relay.capabilities().has_read(),
            write: relay.capabilities().has_write(),
        })
        .collect();
    relays.sort_by(|left, right| left.url.cmp(&right.url));
    relays
}

fn metadata_from_event(event: Event) -> Option<Metadata> {
    serde_json::from_str(&event.content).ok()
}

fn metadata_map(events: impl IntoIterator<Item = Event>) -> HashMap<PublicKey, Metadata> {
    let mut map: HashMap<PublicKey, Metadata> = HashMap::new();
    let mut timestamps: HashMap<PublicKey, Timestamp> = HashMap::new();

    for event in events {
        if event.kind != Kind::Metadata {
            continue;
        }
        let Some(metadata) = metadata_from_event(event.clone()) else {
            continue;
        };
        let newer = timestamps
            .get(&event.pubkey)
            .is_none_or(|timestamp| event.created_at > *timestamp);
        if newer {
            timestamps.insert(event.pubkey, event.created_at);
            map.insert(event.pubkey, metadata);
        }
    }

    map
}

fn spawn_profile_fetch(client: Client, updates: mpsc::Sender<Update>, public_keys: Vec<PublicKey>) {
    if public_keys.is_empty() {
        return;
    }

    tokio::spawn(async move {
        let filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(public_keys)
            .limit(100);
        if let Ok(Ok(events)) = tokio::time::timeout(Duration::from_secs(8), async move {
            client.fetch_events(filter).await
        })
        .await
        {
            let profiles = metadata_map(events);
            if !profiles.is_empty() {
                updates.send(Update::Profiles(profiles)).await.ok();
            }
        }
    });
}

fn sort_replies(replies: &mut [Event]) {
    replies.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.to_hex().cmp(&right.id.to_hex()))
    });
}

fn merge_replies(replies: &mut Vec<Event>, incoming: impl IntoIterator<Item = Event>) {
    let mut seen: HashSet<EventId> = replies.iter().map(|event| event.id).collect();
    for event in incoming {
        if seen.insert(event.id) {
            replies.push(event);
        }
    }
    sort_replies(replies);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact_list(public_keys: &[PublicKey]) -> Event {
        let builder = public_keys
            .iter()
            .fold(EventBuilder::new(Kind::ContactList, ""), |builder, key| {
                builder.tag(Tag::public_key(*key))
            });
        builder.finalize(&Keys::generate()).unwrap()
    }

    #[test]
    fn follows_include_self_and_contact_list() {
        let own = Keys::generate().public_key();
        let other = Keys::generate().public_key();
        let contacts = contact_list(&[other]);

        let authors = follow_authors(own, Some(&contacts));

        assert_eq!(authors.len(), 2);
        assert!(authors.contains(&own));
        assert!(authors.contains(&other));
    }

    #[test]
    fn missing_contact_list_falls_back_to_self() {
        let own = Keys::generate().public_key();

        assert_eq!(follow_authors(own, None), vec![own]);
    }
}
