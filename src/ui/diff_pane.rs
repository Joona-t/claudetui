use crate::git::diff::{FileStatus, GitDiffState, LineKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn draw(frame: &mut Frame, diff_state: &GitDiffState, area: Rect, focused: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let file_count = diff_state.files.len();
    let title = if file_count == 0 {
        " Git Diff ".to_string()
    } else {
        format!(" Git Diff ({} files) ", file_count)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if diff_state.files.is_empty() {
        if let Some(ref err) = diff_state.error {
            let text = Paragraph::new(err.as_str())
                .style(Style::default().fg(Color::Red));
            frame.render_widget(text, inner);
        } else {
            let text = Paragraph::new("No changes detected")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(text, inner);
        }
        return;
    }

    // Build all renderable lines
    let mut lines: Vec<Line> = Vec::new();

    for file in &diff_state.files {
        // File header line
        let status_style = match file.status {
            FileStatus::Modified => Style::default().fg(Color::Yellow).bold(),
            FileStatus::Added => Style::default().fg(Color::Green).bold(),
            FileStatus::Deleted => Style::default().fg(Color::Red).bold(),
            FileStatus::Renamed => Style::default().fg(Color::Cyan).bold(),
            FileStatus::Untracked => Style::default().fg(Color::DarkGray).bold(),
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", file.status.label()), status_style),
            Span::styled(&file.path, Style::default().fg(Color::White).bold()),
        ]));

        for hunk in &file.hunks {
            // Hunk header
            lines.push(Line::from(Span::styled(
                format!(" {}", hunk.header),
                Style::default().fg(Color::Cyan),
            )));

            // Diff lines
            for line in &hunk.lines {
                let (prefix, style) = match line.kind {
                    LineKind::Addition => ("+", Style::default().fg(Color::Green)),
                    LineKind::Deletion => ("-", Style::default().fg(Color::Red)),
                    LineKind::Context => (" ", Style::default().fg(Color::DarkGray)),
                    LineKind::Header => ("@", Style::default().fg(Color::Cyan)),
                };

                // Truncate long lines to fit the pane width
                let max_width = inner.width.saturating_sub(2) as usize;
                let content = if line.content.len() > max_width {
                    let mut end = max_width;
                    while end > 0 && !line.content.is_char_boundary(end) {
                        end -= 1;
                    }
                    &line.content[..end]
                } else {
                    &line.content
                };

                lines.push(Line::from(Span::styled(
                    format!("{}{}", prefix, content),
                    style,
                )));
            }
        }

        // Blank line between files
        lines.push(Line::from(""));
    }

    // Apply scroll offset
    let visible_height = inner.height as usize;
    let scroll = diff_state.scroll_offset;
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner);
}
