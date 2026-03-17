use crate::pty::session::PtySession;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn draw(frame: &mut Frame, pty: &PtySession, area: Rect, focused: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Claude Code ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Get the vt100 screen and convert to ratatui lines
    let screen = pty.screen();
    let mut lines: Vec<Line> = Vec::with_capacity(inner.height as usize);

    for row in 0..inner.height {
        let mut spans: Vec<Span> = Vec::with_capacity(inner.width as usize);
        let mut col = 0u16;

        while col < inner.width {
            let cell = screen.cell(row, col);
            if let Some(cell) = cell {
                let ch = cell.contents();

                let mut style = Style::default();

                // Map vt100 colors to ratatui colors
                style = style.fg(convert_color(cell.fgcolor()));
                style = style.bg(convert_color(cell.bgcolor()));

                if cell.bold() {
                    style = style.bold();
                }
                if cell.italic() {
                    style = style.italic();
                }
                if cell.underline() {
                    style = style.underlined();
                }
                if cell.inverse() {
                    // Swap fg/bg for inverse
                    let fg = style.fg.unwrap_or(Color::White);
                    let bg = style.bg.unwrap_or(Color::Black);
                    style = style.fg(bg).bg(fg);
                }

                if ch.is_empty() {
                    spans.push(Span::styled(" ", style));
                } else {
                    spans.push(Span::styled(ch, style));
                }
                col += 1;
            } else {
                spans.push(Span::raw(" "));
                col += 1;
            }
        }

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);

    // Position cursor where the PTY cursor is
    if focused {
        let cursor = screen.cursor_position();
        let cursor_x = inner.x + cursor.1;
        let cursor_y = inner.y + cursor.0;
        if cursor_x < inner.right() && cursor_y < inner.bottom() {
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
    }
}

fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
