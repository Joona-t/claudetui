use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

pub struct PaletteState {
    pub query: String,
    pub cursor: usize,
    pub items: Vec<PaletteItem>,
    pub filtered: Vec<usize>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub label: String,
    pub kind: PaletteAction,
}

#[derive(Debug, Clone)]
pub enum PaletteAction {
    SwitchSession(usize),
    NewSession,
    CloseSession,
    ToggleSidebar,
    ToggleDiff,
    FocusMode,
    Quit,
}

impl PaletteState {
    pub fn new(sessions: &[(String, usize)]) -> Self {
        let mut items = Vec::new();

        // Session switches
        for (name, idx) in sessions {
            items.push(PaletteItem {
                label: format!("Switch to: {}", name),
                kind: PaletteAction::SwitchSession(*idx),
            });
        }

        // Actions
        items.push(PaletteItem { label: "New session (Ctrl+T)".to_string(), kind: PaletteAction::NewSession });
        items.push(PaletteItem { label: "Close session (Ctrl+W)".to_string(), kind: PaletteAction::CloseSession });
        items.push(PaletteItem { label: "Toggle sidebar".to_string(), kind: PaletteAction::ToggleSidebar });
        items.push(PaletteItem { label: "Toggle diff pane".to_string(), kind: PaletteAction::ToggleDiff });
        items.push(PaletteItem { label: "Focus mode (Ctrl+F)".to_string(), kind: PaletteAction::FocusMode });
        items.push(PaletteItem { label: "Quit (Ctrl+Q)".to_string(), kind: PaletteAction::Quit });

        let filtered: Vec<usize> = (0..items.len()).collect();

        Self {
            query: String::new(),
            cursor: 0,
            items,
            filtered,
            selected: 0,
        }
    }

    pub fn filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            let query_lower = self.query.to_lowercase();
            self.filtered = self.items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.label.to_lowercase().contains(&query_lower))
                .map(|(i, _)| i)
                .collect();
        }
        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }

    pub fn move_up(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = if self.selected == 0 {
                self.filtered.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn selected_action(&self) -> Option<&PaletteAction> {
        self.filtered
            .get(self.selected)
            .and_then(|&idx| self.items.get(idx))
            .map(|item| &item.kind)
    }

    pub fn insert_char(&mut self, c: char) {
        self.query.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.filter();
    }

    pub fn delete_back(&mut self) {
        if self.cursor > 0 {
            let prev = self.query[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.query.drain(prev..self.cursor);
            self.cursor = prev;
            self.filter();
        }
    }
}

pub fn draw(frame: &mut Frame, state: &PaletteState, area: Rect) {
    let width = 50.min(area.width.saturating_sub(4));
    let max_items = 10.min(state.filtered.len());
    let height = (max_items as u16 + 4).min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + 2; // near the top
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Command Palette ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 2 {
        return;
    }

    // Search input
    let search_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let search_text = if state.query.is_empty() {
        Paragraph::new("Type to search...").style(Style::default().fg(Color::DarkGray))
    } else {
        Paragraph::new(state.query.as_str()).style(Style::default().fg(Color::White))
    };
    frame.render_widget(search_text, search_area);

    // Separator
    if inner.height > 1 {
        let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        let sep = Paragraph::new("─".repeat(inner.width as usize))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(sep, sep_area);
    }

    // Items list
    if inner.height > 2 {
        let list_area = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(2));

        let items: Vec<ListItem> = state.filtered
            .iter()
            .enumerate()
            .take(list_area.height as usize)
            .map(|(i, &idx)| {
                let item = &state.items[idx];
                let style = if i == state.selected {
                    Style::default().fg(Color::White).bg(Color::Magenta).bold()
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(format!(" {}", item.label)).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, list_area);
    }

    // Cursor position
    let cursor_x = inner.x + state.cursor as u16;
    frame.set_cursor_position(Position::new(cursor_x.min(inner.right().saturating_sub(1)), inner.y));
}
