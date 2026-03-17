use crate::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

pub fn draw(frame: &mut Frame, app: &App, area: Rect, focused: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let indicator = if i == app.active_session { "▶" } else { " " };
            let num = if i < 9 {
                format!("{}", i + 1)
            } else {
                " ".to_string()
            };

            let alive_marker = if session.pty.is_alive() { "" } else { " ✗" };

            let style = if i == app.active_session {
                Style::default().fg(Color::Magenta).bold()
            } else if !session.pty.is_alive() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            let label = format!("{} {} {}{}", indicator, num, session.name, alive_marker);
            ListItem::new(label).style(style)
        })
        .collect();

    let session_count = app.sessions.len();
    let title = format!(" Sessions ({}) ", session_count);

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    frame.render_widget(list, area);
}
