# Bugs & Iterations

## 2026-03-17: Black Screen Fix

**Problem:** ClaudeTUI showed a black screen on startup when run from `~/` (home directory), which is a git repo with hundreds of untracked files/directories.

**Root cause:** Three interacting problems blocking the main thread:

1. `add_session()` called `compute_diff()` synchronously before the event loop started. With `recurse_untracked_dirs(true)`, git2 recursively scanned all untracked dirs in `~/` — blocked for minutes. Terminal was in raw mode + alternate screen, nothing rendered.

2. `GitWatcher` watched `~/` with `RecursiveMode::Recursive` — generated constant filesystem events from other apps writing to the home dir.

3. `poll_git_watchers()` ran every 16ms and called `compute_diff()` synchronously on each watcher event — event loop starved, frames never drawn.

**Fix:**
- Disabled recursive untracked dir scanning (`recurse_untracked_dirs(false)`)
- Scoped GitWatcher to `.git/` recursive + working tree non-recursive
- Added `DiffWorker` background thread with channel-based request/result pattern
- Wired async diff into App — `add_session()` and `poll_git_watchers()` are fully non-blocking
- Added 5-second periodic diff refresh fallback
- Fixed terminal_pane allocation hot path (`&'static str` instead of `String` per empty cell per frame)

## 2026-03-17: Theme Cycling Crash

**Problem:** Selecting "Cycle theme" from the command palette caused a panic: `index outside of buffer` in ratatui-core's `buffer.rs`.

**Root cause:** Theme change triggered a full re-render with different color values, hitting an off-by-one in ratatui's buffer indexing (y coordinate equaled height instead of height-1).

**Fix:** Removed theme cycling entirely — it was incomplete (only 3 themes, no real visual changes) and not worth debugging at this stage. Can revisit with proper theme support later.

## 2026-07-08: BUG-P1-8 — Unconditional redraw burns ~106k-144k allocs/sec on idle frames

**Problem:** The main loop called `terminal.draw(|frame| ui::draw(frame, &app))` on every iteration of the `event::poll(16ms)` loop — i.e. up to 60 times/sec — regardless of whether anything on screen had actually changed. `ui::terminal_pane::draw()` rebuilds the entire cell grid from the vt100 `Screen` every call: one `String`/`Span` allocation per visible cell (80x24 = 1920 cells) plus a `Vec<Span>` per row and a `Vec<Line>` for the frame, even when the PTY was sitting idle with a static screen (e.g. Claude Code finished responding and waiting on user input). README already flagged this informally as "~144k heap allocs/sec at 60fps."

**Root cause:** No signal existed to distinguish "screen content changed" from "poll timer fired." The render loop treated every 16ms tick as a reason to redraw, so idle time cost the same as active-typing time.

**Fix:** Added a dirty-flag / diff-based redraw gate:
- `PtySession` (`src/pty/session.rs`) now carries an `Arc<AtomicU64> generation` counter, bumped by the background reader thread every time a chunk of new PTY output is processed into the vt100 parser. Exposed via `PtySession::generation()` — a single atomic load, no parser mutex contention.
- `App` (`src/app.rs`) gained a `dirty: bool` field and `mark_dirty()`. Toast add/expiry (only when the toast list actually shrinks) and applied git-diff-worker results now call `mark_dirty()`.
- `run_app()`'s main loop (`src/main.rs`) checks the active session's `generation()` each tick; if it changed since the last render, or a key/resize event landed, it marks the app dirty. `terminal.draw()` now only runs `if app.dirty`, then clears the flag. Idle frames (no keypress, no new PTY output, no toast/diff change) skip the redraw entirely — down to an atomic load + a couple of branches.
- Added a `--bench-redraw` mode (`run_redraw_benchmark()` in `src/main.rs`) that measures real allocation counts via a counting `#[global_allocator]` wrapper, comparing "redraw every idle tick" vs "dirty-flag gated" over 60 simulated idle frames against a real spawned PTY session and the actual `terminal_pane::draw` code path (not a hand-estimate).

**Measured (crude, `cargo run --release -- --bench-redraw`, 80x24 pane, static 3-line idle screen, reproducible across repeated runs):**
```
before (redraw every tick):   106328 allocations  (1772/frame)
after  (dirty-flag gated):      1780 allocations    (30/frame)
reduction: 98.3%
```
At 60fps that's ~106k allocs/sec before vs. ~1.8k allocs/sec after on a truly idle session (in the same ballpark as the README's informal ~144k/sec estimate, which used a larger/differently-colored real Claude Code screen). The residual ~30 allocs on the one frame that does draw is the existing per-cell `Span`/`Vec` cost from `terminal_pane::draw` — untouched by this fix, candidate for a follow-up if it ever matters (P2, not in scope here).

**Verification:** `cargo build --release` green, same pre-existing warning count (6, none new). `cargo test --release` green (0 tests in repo — no existing test suite to regress). No interactive TTY available in this environment to smoke-test the live app end-to-end (crossterm's `enable_raw_mode()` requires a real tty; this sandbox's Bash tool has none), so verification is via the release build + the `--bench-redraw` harness, which exercises the real `PtySession` and `terminal_pane::draw` code paths end-to-end with a live spawned subprocess.
