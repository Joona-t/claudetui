use crate::app::{App, AppMode, FocusedPane};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let mode_str = match app.mode {
        AppMode::Normal => "",
        AppMode::FocusMode => " FOCUS ",
        AppMode::Input => " INPUT ",
        AppMode::Palette => " PALETTE ",
    };

    let session_info = app
        .sessions
        .get(app.active_session)
        .map(|s| format!(" {} [{}/{}] ", s.name, app.active_session + 1, app.sessions.len()))
        .unwrap_or_else(|| " No session ".to_string());

    let hints = match app.focused_pane {
        FocusedPane::Sidebar => "[j/k] Nav  [Enter] Select  [Tab] Focus  [Ctrl+T] New  [Ctrl+W] Close",
        FocusedPane::Terminal => "[Tab] Focus  [Ctrl+T] New  [Ctrl+W] Close  [Ctrl+F] Focus Mode",
        FocusedPane::Diff => "[j/k] Scroll  [Tab] Focus  [a] Stage  [r] Revert",
    };

    let branch = app.active_git_branch()
        .map(|b| format!("  {b}"))
        .unwrap_or_default();

    let left = vec![
        Span::styled(session_info, Style::default().bg(app.theme.accent).fg(Color::White).bold()),
        Span::raw("  "),
        Span::styled(mode_str, Style::default().bg(app.theme.mode_bg).fg(Color::Black).bold()),
        Span::styled(branch, Style::default().fg(Color::Cyan)),
    ];

    let right = Span::styled(
        format!(" {} ", hints),
        Style::default().fg(Color::DarkGray),
    );

    // Build the full line
    let left_line = Line::from(left);
    let left_width: u16 = left_line.spans.iter().map(|s| s.content.len() as u16).sum();

    // Render left-aligned status
    let status = Paragraph::new(left_line).style(Style::default().bg(app.theme.status_bg));
    frame.render_widget(status, area);

    // Only render hints if there's enough room (avoid overlap)
    let hints_len = hints.len() as u16 + 2;
    if area.width > left_width + hints_len {
        let right_line = Line::from(vec![right]);
        let hints_widget = Paragraph::new(right_line)
            .alignment(Alignment::Right)
            .style(Style::default().bg(app.theme.status_bg));
        frame.render_widget(hints_widget, area);
    }
}
