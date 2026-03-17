use anyhow::Result;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// Watches a git repository for file changes.
/// Watches .git/ recursively (stage/commit/branch changes) and the working
/// tree non-recursively (top-level edits). Bails if .git doesn't exist.
pub struct GitWatcher {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<()>,
}

impl GitWatcher {
    pub fn new(path: &str) -> Result<Self> {
        let root = Path::new(path);
        let git_dir = root.join(".git");
        if !git_dir.exists() {
            anyhow::bail!("not a git repository: {}", path);
        }

        let (tx, rx) = mpsc::channel();
        let debounce_tx = tx.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    use notify::EventKind;
                    match event.kind {
                        EventKind::Create(_)
                        | EventKind::Modify(_)
                        | EventKind::Remove(_) => {
                            let _ = debounce_tx.send(());
                        }
                        _ => {}
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        // Watch .git/ recursively — catches stage, commit, branch changes
        watcher.watch(&git_dir, RecursiveMode::Recursive)?;
        // Watch working tree non-recursively — catches top-level file edits
        watcher.watch(root, RecursiveMode::NonRecursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Check if any file changes have occurred (non-blocking).
    /// Returns true if changes detected, consuming all pending notifications.
    pub fn poll_changes(&self) -> bool {
        let mut changed = false;
        while self.rx.try_recv().is_ok() {
            changed = true;
        }
        changed
    }
}
