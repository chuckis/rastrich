use std::collections::HashMap;

use nostr_sdk::prelude::*;

use crate::nostr::commands::{AccountInfo, RelayInfo};

#[derive(Debug, Clone)]
pub enum Update {
    Ready {
        relays: Vec<RelayInfo>,
    },
    RelayStatus(Vec<RelayInfo>),
    LoginRequired {
        mode: crate::config::AccountMode,
        npub: Option<String>,
        message: String,
    },
    LoginSuccess {
        account: AccountInfo,
        relays: Vec<RelayInfo>,
    },
    LoginFailed(String),
    Timeline {
        events: Vec<Event>,
    },
    NewEvent(Event),
    Profiles(HashMap<PublicKey, Metadata>),
    ThreadLoaded {
        root: Event,
        focus: Event,
        replies: Vec<Event>,
    },
    ProfileLoaded {
        public_key: PublicKey,
        metadata: Option<Metadata>,
        notes: Vec<Event>,
    },
    Published {
        event_id: String,
        success: usize,
        failed: usize,
    },
    Status(String),
    Error(String),
    Shutdown,
}
