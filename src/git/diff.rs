use anyhow::Result;
use git2::{Delta, Diff, DiffFormat, DiffOptions, Repository};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineKind {
    Context,
    Addition,
    Deletion,
    Header,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub status: FileStatus,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl FileStatus {
    pub fn label(&self) -> &str {
        match self {
            FileStatus::Modified => "M",
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "?",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GitDiffState {
    pub files: Vec<FileDiff>,
    pub scroll_offset: usize,
    pub selected_file: usize,
    pub error: Option<String>,
}

impl GitDiffState {
    pub fn total_lines(&self) -> usize {
        let mut count = 0;
        for file in &self.files {
            count += 1; // file header
            for hunk in &file.hunks {
                count += 1; // hunk header
                count += hunk.lines.len();
            }
        }
        count
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let max = self.total_lines().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + amount).min(max);
    }
}

/// Parse a git2::Diff into our FileDiff structures using print() which takes a single closure
fn parse_diff(diff: &Diff, prefix: &str) -> Result<Vec<FileDiff>> {
    let mut files: Vec<FileDiff> = Vec::new();

    diff.print(DiffFormat::Patch, |delta, _hunk_opt, line| {
        match line.origin() {
            'F' => {
                // File header line — start a new file
                let path = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let status = match delta.status() {
                    Delta::Added => FileStatus::Added,
                    Delta::Deleted => FileStatus::Deleted,
                    Delta::Modified => FileStatus::Modified,
                    Delta::Renamed => FileStatus::Renamed,
                    Delta::Untracked => FileStatus::Untracked,
                    _ => FileStatus::Modified,
                };

                let display_path = if prefix.is_empty() {
                    path
                } else {
                    format!("{} {}", prefix, path)
                };

                files.push(FileDiff {
                    path: display_path,
                    status,
                    hunks: Vec::new(),
                });
            }
            'H' => {
                // Hunk header
                if let Some(file) = files.last_mut() {
                    let content = String::from_utf8_lossy(line.content()).trim().to_string();
                    file.hunks.push(DiffHunk {
                        header: content,
                        lines: Vec::new(),
                    });
                }
            }
            '+' | '-' | ' ' => {
                // Diff line
                if let Some(file) = files.last_mut() {
                    // If no hunk yet, create a default one
                    if file.hunks.is_empty() {
                        file.hunks.push(DiffHunk {
                            header: String::new(),
                            lines: Vec::new(),
                        });
                    }
                    if let Some(hunk) = file.hunks.last_mut() {
                        let kind = match line.origin() {
                            '+' => LineKind::Addition,
                            '-' => LineKind::Deletion,
                            _ => LineKind::Context,
                        };
                        let content = String::from_utf8_lossy(line.content())
                            .trim_end_matches('\n')
                            .to_string();
                        hunk.lines.push(DiffLine { kind, content });
                    }
                }
            }
            _ => {}
        }
        true
    })?;

    Ok(files)
}

/// Compute the diff for a git repository at the given path
pub fn compute_diff(repo_path: &str) -> Result<Vec<FileDiff>> {
    let path = Path::new(repo_path);
    let repo = Repository::discover(path)?;

    let mut diff_opts = DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(false);

    // Unstaged changes (workdir vs index)
    let unstaged = repo.diff_index_to_workdir(None, Some(&mut diff_opts))?;
    let files = parse_diff(&unstaged, "")?;

    // Staged changes (HEAD vs index)
    let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let staged = repo.diff_tree_to_index(head.as_ref(), None, Some(&mut diff_opts))?;
    let staged_files = parse_diff(&staged, "[staged]")?;

    // Staged first, then unstaged
    let mut all = staged_files;
    all.extend(files);
    Ok(all)
}

/// Validate that a file path stays within the repo working directory
fn validate_repo_path(repo: &Repository, file_path: &str) -> Result<String> {
    let clean = file_path.trim_start_matches("[staged] ");
    let workdir = repo.workdir()
        .ok_or_else(|| anyhow::anyhow!("bare repository"))?;
    let full = workdir.join(clean).canonicalize()
        .map_err(|_| anyhow::anyhow!("cannot resolve path: {}", clean))?;
    let workdir_canon = workdir.canonicalize().unwrap_or_else(|_| workdir.to_path_buf());
    if !full.starts_with(&workdir_canon) {
        anyhow::bail!("path traversal rejected: {}", clean);
    }
    Ok(clean.to_string())
}

/// Stage a file by index
pub fn stage_file(repo_path: &str, file_path: &str) -> Result<()> {
    let repo = Repository::discover(repo_path)?;
    let clean_path = validate_repo_path(&repo, file_path)?;
    let mut index = repo.index()?;
    index.add_path(Path::new(&clean_path))?;
    index.write()?;
    Ok(())
}

/// Revert a file's changes (checkout from HEAD)
pub fn revert_file(repo_path: &str, file_path: &str) -> Result<()> {
    let repo = Repository::discover(repo_path)?;
    let clean_path = validate_repo_path(&repo, file_path)?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .path(clean_path)
            .force(),
    ))?;
    Ok(())
}

/// Background diff computation worker.
/// Owns a thread that computes diffs off the main thread so the event loop never blocks.
pub struct DiffWorker {
    request_tx: mpsc::Sender<String>,
    result: Arc<Mutex<Option<(String, Vec<FileDiff>)>>>,
}

impl DiffWorker {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<String>();
        let result: Arc<Mutex<Option<(String, Vec<FileDiff>)>>> = Arc::new(Mutex::new(None));
        let result_clone = Arc::clone(&result);

        thread::spawn(move || {
            while let Ok(dir) = request_rx.recv() {
                // Drain queued requests, keep only the latest
                let mut latest = dir;
                while let Ok(newer) = request_rx.try_recv() {
                    latest = newer;
                }
                // Compute diff (this is the slow part — runs off main thread)
                if let Ok(files) = compute_diff(&latest) {
                    if let Ok(mut slot) = result_clone.lock() {
                        *slot = Some((latest, files));
                    }
                }
            }
        });

        Self { request_tx, result }
    }

    /// Request a diff refresh for the given directory. Non-blocking.
    pub fn request_refresh(&self, directory: &str) {
        let _ = self.request_tx.send(directory.to_string());
    }

    /// Take the latest computed result, if available. Non-blocking (microsecond mutex).
    pub fn take_result(&self) -> Option<(String, Vec<FileDiff>)> {
        self.result.lock().ok().and_then(|mut slot| slot.take())
    }
}
