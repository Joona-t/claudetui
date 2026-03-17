use crate::config::{Config, ThemeColors};
use crate::git::diff::{DiffWorker, GitDiffState};
use crate::git::watcher::GitWatcher;
use crate::pty::session::PtySession;
use crate::ui::command_palette::PaletteState;
use crate::ui::toast::Toast;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedPane {
    Sidebar,
    Terminal,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Normal,
    FocusMode,
    /// Text input overlay is active (e.g. new session directory prompt)
    Input,
    /// Command palette overlay
    Palette,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputPurpose {
    NewSession,
}

pub struct InputState {
    pub purpose: InputPurpose,
    pub prompt: String,
    pub buffer: String,
    pub cursor: usize,
}

impl InputState {
    pub fn new_session() -> Self {
        let default_dir = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "~".to_string());
        Self {
            purpose: InputPurpose::NewSession,
            prompt: "New session directory:".to_string(),
            buffer: default_dir,
            cursor: 0, // will be set to end
        }
    }

    pub fn cursor_to_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_back(&mut self) {
        if self.cursor > 0 {
            // Find the previous char boundary
            let prev = self.buffer[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.buffer.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursor < self.buffer.len() {
            let next = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.buffer.len());
            self.buffer.drain(self.cursor..next);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.buffer.len());
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    /// Delete from cursor to end of line (Ctrl+K)
    pub fn kill_to_end(&mut self) {
        self.buffer.truncate(self.cursor);
    }

    /// Delete the word before cursor (Ctrl+W / Alt+Backspace)
    pub fn delete_word_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Skip trailing whitespace/slashes, then delete until next separator
        let before = &self.buffer[..self.cursor];
        let new_cursor = before
            .rfind(|c: char| c == '/' || c == ' ')
            .map(|i| if i == self.cursor - 1 {
                // We're right after a separator, find the one before it
                before[..i].rfind(|c: char| c == '/' || c == ' ')
                    .map(|j| j + 1)
                    .unwrap_or(0)
            } else {
                i + 1
            })
            .unwrap_or(0);
        self.buffer.drain(new_cursor..self.cursor);
        self.cursor = new_cursor;
    }
}

pub struct Session {
    pub name: String,
    pub directory: String,
    pub pty: PtySession,
    pub diff_state: GitDiffState,
    pub last_restart: Option<Instant>,
    pub watcher: Option<GitWatcher>,
    pub git_branch: Option<String>,
    pub last_diff_refresh: Instant,
}

fn resolve_git_branch(directory: &str) -> Option<String> {
    git2::Repository::discover(directory)
        .ok()
        .and_then(|repo| {
            repo.head().ok().and_then(|h| {
                h.shorthand().map(|s| s.to_string())
            })
        })
}

pub struct App {
    pub sessions: Vec<Session>,
    pub active_session: usize,
    pub focused_pane: FocusedPane,
    pub mode: AppMode,
    pub sidebar_visible: bool,
    pub diff_visible: bool,
    pub should_quit: bool,
    pub input: Option<InputState>,
    pub config: Config,
    pub theme: ThemeColors,
    pub palette: Option<PaletteState>,
    pub toasts: Vec<Toast>,
    pub diff_worker: DiffWorker,
}

impl App {
    pub fn new() -> Self {
        let (config, config_warning) = Config::load();
        let theme = config.theme_colors();
        let mut toasts = Vec::new();
        if let Some(warning) = config_warning {
            toasts.push(Toast::error(warning));
        }
        Self {
            sessions: Vec::new(),
            active_session: 0,
            focused_pane: FocusedPane::Terminal,
            mode: AppMode::Normal,
            sidebar_visible: config.layout.sidebar_visible,
            diff_visible: config.layout.diff_visible,
            should_quit: false,
            input: None,
            config,
            theme,
            palette: None,
            toasts,
            diff_worker: DiffWorker::new(),
        }
    }

    pub fn add_session(&mut self, name: String, directory: String, pty: PtySession) {
        let watcher = GitWatcher::new(&directory).ok();
        let git_branch = resolve_git_branch(&directory);
        // Request background diff — no blocking I/O here
        self.diff_worker.request_refresh(&directory);
        self.sessions.push(Session {
            name,
            directory,
            pty,
            diff_state: GitDiffState::default(),
            watcher,
            last_restart: None,
            git_branch,
            last_diff_refresh: Instant::now(),
        });
        self.active_session = self.sessions.len() - 1;
    }

    /// Get the active session's diff state
    pub fn active_diff(&self) -> Option<&GitDiffState> {
        self.sessions.get(self.active_session).map(|s| &s.diff_state)
    }

    /// Get the active session's diff state mutably
    pub fn active_diff_mut(&mut self) -> Option<&mut GitDiffState> {
        self.sessions.get_mut(self.active_session).map(|s| &mut s.diff_state)
    }

    /// Poll background diff worker for results and check watchers for new changes.
    /// Zero blocking I/O — only microsecond mutex reads and channel sends.
    pub fn poll_git_watchers(&mut self) {
        // Phase 1: Check if the background worker has a result ready
        if let Some((dir, files)) = self.diff_worker.take_result() {
            // Match result to the session that owns this directory
            for session in &mut self.sessions {
                if session.directory == dir {
                    session.diff_state.files = files.clone();
                    session.diff_state.error = None;
                    let max = session.diff_state.total_lines().saturating_sub(1);
                    if session.diff_state.scroll_offset > max {
                        session.diff_state.scroll_offset = 0;
                    }
                    session.git_branch = resolve_git_branch(&session.directory);
                    session.last_diff_refresh = Instant::now();
                    break;
                }
            }
        }

        // Phase 2: Check watchers — if changes detected, request background refresh
        for session in &self.sessions {
            let needs_refresh = if let Some(ref watcher) = session.watcher {
                watcher.poll_changes()
            } else {
                false
            };
            // Periodic fallback: refresh every 5s even if watcher missed changes
            let periodic = session.last_diff_refresh.elapsed().as_secs() >= 5;

            if needs_refresh || periodic {
                self.diff_worker.request_refresh(&session.directory);
            }
        }
    }

    /// Request a diff refresh for the active session (used after stage/revert)
    pub fn request_diff_refresh(&self) {
        if let Some(session) = self.sessions.get(self.active_session) {
            self.diff_worker.request_refresh(&session.directory);
        }
    }

    /// Remove session at index, killing its PTY. Returns true if any sessions remain.
    pub fn remove_session(&mut self, index: usize) -> bool {
        if index >= self.sessions.len() {
            return !self.sessions.is_empty();
        }

        // Drop the session (PtySession's Drop will clean up the child)
        self.sessions.remove(index);

        if self.sessions.is_empty() {
            self.active_session = 0;
            return false;
        }

        // Adjust active index
        if self.active_session >= self.sessions.len() {
            self.active_session = self.sessions.len() - 1;
        } else if self.active_session > index {
            self.active_session -= 1;
        }

        true
    }

    /// Close the currently active session. Returns true if sessions remain.
    pub fn close_active_session(&mut self) -> bool {
        self.remove_session(self.active_session)
    }

    /// Start the new-session input dialog
    pub fn begin_new_session_input(&mut self) {
        let mut input = InputState::new_session();
        input.cursor_to_end();
        self.input = Some(input);
        self.mode = AppMode::Input;
    }

    /// Cancel input mode
    pub fn cancel_input(&mut self) {
        self.input = None;
        self.mode = AppMode::Normal;
    }

    /// Confirm input and return the buffer contents
    pub fn confirm_input(&mut self) -> Option<(InputPurpose, String)> {
        if let Some(input) = self.input.take() {
            self.mode = AppMode::Normal;
            Some((input.purpose, input.buffer))
        } else {
            None
        }
    }

    pub fn active_pty(&self) -> Option<&PtySession> {
        self.sessions.get(self.active_session).map(|s| &s.pty)
    }

    pub fn active_pty_mut(&mut self) -> Option<&mut PtySession> {
        self.sessions.get_mut(self.active_session).map(|s| &mut s.pty)
    }

    pub fn switch_session(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_session = index;
        }
    }

    pub fn next_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_session = (self.active_session + 1) % self.sessions.len();
        }
    }

    pub fn prev_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_session = if self.active_session == 0 {
                self.sessions.len() - 1
            } else {
                self.active_session - 1
            };
        }
    }

    pub fn cycle_focus(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::Sidebar => FocusedPane::Terminal,
            FocusedPane::Terminal if self.diff_visible => FocusedPane::Diff,
            FocusedPane::Terminal if self.sidebar_visible => FocusedPane::Sidebar,
            FocusedPane::Terminal => FocusedPane::Terminal,
            FocusedPane::Diff if self.sidebar_visible => FocusedPane::Sidebar,
            FocusedPane::Diff => FocusedPane::Terminal,
        };
    }

    pub fn toggle_focus_mode(&mut self) {
        self.mode = match self.mode {
            AppMode::Normal => AppMode::FocusMode,
            AppMode::FocusMode => AppMode::Normal,
            AppMode::Input | AppMode::Palette => self.mode,
        };
    }

    pub fn open_palette(&mut self) {
        let sessions: Vec<(String, usize)> = self.sessions
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.clone(), i))
            .collect();
        self.palette = Some(PaletteState::new(&sessions));
        self.mode = AppMode::Palette;
    }

    pub fn close_palette(&mut self) {
        self.palette = None;
        self.mode = AppMode::Normal;
    }

    pub fn add_toast(&mut self, toast: Toast) {
        self.toasts.push(toast);
    }

    pub fn prune_toasts(&mut self) {
        self.toasts.retain(|t| !t.is_expired());
    }

    pub fn cycle_theme(&mut self) {
        let next = match self.config.theme.name.as_str() {
            "dark" => "light",
            "light" => "solarized",
            _ => "dark",
        };
        self.config.theme.name = next.to_string();
        self.theme = self.config.theme_colors();
        let _ = self.config.save();
        self.add_toast(Toast::info(format!("Theme: {}", next)));
    }

    pub fn save_layout(&mut self) {
        self.config.layout.sidebar_visible = self.sidebar_visible;
        self.config.layout.diff_visible = self.diff_visible;
        let _ = self.config.save();
    }

    /// Get the git branch for the active session (cached, refreshed on git changes)
    pub fn active_git_branch(&self) -> Option<&str> {
        self.sessions.get(self.active_session)
            .and_then(|s| s.git_branch.as_deref())
    }
}

/// Extract a short session name from a directory path.
/// Uses the last path component, or the last two if the parent is generic (src, app, etc).
pub fn session_name_from_dir(dir: &str) -> String {
    let expanded = if dir.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            home.join(&dir[1..].trim_start_matches('/')).to_string_lossy().to_string()
        } else {
            dir.to_string()
        }
    } else {
        dir.to_string()
    };

    let path = Path::new(&expanded);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "session".to_string());

    name
}
