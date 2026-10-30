use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListItem, Padding, Paragraph};

use crate::app::App;
use crate::theme;
use crate::ui::widgets;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let title = Line::from(vec![
        Span::styled(" Timeline ", theme::title()),
        Span::styled(format!("({} notes) ", app.events.len()), theme::muted()),
    ]);
    let outer = widgets::block(title);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if app.events.is_empty() {
        let empty = Paragraph::new("Loading notes…")
            .style(theme::muted())
            .alignment(Alignment::Center)
            .block(Block::new().padding(Padding::vertical(inner.height / 2)));
        frame.render_widget(empty, inner);
        return;
    }

    let width = inner.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = app
        .events
        .iter()
        .map(|event| widgets::note_item(event, &app.profiles, width))
        .collect();

    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("▌ ")
        .scroll_padding(2);

    frame.render_stateful_widget(list, inner, &mut app.timeline_state);
}
