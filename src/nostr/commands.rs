use nostr_sdk::prelude::*;

use crate::config::{AccountConfig, AccountMode};

#[derive(Debug, Clone)]
pub struct RelayInfo {
    pub url: String,
    pub status: String,
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub public_key: PublicKey,
    pub npub: String,
    pub mode: AccountMode,
}

#[derive(Debug, Clone)]
pub enum Command {
    AutoLogin(Option<AccountConfig>),
    Login {
        mode: AccountMode,
        input: String,
    },
    LoadTimeline,
    OpenThread(EventId),
    OpenProfile(PublicKey),
    PublishNote {
        content: String,
        reply_to: Option<EventId>,
    },
    React {
        event: EventId,
        reaction: String,
    },
    AddRelay {
        url: String,
    },
    RemoveRelay {
        url: String,
    },
    RefreshRelays,
    Shutdown,
}
