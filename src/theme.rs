use ratatui::style::{Color, Modifier, Style};

pub const ACCENT: Color = Color::Cyan;
pub const MUTED: Color = Color::DarkGray;
pub const ERROR: Color = Color::Red;
pub const SUCCESS: Color = Color::Green;

pub fn border() -> Style {
    Style::default().fg(MUTED)
}

pub fn title() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn selected() -> Style {
    Style::default()
        .bg(Color::Indexed(238))
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

pub fn author() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

pub fn error() -> Style {
    Style::default().fg(ERROR)
}

pub fn success() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn text() -> Style {
    Style::default().fg(Color::Gray)
}
