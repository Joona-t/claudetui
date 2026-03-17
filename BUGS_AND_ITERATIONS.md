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
