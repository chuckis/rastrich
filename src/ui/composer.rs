use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Padding};

use crate::app::App;
use crate::model::feed;
use crate::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(composer) = app.composer.as_mut() else {
        return;
    };

    let popup = area.centered(Constraint::Percentage(80), Constraint::Percentage(50));
    frame.render_widget(Clear, popup);

    let title = match &composer.reply_to {
        Some(event) => format!(" Reply to {} ", feed::short_npub(&event.pubkey)),
        None => " Compose ".to_string(),
    };

    let block = Block::bordered()
        .title(title)
        .title_bottom(" Ctrl+Enter send · Esc cancel ")
        .title_style(theme::title())
        .border_style(theme::border())
        .padding(Padding::uniform(1));

    composer.textarea.set_block(block);
    composer.textarea.set_style(theme::text());
    composer.textarea.set_placeholder_style(theme::muted());
    composer
        .textarea
        .set_cursor_style(Style::default().fg(theme::ACCENT));

    frame.render_widget(&composer.textarea, popup);

    let inner = popup.inner(Margin::new(2, 2));
    if inner.width > 0 && inner.height > 0 {
        let cursor = composer.textarea.cursor();
        let x = (inner.x + cursor.1 as u16).min(inner.right().saturating_sub(1));
        let y = (inner.y + cursor.0 as u16).min(inner.bottom().saturating_sub(1));
        frame.set_cursor_position((x, y));
    }
}
