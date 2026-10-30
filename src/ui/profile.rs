use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListItem, Padding, Paragraph, Wrap};

use crate::app::App;
use crate::model::feed;
use crate::theme;
use crate::ui::widgets;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(profile) = app.profile.as_mut() else {
        return;
    };

    let name = profile
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .display_name
                .clone()
                .or_else(|| metadata.name.clone())
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| feed::short_npub(&profile.public_key));

    let outer = widgets::block(Line::from(vec![
        Span::styled(format!(" Profile: {name} "), theme::title()),
        Span::styled(format!("{} notes ", profile.notes.len()), theme::muted()),
    ]));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let [header, notes] =
        Layout::vertical([Constraint::Length(6), Constraint::Min(1)]).areas(inner);

    let mut lines = vec![Line::from(vec![
        Span::styled(name, theme::author()),
        Span::raw("  "),
        Span::styled(feed::short_npub(&profile.public_key), theme::muted()),
    ])];

    if let Some(metadata) = &profile.metadata {
        if let Some(about) = metadata
            .about
            .as_ref()
            .filter(|about| !about.trim().is_empty())
        {
            lines.push(Line::from(about.clone()));
        }
        let mut links = Vec::new();
        if let Some(nip05) = metadata.nip05.as_ref().filter(|value| !value.is_empty()) {
            links.push(format!("nip05: {nip05}"));
        }
        if let Some(lud16) = metadata.lud16.as_ref().filter(|value| !value.is_empty()) {
            links.push(format!("⚡ {lud16}"));
        }
        if let Some(website) = metadata.website.as_ref().filter(|value| !value.is_empty()) {
            links.push(website.clone());
        }
        if !links.is_empty() {
            lines.push(Line::from(Span::styled(
                links.join("  ·  "),
                theme::muted(),
            )));
        }
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(theme::text())
            .wrap(Wrap { trim: true }),
        header,
    );

    if profile.notes.is_empty() {
        let empty = Paragraph::new("No notes yet.")
            .style(theme::muted())
            .alignment(Alignment::Center)
            .block(Block::new().padding(Padding::vertical(notes.height / 2)));
        frame.render_widget(empty, notes);
        return;
    }

    let width = notes.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = profile
        .notes
        .iter()
        .map(|event| widgets::note_item(event, &app.profiles, width))
        .collect();

    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("▌ ")
        .scroll_padding(2);

    frame.render_stateful_widget(list, notes, &mut profile.state);
}
