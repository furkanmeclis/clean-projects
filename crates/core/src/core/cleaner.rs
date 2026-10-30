//! Executes candidate actions with safety checks, streaming progress events.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};

use super::candidate::{Action, Candidate};
use super::fsutil::{check_deletable, dir_size, remove_path};
use super::state::{HistoryEntry, HistoryItem};

#[derive(Clone, Debug)]
pub struct CleanOptions {
    pub dry_run: bool,
    pub use_trash: bool,
    pub home: PathBuf,
    pub protected: Vec<PathBuf>,
}

/// Progress events emitted while cleaning.
#[derive(Clone, Debug)]
pub enum CleanEvent {
    Start {
        index: usize,
        total: usize,
        id: String,
        label: String,
    },
    Log(String),
    Item {
        id: String,
        ok: bool,
        freed: u64,
        error: Option<String>,
    },
    Done {
        freed: u64,
        ok: usize,
        failed: usize,
    },
}

/// Run every candidate in order, reporting through `emit`. Returns the history entry.
pub fn run(
    candidates: &[Candidate],
    opts: &CleanOptions,
    mut emit: impl FnMut(CleanEvent),
) -> HistoryEntry {
    let total = candidates.len();
    let mut freed = 0u64;
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut items = Vec::with_capacity(total);

    for (index, c) in candidates.iter().enumerate() {
        emit(CleanEvent::Start {
            index,
            total,
            id: c.id.clone(),
            label: c.label.clone(),
        });
        let result = execute(c, opts, &mut emit);
        match result {
            Ok(bytes) => {
                freed += bytes;
                ok += 1;
                emit(CleanEvent::Item {
                    id: c.id.clone(),
                    ok: true,
                    freed: bytes,
                    error: None,
                });
                items.push(HistoryItem {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    detail: c.detail(),
                    size: bytes,
                    ok: true,
                    error: None,
                });
            }
            Err(e) => {
                failed += 1;
                let msg = format!("{e:#}");
                emit(CleanEvent::Log(format!("  ✗ {msg}")));
                emit(CleanEvent::Item {
                    id: c.id.clone(),
                    ok: false,
                    freed: 0,
                    error: Some(msg.clone()),
                });
                items.push(HistoryItem {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    detail: c.detail(),
                    size: 0,
                    ok: false,
                    error: Some(msg),
                });
            }
        }
    }
    emit(CleanEvent::Done { freed, ok, failed });
    HistoryEntry {
        at: chrono::Local::now(),
        freed,
        dry_run: opts.dry_run,
        trashed: opts.use_trash,
        items,
    }
}

fn execute(c: &Candidate, opts: &CleanOptions, emit: &mut impl FnMut(CleanEvent)) -> Result<u64> {
    match &c.action {
        Action::RemovePath { path } => remove(path, c.size, opts, emit),
        Action::RunCommand { program, args, cwd } => {
            let mut cmd = Command::new(program);
            cmd.args(args);
            if let Some(cwd) = cwd {
                cmd.current_dir(cwd);
            }
            run_command(cmd, opts.dry_run, emit)?;
            Ok(c.size)
        }
        Action::GitWorktreePrune { repo } => {
            let mut cmd = git(repo);
            cmd.args(["worktree", "prune", "-v"]);
            run_command(cmd, opts.dry_run, emit)?;
            Ok(c.size)
        }
        Action::GitWorktreeRemove { repo, worktree } => {
            check_deletable(worktree, &opts.home, &opts.protected)
                .map_err(|e| anyhow!("{}: {e}", worktree.display()))?;
            let size = if c.size > 0 {
                c.size
            } else {
                dir_size(worktree)
            };
            let mut cmd = git(repo);
            cmd.args(["worktree", "remove"]);
            cmd.arg(worktree);
            run_command(cmd, opts.dry_run, emit)?;
            Ok(size)
        }
        Action::GitBranchDelete { repo, branch } => {
            let mut cmd = git(repo);
            // `-d` (not `-D`): git refuses if the branch is not fully merged.
            cmd.args(["branch", "-d", branch]);
            run_command(cmd, opts.dry_run, emit)?;
            Ok(0)
        }
        Action::GitGc { repo } => {
            let before = dir_size(&repo.join(".git"));
            let mut cmd = git(repo);
            cmd.args(["gc", "--prune=now", "--quiet"]);
            run_command(cmd, opts.dry_run, emit)?;
            if opts.dry_run {
                return Ok(c.size);
            }
            let after = dir_size(&repo.join(".git"));
            Ok(before.saturating_sub(after))
        }
    }
}

fn git(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

fn remove(
    path: &Path,
    known_size: u64,
    opts: &CleanOptions,
    emit: &mut impl FnMut(CleanEvent),
) -> Result<u64> {
    check_deletable(path, &opts.home, &opts.protected)
        .map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let size = if known_size > 0 {
        known_size
    } else {
        dir_size(path)
    };
    if opts.dry_run {
        emit(CleanEvent::Log(format!(
            "  [dry-run] would remove {}",
            path.display()
        )));
        return Ok(size);
    }
    if opts.use_trash {
        emit(CleanEvent::Log(format!("  trash {}", path.display())));
        match trash::delete(path) {
            Ok(()) => return Ok(size),
            Err(e) => {
                emit(CleanEvent::Log(format!(
                    "  trash unavailable ({e}), deleting permanently"
                )));
            }
        }
    } else {
        emit(CleanEvent::Log(format!("  rm -rf {}", path.display())));
    }
    remove_path(path).with_context(|| format!("removing {}", path.display()))?;
    Ok(size)
}

fn run_command(mut cmd: Command, dry_run: bool, emit: &mut impl FnMut(CleanEvent)) -> Result<()> {
    let display = format!(
        "{} {}",
        cmd.get_program().to_string_lossy(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    );
    if dry_run {
        emit(CleanEvent::Log(format!("  [dry-run] would run: {display}")));
        return Ok(());
    }
    emit(CleanEvent::Log(format!("  $ {display}")));
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .with_context(|| format!("starting `{display}`"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let err_thread = stderr.map(|e| {
        std::thread::spawn(move || {
            BufReader::new(e)
                .lines()
                .map_while(Result::ok)
                .collect::<Vec<_>>()
        })
    });
    if let Some(out) = stdout {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            emit(CleanEvent::Log(format!("    {line}")));
        }
    }
    let err_lines = err_thread.and_then(|t| t.join().ok()).unwrap_or_default();
    for line in &err_lines {
        emit(CleanEvent::Log(format!("    {line}")));
    }
    let status = child.wait()?;
    if !status.success() {
        let tail = err_lines.last().cloned().unwrap_or_default();
        anyhow::bail!(
            "`{display}` exited with {status}{}",
            if tail.is_empty() {
                String::new()
            } else {
                format!(": {tail}")
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::candidate::{Category, Risk};
    use std::fs;

    fn opts(home: &Path, dry_run: bool) -> CleanOptions {
        CleanOptions {
            dry_run,
            use_trash: false,
            home: home.to_path_buf(),
            protected: vec![home.join("Dropbox")],
        }
    }

    #[test]
    fn removes_directory_and_records_history() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let target = home.join("proj").join("node_modules");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("x"), vec![1u8; 2048]).unwrap();
        let c = Candidate::new(
            "node_modules",
            "test",
            Category::Project,
            Risk::Low,
            Action::RemovePath {
                path: target.clone(),
            },
        )
        .with_size(2048, true);
        let mut log = Vec::new();
        let entry = run(std::slice::from_ref(&c), &opts(&home, true), |e| {
            log.push(e)
        });
        assert!(target.exists(), "dry run must not delete");
        assert!(entry.dry_run);
        assert_eq!(entry.freed, 2048);
        let entry = run(&[c], &opts(&home, false), |_| {});
        assert!(!target.exists());
        assert_eq!(entry.freed, 2048);
        assert!(entry.items[0].ok);
    }

    #[test]
    fn refuses_protected_paths() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let target = home.join("Dropbox").join("cache");
        fs::create_dir_all(&target).unwrap();
        let c = Candidate::new(
            "cache",
            "test",
            Category::Temp,
            Risk::Low,
            Action::RemovePath {
                path: target.clone(),
            },
        );
        let entry = run(&[c], &opts(&home, false), |_| {});
        assert!(target.exists());
        assert!(!entry.items[0].ok);
        assert!(entry.items[0]
            .error
            .as_deref()
            .unwrap()
            .contains("protected"));
    }

    #[test]
    fn failing_command_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let c = Candidate::new(
            "nope",
            "test",
            Category::Container,
            Risk::Low,
            Action::RunCommand {
                program: "definitely-not-a-real-program-xyz".into(),
                args: vec![],
                cwd: None,
            },
        );
        let entry = run(&[c], &opts(&home, false), |_| {});
        assert!(!entry.items[0].ok);
    }
}
