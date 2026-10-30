use std::collections::HashMap;

use chrono::{DateTime, Utc};
use nostr_sdk::prelude::*;

pub fn merge_events(existing: &mut Vec<Event>, incoming: impl IntoIterator<Item = Event>) {
    let mut by_id: HashMap<EventId, Event> =
        existing.drain(..).map(|event| (event.id, event)).collect();
    for event in incoming {
        by_id.insert(event.id, event);
    }
    existing.extend(by_id.into_values());
    sort_events(existing);
}

pub fn sort_events(events: &mut [Event]) {
    events.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.to_hex().cmp(&left.id.to_hex()))
    });
}

pub fn relative_time(timestamp: Timestamp) -> String {
    let now = Utc::now().timestamp();
    let then = timestamp.as_secs() as i64;
    let delta = now.saturating_sub(then);

    if delta < 0 {
        return "now".to_string();
    }
    if delta < 60 {
        return format!("{delta}s");
    }
    if delta < 3_600 {
        return format!("{}m", delta / 60);
    }
    if delta < 86_400 {
        return format!("{}h", delta / 3_600);
    }
    if delta < 604_800 {
        return format!("{}d", delta / 86_400);
    }

    DateTime::<Utc>::from_timestamp(then, 0)
        .map(|dt| dt.format("%b %d").to_string())
        .unwrap_or_else(|| format!("{}d", delta / 86_400))
}

pub fn short_npub(public_key: &PublicKey) -> String {
    let npub = public_key
        .to_bech32()
        .unwrap_or_else(|_| public_key.to_hex());
    short_npub_str(&npub)
}

pub fn short_npub_str(npub: &str) -> String {
    if npub.len() <= 16 {
        return npub.to_string();
    }
    format!("{}…{}", &npub[..10], &npub[npub.len() - 4..])
}

pub fn author_name(profiles: &HashMap<PublicKey, Metadata>, public_key: &PublicKey) -> String {
    profiles
        .get(public_key)
        .and_then(|metadata| {
            metadata
                .display_name
                .clone()
                .or_else(|| metadata.name.clone())
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| short_npub(public_key))
}

pub fn note_body(content: &str) -> String {
    if content.trim().is_empty() {
        "[empty note]".to_string()
    } else {
        content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: u8, created_at: u64) -> Event {
        let keys = Keys::generate();
        let mut event = EventBuilder::new(Kind::TextNote, "hello")
            .custom_created_at(Timestamp::from_secs(created_at))
            .finalize(&keys)
            .unwrap();
        event.id = EventId::from_slice(&[id; 32]).unwrap();
        event
    }

    #[test]
    fn merge_deduplicates_and_sorts() {
        let mut events = vec![event(1, 10), event(2, 20)];
        merge_events(&mut events, [event(1, 10), event(3, 30)]);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].id, EventId::from_slice(&[3; 32]).unwrap());
        assert_eq!(events[1].id, EventId::from_slice(&[2; 32]).unwrap());
    }

    #[test]
    fn relative_time_is_compact() {
        let now = Timestamp::now().as_secs();
        assert_eq!(relative_time(Timestamp::from_secs(now)), "0s");
        assert_eq!(
            relative_time(Timestamp::from_secs(now.saturating_sub(120))),
            "2m"
        );
    }

    #[test]
    fn empty_note_has_placeholder() {
        assert_eq!(note_body("   "), "[empty note]");
        assert_eq!(note_body("hi"), "hi");
    }
}
