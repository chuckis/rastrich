use ratatui::prelude::*;
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph};

use crate::app::App;
use crate::theme;
use crate::ui::widgets;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(manager) = app.relay_manager.as_mut() else {
        return;
    };

    let popup = area.centered(Constraint::Percentage(78), Constraint::Percentage(70));
    frame.render_widget(Clear, popup);

    let outer = widgets::block(Line::from(vec![
        Span::styled(" Relays ", theme::title()),
        Span::styled(format!("({}) ", app.relays.len()), theme::muted()),
    ]));
    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let [list_area, input_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let items: Vec<ListItem> = app
        .relays
        .iter()
        .map(|relay| {
            let capabilities = format!(
                "{}{}",
                if relay.read { "R" } else { "-" },
                if relay.write { "W" } else { "-" }
            );
            let line = Line::from(vec![
                Span::styled(format!("{:<12}", relay.status), status_style(&relay.status)),
                Span::raw(" "),
                Span::styled(relay.url.clone(), theme::text()),
                Span::raw("  "),
                Span::styled(capabilities, theme::muted()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let selected = if app.relays.is_empty() {
        None
    } else {
        Some(manager.selected.min(app.relays.len() - 1))
    };
    let mut state = ListState::default().with_selected(selected);

    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("▌ ");
    frame.render_stateful_widget(list, list_area, &mut state);

    let input = if manager.adding {
        Line::from(vec![
            Span::styled("Add relay: ", theme::muted()),
            Span::styled(manager.input.clone(), theme::text()),
            Span::styled("█", Style::default().fg(theme::ACCENT)),
        ])
    } else {
        Line::from(Span::styled(
            "a add · d delete · r refresh · Esc close",
            theme::muted(),
        ))
    };
    frame.render_widget(Paragraph::new(input), input_area);

    if manager.adding {
        let x = (input_area.x + 11 + manager.input.chars().count() as u16)
            .min(input_area.right().saturating_sub(1));
        frame.set_cursor_position((x, input_area.y));
    }
}

fn status_style(status: &str) -> Style {
    match status {
        "Connected" => theme::success(),
        "Connecting" | "Pending" => Style::default().fg(theme::ACCENT),
        "Disconnected" | "Terminated" | "Banned" | "Shutdown" => theme::error(),
        _ => theme::muted(),
    }
}
