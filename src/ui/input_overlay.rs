use crate::app::InputState;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub fn draw(frame: &mut Frame, input: &InputState, area: Rect) {
    // Center a box: 60 chars wide, 5 lines tall
    let width = 62.min(area.width.saturating_sub(4));
    let height = 5.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(format!(" {} ", input.prompt))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Compute visible window of input text
    let visible_width = inner.width as usize;
    let cursor_char_pos = input.buffer[..input.cursor].chars().count();

    // Scroll so cursor is always visible
    let scroll_offset = if cursor_char_pos >= visible_width {
        cursor_char_pos - visible_width + 1
    } else {
        0
    };

    let display_text: String = input.buffer.chars()
        .skip(scroll_offset)
        .take(visible_width)
        .collect();
    let input_line = Paragraph::new(display_text)
        .style(Style::default().fg(Color::White));
    frame.render_widget(input_line, inner);

    // Render help text below input
    if inner.height > 1 {
        let help_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        let help = Paragraph::new("[Enter] Confirm  [Esc] Cancel  [Ctrl+W] Delete word")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(help, help_area);
    }

    // Position cursor in the input field (clamped to visible area)
    let cursor_in_view = (cursor_char_pos - scroll_offset) as u16;
    let cursor_x = inner.x + cursor_in_view.min(inner.width.saturating_sub(1));
    frame.set_cursor_position(Position::new(cursor_x, inner.y));
}
