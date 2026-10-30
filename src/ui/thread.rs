use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListItem, Padding, Paragraph};

use crate::app::App;
use crate::model::feed;
use crate::theme;
use crate::ui::widgets;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(view) = app.thread.as_mut() else {
        return;
    };

    let title = Line::from(vec![
        Span::styled(" Thread ", theme::title()),
        Span::styled(
            format!(
                "{} · {} replies ",
                feed::short_npub(&view.focus.pubkey),
                view.replies.len()
            ),
            theme::muted(),
        ),
    ]);
    let outer = widgets::block(title);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let width = inner.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = {
        let events = view.events();
        events
            .iter()
            .map(|event| widgets::note_item(event, &app.profiles, width))
            .collect()
    };

    if items.is_empty() {
        let empty = Paragraph::new("No notes in this thread.")
            .style(theme::muted())
            .alignment(Alignment::Center)
            .block(Block::new().padding(Padding::vertical(inner.height / 2)));
        frame.render_widget(empty, inner);
        return;
    }

    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("▌ ")
        .scroll_padding(2);

    frame.render_stateful_widget(list, inner, &mut view.state);
}
