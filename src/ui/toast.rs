use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub created_at: Instant,
    pub ttl: Duration,
}

#[derive(Debug, Clone, Copy)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

impl Toast {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Info,
            created_at: Instant::now(),
            ttl: Duration::from_secs(3),
        }
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Success,
            created_at: Instant::now(),
            ttl: Duration::from_secs(3),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Error,
            created_at: Instant::now(),
            ttl: Duration::from_secs(5),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.ttl
    }
}

pub fn draw_toasts(frame: &mut Frame, toasts: &[Toast], area: Rect) {
    let mut y = area.bottom().saturating_sub(2); // start above status bar

    for toast in toasts.iter().rev().take(3) {
        if toast.is_expired() {
            continue;
        }

        let width = (toast.message.len() as u16 + 4).min(area.width.saturating_sub(4));
        let x = area.right().saturating_sub(width + 2);
        let toast_area = Rect::new(x, y, width, 3);

        if y < area.y + 2 {
            break;
        }

        frame.render_widget(Clear, toast_area);

        let border_color = match toast.kind {
            ToastKind::Info => Color::Cyan,
            ToastKind::Success => Color::Green,
            ToastKind::Error => Color::Red,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(toast_area);
        frame.render_widget(block, toast_area);

        let text = Paragraph::new(toast.message.as_str())
            .style(Style::default().fg(Color::White));
        frame.render_widget(text, inner);

        y = y.saturating_sub(4);
    }
}
