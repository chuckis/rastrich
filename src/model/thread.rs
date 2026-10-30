use nostr_sdk::prelude::*;

fn event_id_from_tag(tag: &Tag) -> Option<EventId> {
    tag.content()
        .and_then(|value| EventId::from_hex(value).ok())
}

pub fn root_id(event: &Event) -> Option<EventId> {
    let mut first = None;
    let mut has_marker = false;

    for tag in event.tags.iter() {
        if tag.kind() != "e" {
            continue;
        }
        let Some(id) = event_id_from_tag(tag) else {
            continue;
        };
        first.get_or_insert(id);

        let marker = tag.as_slice().get(3).map(String::as_str);
        if marker.is_some() {
            has_marker = true;
        }
        if marker == Some("root") {
            return Some(id);
        }
    }

    has_marker.then_some(first).flatten()
}

pub fn is_reply(event: &Event) -> bool {
    event.tags.iter().any(|tag| {
        tag.kind() == "e"
            && matches!(
                tag.as_slice().get(3).map(String::as_str),
                Some("root" | "reply")
            )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note() -> Event {
        EventBuilder::new(Kind::TextNote, "hello")
            .finalize(&Keys::generate())
            .unwrap()
    }

    #[test]
    fn top_level_note_has_no_root() {
        assert!(root_id(&note()).is_none());
        assert!(!is_reply(&note()));
    }

    #[test]
    fn nip10_markers_are_understood() {
        let root = note();
        let reply = note();
        let event = EventBuilder::new(Kind::TextNote, "reply")
            .tag(Tag::custom(
                "e",
                [root.id.to_hex(), String::new(), "root".to_string()],
            ))
            .tag(Tag::custom(
                "e",
                [reply.id.to_hex(), String::new(), "reply".to_string()],
            ))
            .finalize(&Keys::generate())
            .unwrap();

        assert_eq!(root_id(&event), Some(root.id));
        assert!(is_reply(&event));
    }
}
