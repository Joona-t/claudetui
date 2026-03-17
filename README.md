# ClaudeTUI

A terminal multiplexer for [Claude Code](https://claude.ai/claude-code) built in Rust. Wraps the Claude Code CLI in embedded PTY sessions with a ratatui-based TUI layer, adding multi-session management, a git-aware diff pane, and a fuzzy command palette.

> **Status:** Experimental — Phase 1 complete, actively testing and iterating.

## Architecture

```
┌─────────────┬──────────────────────┬─────────────┐
│  Sidebar    │   Terminal Pane      │  Diff Pane   │
│  (sessions) │   (PTY → vt100)      │  (git2)      │
│             │                      │              │
│  Ctrl+T new │  Claude Code CLI     │  j/k scroll  │
│  j/k nav    │  embedded via        │  a   stage   │
│  1-9 switch │  portable-pty        │  r   revert  │
└─────────────┴──────────────────────┴─────────────┘
                    Status Bar
           [mode] [branch] [session] [focus]
```

### Core components

| Module | Responsibility |
|--------|---------------|
| `pty/session.rs` | Spawns Claude Code in a pseudo-terminal via `portable-pty`. Background reader thread feeds output into `vt100::Parser` for ANSI-correct rendering. Crash recovery with exponential backoff. |
| `git/diff.rs` | Diff computation via `git2`. Parses hunks into displayable lines with add/remove/context classification. Supports file staging and reverting. |
| `git/watcher.rs` | File system watcher (`notify` crate) triggers non-blocking diff refresh on changes. Zero blocking I/O in the main render loop. |
| `ui/terminal_pane.rs` | Renders vt100-parsed terminal state cell-by-cell into ratatui spans, preserving colors and attributes. |
| `ui/command_palette.rs` | Fuzzy-searchable action menu (Ctrl+K) using `nucleo-matcher`. |
| `app.rs` | Application state machine. Manages sessions, focus modes, config persistence, toast notifications, and background workers. |
| `config.rs` | TOML config at `~/.claudetui/config.toml`. File permissions hardened to 0600/0700. |

### Data flow

```
Keyboard Input → crossterm Events → App state machine
                                         │
                    ┌────────────────────┼────────────────────┐
                    ▼                    ▼                    ▼
              PTY write()         UI state update       Git operations
                    │                    │                    │
                    ▼                    ▼                    ▼
            Claude Code CLI      ratatui render()     git2 diff/stage
                    │                                        │
                    ▼                                        ▼
            vt100::Parser ──→ Terminal Pane          Diff Pane update
```

## Key bindings

| Key | Action |
|-----|--------|
| `Ctrl+T` | New session |
| `Ctrl+N/P` | Next/previous session |
| `1-9` | Switch to session N |
| `Ctrl+F` | Toggle focus mode (fullscreen terminal) |
| `Ctrl+Left` | Toggle sidebar |
| `Ctrl+Right` | Toggle diff pane |
| `Ctrl+K` | Command palette |
| `Tab` | Cycle focus between panes |

## Dependencies

Built on stable, well-maintained crates:

- **ratatui** — TUI rendering framework
- **crossterm** — Terminal event handling
- **portable-pty** — Cross-platform PTY spawning
- **vt100** — Terminal state parser
- **git2** — libgit2 bindings for diff/stage/revert
- **notify** — File system change watcher
- **nucleo-matcher** — Fuzzy matching (same engine as Helix editor)
- **tokio** — Async runtime

## Building

```bash
cargo build --release
./target/release/claudetui
```

Requires `claude` CLI to be installed and available in PATH.

## Current state

Phase 1 is complete — the core multiplexer works. Known areas for improvement:

- Hot path allocations in terminal rendering (~144k heap allocs/sec at 60fps)
- Git branch name queried from filesystem every frame (should be cached)
- Silent failures on git stage/revert operations (need toast feedback)
- Zombie process cleanup on PTY drop

These are tracked and planned for Phase 2.

## License

MIT
