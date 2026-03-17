mod app;
mod config;
mod git;
mod pty;
mod ui;

use app::{App, AppMode, FocusedPane, InputPurpose, session_name_from_dir};
use ui::toast::Toast;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    let mut app = App::new();

    // Get the initial terminal size for PTY
    let size = terminal.size()?;
    let (pty_cols, pty_rows) = calc_pty_size(&app, size);

    // Get current directory as default session
    let cwd = std::env::current_dir()?
        .to_string_lossy()
        .to_string();
    let name = session_name_from_dir(&cwd);

    // Spawn initial Claude Code session
    let pty_session = pty::session::PtySession::spawn("claude", &cwd, pty_rows, pty_cols)?;
    app.add_session(name, cwd, pty_session);

    let mut last_size = size;

    loop {
        // Draw UI
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // Handle resize
        let current_size = terminal.size()?;
        if current_size != last_size {
            last_size = current_size;
            let (pty_cols, pty_rows) = calc_pty_size(&app, current_size);
            if let Some(pty) = app.active_pty() {
                let _ = pty.resize(pty_rows, pty_cols);
            }
        }

        // Poll for events with a short timeout for responsive PTY updates
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if !handle_key(&mut app, key, last_size)? {
                        break;
                    }
                }
                Event::Resize(_, _) => {
                    // Handled above on next loop iteration
                }
                _ => {}
            }
        }

        // Poll git watchers for file changes
        app.poll_git_watchers();

        // Prune expired toasts
        app.prune_toasts();

        // Crash recovery: restart dead PTY sessions (with 5s backoff)
        for i in 0..app.sessions.len() {
            if !app.sessions[i].pty.is_alive() {
                let should_restart = app.sessions[i].last_restart
                    .map(|t| t.elapsed() >= Duration::from_secs(5))
                    .unwrap_or(true);
                if !should_restart { continue; }
                let dir = app.sessions[i].directory.clone();
                let (pty_cols, pty_rows) = calc_pty_size(&app, last_size);
                app.sessions[i].last_restart = Some(std::time::Instant::now());
                if let Ok(new_pty) = pty::session::PtySession::spawn("claude", &dir, pty_rows, pty_cols) {
                    app.sessions[i].pty = new_pty;
                    app.add_toast(Toast::info(format!("Restarted: {}", app.sessions[i].name)));
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Calculate PTY dimensions from terminal size, accounting for sidebar/diff/borders
fn calc_pty_size(app: &App, size: Size) -> (u16, u16) {
    let sidebar_width = if app.sidebar_visible { 22 } else { 0 };
    let diff_width = if app.diff_visible { size.width * 30 / 100 } else { 0 };
    let borders = 2; // left + right border of terminal pane
    let cols = size.width.saturating_sub(sidebar_width + diff_width + borders).max(40);
    let rows = size.height.saturating_sub(3); // borders + status bar
    (cols, rows)
}

/// Returns false if the app should quit
fn handle_key(app: &mut App, key: KeyEvent, term_size: Size) -> anyhow::Result<bool> {
    // If we're in input mode, route all keys to the input handler
    if app.mode == AppMode::Input {
        return handle_input_key(app, key, term_size);
    }

    // If palette is open, route keys there
    if app.mode == AppMode::Palette {
        return handle_palette_key(app, key, term_size);
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Global keybindings (work regardless of focus)
    match (ctrl, key.code) {
        // Ctrl+Q: quit
        (true, KeyCode::Char('q')) => {
            app.should_quit = true;
            return Ok(false);
        }
        // Tab: cycle focus (except when in terminal pane — Tab goes to PTY unless explicit)
        (false, KeyCode::Tab) if app.focused_pane != FocusedPane::Terminal => {
            app.cycle_focus();
            return Ok(true);
        }
        // Ctrl+F: toggle focus mode
        (true, KeyCode::Char('f')) => {
            app.toggle_focus_mode();
            return Ok(true);
        }
        // Ctrl+Left: toggle sidebar
        (true, KeyCode::Left) => {
            app.sidebar_visible = !app.sidebar_visible;
            app.save_layout();
            return Ok(true);
        }
        // Ctrl+Right: toggle diff pane
        (true, KeyCode::Right) => {
            app.diff_visible = !app.diff_visible;
            app.save_layout();
            return Ok(true);
        }
        // Ctrl+N: next session
        (true, KeyCode::Char('n')) => {
            app.next_session();
            return Ok(true);
        }
        // Ctrl+P: prev session
        (true, KeyCode::Char('p')) => {
            app.prev_session();
            return Ok(true);
        }
        // Ctrl+T: new session
        (true, KeyCode::Char('t')) => {
            app.begin_new_session_input();
            return Ok(true);
        }
        // Ctrl+W: close active session
        (true, KeyCode::Char('w')) => {
            if !app.close_active_session() {
                app.should_quit = true;
                return Ok(false);
            }
            return Ok(true);
        }
        // Ctrl+K: command palette
        (true, KeyCode::Char('k')) => {
            app.open_palette();
            return Ok(true);
        }
        _ => {}
    }

    // Session number switching (1-9) when not focused on terminal
    if app.focused_pane != FocusedPane::Terminal {
        if let KeyCode::Char(c @ '1'..='9') = key.code {
            let idx = (c as usize) - ('1' as usize);
            app.switch_session(idx);
            return Ok(true);
        }
    }

    // Pane-specific keybindings
    match app.focused_pane {
        FocusedPane::Terminal => {
            // Tab cycles focus if other panes are visible, otherwise forward to PTY
            if key.code == KeyCode::Tab && !ctrl {
                if app.sidebar_visible || app.diff_visible {
                    app.cycle_focus();
                    return Ok(true);
                }
                // No other panes — fall through to forward Tab to PTY
            }

            if let Some(pty) = app.active_pty() {
                let bytes = key_to_bytes(key);
                if !bytes.is_empty() {
                    pty.write(&bytes)?;
                }
            }
        }
        FocusedPane::Sidebar => {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => app.next_session(),
                KeyCode::Char('k') | KeyCode::Up => app.prev_session(),
                KeyCode::Enter => {
                    app.focused_pane = FocusedPane::Terminal;
                }
                _ => {}
            }
        }
        FocusedPane::Diff => {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if let Some(diff) = app.active_diff_mut() {
                        diff.scroll_down(1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if let Some(diff) = app.active_diff_mut() {
                        diff.scroll_up(1);
                    }
                }
                KeyCode::Char('d') if ctrl => {
                    if let Some(diff) = app.active_diff_mut() {
                        diff.scroll_down(10);
                    }
                }
                KeyCode::Char('u') if ctrl => {
                    if let Some(diff) = app.active_diff_mut() {
                        diff.scroll_up(10);
                    }
                }
                KeyCode::Char('g') => {
                    // gg = top (simplified: single g goes to top)
                    if let Some(diff) = app.active_diff_mut() {
                        diff.scroll_offset = 0;
                    }
                }
                KeyCode::Char('G') => {
                    if let Some(diff) = app.active_diff_mut() {
                        let max = diff.total_lines().saturating_sub(1);
                        diff.scroll_offset = max;
                    }
                }
                KeyCode::Char('a') => {
                    // Stage file at current position
                    let stage_result = app.sessions.get(app.active_session).and_then(|session| {
                        let file_idx = session.diff_state.selected_file;
                        session.diff_state.files.get(file_idx).map(|file| {
                            crate::git::diff::stage_file(&session.directory, &file.path)
                        })
                    });
                    if let Some(Err(e)) = stage_result {
                        app.add_toast(Toast::error(format!("Stage failed: {}", e)));
                    }
                    app.request_diff_refresh();
                }
                KeyCode::Char('r') => {
                    // Revert file at current position
                    let revert_result = app.sessions.get(app.active_session).and_then(|session| {
                        let file_idx = session.diff_state.selected_file;
                        session.diff_state.files.get(file_idx).map(|file| {
                            crate::git::diff::revert_file(&session.directory, &file.path)
                        })
                    });
                    if let Some(Err(e)) = revert_result {
                        app.add_toast(Toast::error(format!("Revert failed: {}", e)));
                    }
                    app.request_diff_refresh();
                }
                KeyCode::Tab => {
                    app.cycle_focus();
                }
                _ => {}
            }
        }
    }

    Ok(true)
}

/// Handle keyboard input while the input overlay is active
fn handle_input_key(app: &mut App, key: KeyEvent, term_size: Size) -> anyhow::Result<bool> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match (ctrl, key.code) {
        // Escape: cancel input
        (_, KeyCode::Esc) => {
            app.cancel_input();
        }
        // Ctrl+C: also cancel
        (true, KeyCode::Char('c')) => {
            app.cancel_input();
        }
        // Enter: confirm
        (_, KeyCode::Enter) => {
            if let Some((purpose, value)) = app.confirm_input() {
                match purpose {
                    InputPurpose::NewSession => {
                        spawn_session_from_input(app, &value, term_size)?;
                    }
                }
            }
        }
        // Ctrl+W: delete word back
        (true, KeyCode::Char('w')) => {
            if let Some(ref mut input) = app.input {
                input.delete_word_back();
            }
        }
        // Ctrl+K: kill to end
        (true, KeyCode::Char('k')) => {
            if let Some(ref mut input) = app.input {
                input.kill_to_end();
            }
        }
        // Ctrl+A: home
        (true, KeyCode::Char('a')) => {
            if let Some(ref mut input) = app.input {
                input.move_home();
            }
        }
        // Ctrl+E: end
        (true, KeyCode::Char('e')) => {
            if let Some(ref mut input) = app.input {
                input.move_end();
            }
        }
        // Ctrl+U: clear line
        (true, KeyCode::Char('u')) => {
            if let Some(ref mut input) = app.input {
                input.buffer.clear();
                input.cursor = 0;
            }
        }
        // Navigation
        (_, KeyCode::Left) => {
            if let Some(ref mut input) = app.input {
                input.move_left();
            }
        }
        (_, KeyCode::Right) => {
            if let Some(ref mut input) = app.input {
                input.move_right();
            }
        }
        (_, KeyCode::Home) => {
            if let Some(ref mut input) = app.input {
                input.move_home();
            }
        }
        (_, KeyCode::End) => {
            if let Some(ref mut input) = app.input {
                input.move_end();
            }
        }
        // Backspace
        (_, KeyCode::Backspace) => {
            if let Some(ref mut input) = app.input {
                input.delete_back();
            }
        }
        // Delete
        (_, KeyCode::Delete) => {
            if let Some(ref mut input) = app.input {
                input.delete_forward();
            }
        }
        // Regular character input
        (false, KeyCode::Char(c)) => {
            if let Some(ref mut input) = app.input {
                input.insert_char(c);
            }
        }
        _ => {}
    }

    Ok(true)
}

/// Spawn a new Claude Code session from the input overlay directory
fn spawn_session_from_input(app: &mut App, raw_dir: &str, term_size: Size) -> anyhow::Result<()> {
    let dir = expand_tilde(raw_dir.trim());

    // Canonicalize first to resolve symlinks, then validate
    let path = std::path::Path::new(&dir);
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            app.add_toast(Toast::error(format!("Not a directory: {}", dir)));
            return Ok(());
        }
    };
    if !canonical.is_dir() {
        app.add_toast(Toast::error(format!("Not a directory: {}", dir)));
        return Ok(());
    }

    let dir = canonical.to_string_lossy().to_string();
    let name = session_name_from_dir(&dir);
    let (pty_cols, pty_rows) = calc_pty_size(app, term_size);

    let pty_session = pty::session::PtySession::spawn("claude", &dir, pty_rows, pty_cols)?;
    app.add_session(name, dir, pty_session);
    app.focused_pane = FocusedPane::Terminal;

    Ok(())
}

/// Handle keyboard input while the command palette is open
fn handle_palette_key(app: &mut App, key: KeyEvent, term_size: Size) -> anyhow::Result<bool> {
    use crate::ui::command_palette::PaletteAction;

    match key.code {
        KeyCode::Esc => {
            app.close_palette();
        }
        KeyCode::Enter => {
            let action = app.palette.as_ref().and_then(|p| p.selected_action().cloned());
            app.close_palette();
            if let Some(action) = action {
                match action {
                    PaletteAction::SwitchSession(idx) => app.switch_session(idx),
                    PaletteAction::NewSession => app.begin_new_session_input(),
                    PaletteAction::CloseSession => {
                        if !app.close_active_session() {
                            app.should_quit = true;
                            return Ok(false);
                        }
                    }
                    PaletteAction::ToggleSidebar => {
                        app.sidebar_visible = !app.sidebar_visible;
                        app.save_layout();
                    }
                    PaletteAction::ToggleDiff => {
                        app.diff_visible = !app.diff_visible;
                        app.save_layout();
                    }
                    PaletteAction::FocusMode => app.toggle_focus_mode(),
                    PaletteAction::CycleTheme => app.cycle_theme(),
                    PaletteAction::Quit => {
                        app.should_quit = true;
                        return Ok(false);
                    }
                }
            }
        }
        KeyCode::Up => {
            if let Some(ref mut palette) = app.palette {
                palette.move_up();
            }
        }
        KeyCode::Down => {
            if let Some(ref mut palette) = app.palette {
                palette.move_down();
            }
        }
        KeyCode::Backspace => {
            if let Some(ref mut palette) = app.palette {
                palette.delete_back();
            }
        }
        KeyCode::Char(c) => {
            if let Some(ref mut palette) = app.palette {
                palette.insert_char(c);
            }
        }
        _ => {}
    }

    Ok(true)
}

/// Expand ~ to home directory
fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[1..].trim_start_matches('/')).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// Convert a crossterm KeyEvent into bytes to send to the PTY
fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char(c) if ctrl => {
            // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
            let byte = (c as u8).wrapping_sub(b'a').wrapping_add(1);
            vec![byte]
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            s.as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        _ => vec![],
    }
}
