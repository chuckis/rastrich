use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::app::{App, Screen};

pub mod composer;
pub mod help;
pub mod login;
pub mod profile;
pub mod relay_manager;
pub mod thread;
pub mod timeline;
pub mod widgets;

pub fn render(frame: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Login => login::render(frame, app),
        Screen::Main => render_main(frame, app),
    }

    if app.show_help {
        help::render(frame);
    }
}

fn render_main(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    widgets::render_header(frame, header, app);

    if app.profile.is_some() {
        profile::render(frame, body, app);
    } else if app.thread.is_some() {
        thread::render(frame, body, app);
    } else {
        timeline::render(frame, body, app);
    }

    widgets::render_footer(frame, footer, app);

    if app.composer.is_some() {
        composer::render(frame, area, app);
    }
    if app.relay_manager.is_some() {
        relay_manager::render(frame, area, app);
    }
}
