use std::collections::HashMap;

use nostr_sdk::prelude::*;
use ratatui::prelude::*;
use ratatui::widgets::{Block, ListItem, Paragraph};

use crate::app::App;
use crate::model::{feed, thread};
use crate::theme;

pub fn block<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::bordered()
        .title(title)
        .title_style(theme::title())
        .border_style(theme::border())
}

pub fn note_item(
    event: &Event,
    profiles: &HashMap<PublicKey, Metadata>,
    width: usize,
) -> ListItem<'static> {
    let mut header = vec![
        Span::styled(feed::author_name(profiles, &event.pubkey), theme::author()),
        Span::raw(" "),
        Span::styled(feed::short_npub(&event.pubkey), theme::muted()),
        Span::raw(" · "),
        Span::styled(feed::relative_time(event.created_at), theme::muted()),
    ];

    if thread::is_reply(event) {
        header.push(Span::styled("  ↳ reply", theme::muted()));
    }

    let mut lines = vec![Line::from(header)];
    let body = feed::note_body(&event.content);
    for line in textwrap::wrap(&body, width.max(1)) {
        lines.push(Line::from(line.to_string()));
    }
    lines.push(Line::from(""));

    ListItem::new(Text::from(lines))
}

pub fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(24)]).areas(area);

    let account = match &app.account {
        Some(account) => {
            let name = feed::author_name(&app.profiles, &account.public_key);
            format!(
                "{} · {} · {}",
                name,
                account.mode.label(),
                feed::short_npub_str(&account.npub)
            )
        }
        None => "not signed in".to_string(),
    };

    let line = Line::from(vec![
        Span::styled(" rastrich ", theme::title()),
        Span::raw(" "),
        Span::styled(account, theme::muted()),
    ]);
    frame.render_widget(Paragraph::new(line), left);

    let relays = format!("{} relay(s) ", app.relays.len());
    frame.render_widget(
        Paragraph::new(relays)
            .style(theme::muted())
            .alignment(Alignment::Right),
        right,
    );
}

pub fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let (message, style) = match &app.status {
        Some((message, _)) => (message.clone(), theme::muted()),
        None => (hint(app), theme::muted()),
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {message}"), style))),
        area,
    );
}

fn hint(app: &App) -> String {
    if app.relay_manager.is_some() {
        "a add · d delete · r refresh · Esc close".to_string()
    } else if app.composer.is_some() {
        "Ctrl+Enter send · Esc cancel".to_string()
    } else if app.profile.is_some() {
        "j/k move · Enter open · r reply · l like · Esc back · ? help".to_string()
    } else if app.thread.is_some() {
        "j/k move · r reply · l like · p profile · Esc back · ? help".to_string()
    } else {
        "c compose · r reply · l like · Enter thread · p profile · R relays · ? help · q quit"
            .to_string()
    }
}
