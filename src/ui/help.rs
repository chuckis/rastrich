use ratatui::prelude::*;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::theme;
use crate::ui::widgets;

pub fn render(frame: &mut Frame) {
    let area = frame.area();
    let popup = area.centered(Constraint::Percentage(64), Constraint::Percentage(72));
    frame.render_widget(Clear, popup);

    let outer = widgets::block(" Help ");
    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let heading = Style::default()
        .fg(theme::ACCENT)
        .add_modifier(Modifier::BOLD);
    let lines = vec![
        Line::from(Span::styled("Navigation", heading)),
        Line::from("j / k, ↑ / ↓      move selection"),
        Line::from("g / G, Home / End jump to top / bottom"),
        Line::from("Enter             open thread or note"),
        Line::from("Esc               go back / close"),
        Line::from(""),
        Line::from(Span::styled("Actions", heading)),
        Line::from("c                 compose a note"),
        Line::from("r                 reply to selected note"),
        Line::from("l / +             react with +"),
        Line::from("p                 open author profile"),
        Line::from("R                 manage relays"),
        Line::from("Ctrl+R            reload timeline"),
        Line::from(""),
        Line::from(Span::styled("General", heading)),
        Line::from("?                 toggle this help"),
        Line::from("q / Ctrl+C        quit"),
    ];

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(theme::text())
            .wrap(Wrap { trim: false }),
        inner,
    );
}
