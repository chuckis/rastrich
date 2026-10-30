use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Padding, Paragraph, Wrap};

use crate::app::App;
use crate::config::AccountMode;
use crate::theme;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let popup = area.centered(Constraint::Percentage(72), Constraint::Percentage(60));
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" rastrich ")
        .title_style(theme::title())
        .title_alignment(Alignment::Center)
        .border_style(theme::border())
        .padding(Padding::uniform(1));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let login = &app.login;
    let masked = login.mode != AccountMode::ReadOnly;
    let display = if login.input.is_empty() {
        Span::styled("<paste here>", theme::muted())
    } else if masked {
        Span::styled("•".repeat(login.input.chars().count()), theme::text())
    } else {
        Span::styled(login.input.clone(), theme::text())
    };

    let instruction = match login.mode {
        AccountMode::ReadOnly => "Paste an npub or hex public key to browse without signing.",
        AccountMode::Local => "Paste an nsec. It is stored in your system keyring.",
        AccountMode::Bunker => "Paste a bunker:// URI from your NIP-46 signer.",
    };

    let status = if login.busy {
        Line::from(Span::styled(
            "Connecting…",
            Style::default().fg(theme::ACCENT),
        ))
    } else if let Some(error) = &login.error {
        Line::from(Span::styled(error.clone(), theme::error()))
    } else {
        Line::from("")
    };

    let lines = vec![
        Line::from(Span::styled(
            "Connect to Nostr",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Mode: ", theme::muted()),
            Span::styled(login.mode.label(), Style::default().fg(theme::ACCENT)),
            Span::styled("  (Tab to change)", theme::muted()),
        ]),
        Line::from(Span::styled(instruction, theme::muted())),
        Line::from(vec![Span::styled("Key: ", theme::muted()), display]),
        Line::from(""),
        status,
        Line::from(""),
        Line::from(Span::styled("Enter connect · Esc quit", theme::muted())),
    ];

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(theme::text())
            .wrap(Wrap { trim: true }),
        inner,
    );

    if !login.busy && inner.height > 4 {
        let x =
            (inner.x + 5 + login.input.chars().count() as u16).min(inner.right().saturating_sub(1));
        let y = inner.y + 4;
        frame.set_cursor_position((x, y));
    }
}
