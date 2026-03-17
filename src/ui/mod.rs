pub mod sidebar;
pub mod terminal_pane;
pub mod status_bar;
pub mod input_overlay;
pub mod diff_pane;
pub mod command_palette;
pub mod toast;

use crate::app::{App, AppMode, FocusedPane};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Reserve bottom row for status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    // Draw status bar
    status_bar::draw(frame, app, status_area);

    // In focus mode, only show the terminal pane
    if app.mode == AppMode::FocusMode {
        if let Some(pty) = app.active_pty() {
            terminal_pane::draw(frame, pty, content_area, true);
        } else {
            let block = Block::default().title("ClaudeTUI").borders(Borders::ALL);
            frame.render_widget(block, content_area);
        }
        return;
    }

    // Build horizontal constraints based on visible panes
    let mut constraints = Vec::new();
    if app.sidebar_visible {
        constraints.push(Constraint::Length(22));
    }
    constraints.push(Constraint::Min(40)); // PTY always visible
    if app.diff_visible {
        constraints.push(Constraint::Percentage(30));
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(content_area);

    let mut chunk_idx = 0;

    // Sidebar
    if app.sidebar_visible {
        let focused = app.focused_pane == FocusedPane::Sidebar;
        sidebar::draw(frame, app, chunks[chunk_idx], focused);
        chunk_idx += 1;
    }

    // Terminal pane
    let terminal_focused = app.focused_pane == FocusedPane::Terminal;
    if let Some(pty) = app.active_pty() {
        terminal_pane::draw(frame, pty, chunks[chunk_idx], terminal_focused);
    } else {
        // No sessions — show empty state
        let block = Block::default()
            .title(" Claude Code ")
            .borders(Borders::ALL)
            .border_style(if terminal_focused {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::DarkGray)
            });
        let text = ratatui::widgets::Paragraph::new("No sessions. Press Ctrl+T to create one.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(block);
        frame.render_widget(text, chunks[chunk_idx]);
    }
    chunk_idx += 1;

    // Diff pane
    if app.diff_visible {
        let focused = app.focused_pane == FocusedPane::Diff;
        let diff_state = app.active_diff().cloned().unwrap_or_default();
        diff_pane::draw(frame, &diff_state, chunks[chunk_idx], focused);
    }

    // Draw input overlay on top of everything if active
    if app.mode == AppMode::Input {
        if let Some(ref input) = app.input {
            input_overlay::draw(frame, input, area);
        }
    }

    // Draw command palette overlay
    if app.mode == AppMode::Palette {
        if let Some(ref palette) = app.palette {
            command_palette::draw(frame, palette, area);
        }
    }

    // Draw toasts
    toast::draw_toasts(frame, &app.toasts, area);
}
