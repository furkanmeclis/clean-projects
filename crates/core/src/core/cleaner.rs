//! Executes candidate actions with safety checks, streaming progress events.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::candidate::{Action, Candidate};
use super::disk::Volume;
use super::fsutil::{check_deletable, dir_size, remove_path};
use super::process::{running_app, running_process_names};
use super::recovery::{self, Journal};
use super::state::{HistoryEntry, HistoryItem};

/// How removed files are disposed of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CleanMode {
    /// Move into the recovery bin (same volume rename, undoable).
    #[default]
    Recovery,
    /// Move to the operating system trash.
    Trash,
    /// Delete immediately.
    Permanent,
}

impl CleanMode {
    pub fn parse(s: &str) -> CleanMode {
        match s.to_ascii_lowercase().as_str() {
            "trash" => CleanMode::Trash,
            "permanent" | "delete" => CleanMode::Permanent,
            _ => CleanMode::Recovery,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CleanMode::Recovery => "recovery",
            CleanMode::Trash => "trash",
            CleanMode::Permanent => "permanent",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CleanOptions {
    pub dry_run: bool,
    pub mode: CleanMode,
    pub home: PathBuf,
    pub protected: Vec<PathBuf>,
    /// Where the recovery bin lives for the home volume.
    pub data_dir: PathBuf,
    pub volumes: Vec<Volume>,
    /// Refuse items whose owning application is running.
    pub block_running_apps: bool,
}

impl CleanOptions {
    /// Backwards compatible helper for callers that only know about trash on/off.
    pub fn use_trash(&self) -> bool {
        self.mode != CleanMode::Permanent
    }
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

/// Outcome of running a batch: the history entry and the recovery journal entries created.
pub struct Outcome {
    pub entry: HistoryEntry,
    pub recovered: Vec<recovery::RecoveryEntry>,
}

/// Run every candidate in order, reporting through `emit`.
pub fn run(
    candidates: &[Candidate],
    opts: &CleanOptions,
    mut emit: impl FnMut(CleanEvent),
) -> HistoryEntry {
    run_with_journal(candidates, opts, &mut emit).entry
}

/// Like [`run`] but also returns the recovery entries so the caller can persist the journal.
pub fn run_with_journal(
    candidates: &[Candidate],
    opts: &CleanOptions,
    emit: &mut impl FnMut(CleanEvent),
) -> Outcome {
    let total = candidates.len();
    let mut freed = 0u64;
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut items = Vec::with_capacity(total);
    let mut recovered = Vec::new();
    let run_id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let running = if opts.block_running_apps && !opts.dry_run {
        Some(running_process_names())
    } else {
        None
    };

    for (index, c) in candidates.iter().enumerate() {
        emit(CleanEvent::Start {
            index,
            total,
            id: c.id.clone(),
            label: c.label.clone(),
        });
        let result = match (&running, &c.blocked_by) {
            (Some(names), Some(app)) if running_app(app, names).is_some() => {
                Err(anyhow!("{app} is running; close it first"))
            }
            _ => execute(c, opts, &run_id, &mut recovered, emit),
        };
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
                let recovery_id = recovered
                    .iter()
                    .rev()
                    .find(|r| r.candidate_id == c.id)
                    .map(|r| r.id.clone());
                items.push(HistoryItem {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    detail: c.detail(),
                    size: bytes,
                    ok: true,
                    error: None,
                    recovery_id,
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
                    recovery_id: None,
                });
            }
        }
    }
    emit(CleanEvent::Done { freed, ok, failed });
    Outcome {
        entry: HistoryEntry {
            at: chrono::Local::now(),
            freed,
            dry_run: opts.dry_run,
            trashed: opts.mode != CleanMode::Permanent,
            mode: opts.mode.as_str().to_string(),
            items,
        },
        recovered,
    }
}

/// Persist recovery entries into the journal file.
pub fn record_recovered(journal: &mut Journal, recovered: Vec<recovery::RecoveryEntry>) {
    journal.entries.extend(recovered);
}

fn execute(
    c: &Candidate,
    opts: &CleanOptions,
    run_id: &str,
    recovered: &mut Vec<recovery::RecoveryEntry>,
    emit: &mut impl FnMut(CleanEvent),
) -> Result<u64> {
    match &c.action {
        Action::RemovePath { path } => remove(c, path, opts, run_id, recovered, emit),
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
    c: &Candidate,
    path: &Path,
    opts: &CleanOptions,
    run_id: &str,
    recovered: &mut Vec<recovery::RecoveryEntry>,
    emit: &mut impl FnMut(CleanEvent),
) -> Result<u64> {
    check_deletable(path, &opts.home, &opts.protected)
        .map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let size = if c.size > 0 { c.size } else { dir_size(path) };
    if opts.dry_run {
        emit(CleanEvent::Log(format!(
            "  [dry-run] would remove {} ({})",
            path.display(),
            opts.mode.as_str()
        )));
        return Ok(size);
    }
    let mut mode = opts.mode;
    if mode == CleanMode::Recovery {
        let bin = recovery::bin_dir_for(path, &opts.home, &opts.data_dir, &opts.volumes);
        emit(CleanEvent::Log(format!("  recover → {}", bin.display())));
        match recovery::move_to_bin(path, &bin, run_id, &c.id, &c.label, size) {
            Ok(entry) => {
                recovered.push(entry);
                return Ok(size);
            }
            Err(e) => {
                emit(CleanEvent::Log(format!(
                    "  recovery bin unavailable ({}), using trash",
                    e.0
                )));
                mode = CleanMode::Trash;
            }
        }
    }
    if mode == CleanMode::Trash {
        emit(CleanEvent::Log(format!("  trash {}", path.display())));
        match trash::delete(path) {
            Ok(()) => return Ok(size),
            Err(e) => emit(CleanEvent::Log(format!(
                "  trash unavailable ({e}), deleting permanently"
            ))),
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

/// Restore every recoverable item of a history entry. Returns (restored, failed messages).
pub fn undo(entry: &HistoryEntry, journal: &mut Journal) -> (usize, Vec<String>) {
    let mut restored = 0;
    let mut errors = Vec::new();
    let ids: Vec<String> = entry
        .items
        .iter()
        .filter_map(|i| i.recovery_id.clone())
        .collect();
    for id in ids {
        let Some(pos) = journal.entries.iter().position(|e| e.id == id) else {
            errors.push(format!("{id}: not in recovery bin"));
            continue;
        };
        match recovery::restore(&journal.entries[pos]) {
            Ok(()) => {
                journal.entries.remove(pos);
                restored += 1;
            }
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    (restored, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::candidate::{Category, Risk};
    use std::fs;

    fn opts(home: &Path, dry_run: bool, mode: CleanMode) -> CleanOptions {
        CleanOptions {
            dry_run,
            mode,
            home: home.to_path_buf(),
            protected: vec![home.join("Dropbox")],
            data_dir: home.join(".local/share/cleanprojects"),
            volumes: vec![],
            block_running_apps: false,
        }
    }

    fn cand(path: &Path) -> Candidate {
        Candidate::new(
            "node_modules",
            "test",
            Category::Project,
            Risk::Low,
            Action::RemovePath {
                path: path.to_path_buf(),
            },
        )
        .with_size(2048, true)
    }

    #[test]
    fn removes_directory_permanently_and_records_history() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let target = home.join("proj").join("node_modules");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("x"), vec![1u8; 2048]).unwrap();
        let c = cand(&target);
        let mut log = Vec::new();
        let entry = run(
            std::slice::from_ref(&c),
            &opts(&home, true, CleanMode::Permanent),
            |e| log.push(e),
        );
        assert!(target.exists(), "dry run must not delete");
        assert!(entry.dry_run);
        assert_eq!(entry.freed, 2048);
        let entry = run(&[c], &opts(&home, false, CleanMode::Permanent), |_| {});
        assert!(!target.exists());
        assert_eq!(entry.freed, 2048);
        assert!(entry.items[0].ok);
        assert_eq!(entry.mode, "permanent");
    }

    #[test]
    fn recovery_mode_is_undoable() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let target = home.join("proj").join("node_modules");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("x"), b"1").unwrap();
        let o = opts(&home, false, CleanMode::Recovery);
        let out = run_with_journal(&[cand(&target)], &o, &mut |_| {});
        assert!(!target.exists());
        assert_eq!(out.recovered.len(), 1);
        assert!(out.recovered[0].stored.starts_with(&o.data_dir));
        assert!(out.entry.items[0].recovery_id.is_some());
        let mut journal = Journal::default();
        record_recovered(&mut journal, out.recovered);
        let (restored, errors) = undo(&out.entry, &mut journal);
        assert_eq!((restored, errors.len()), (1, 0));
        assert!(target.join("x").exists());
        assert!(journal.entries.is_empty());
    }

    #[test]
    fn refuses_protected_paths() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let target = home.join("Dropbox").join("cache");
        fs::create_dir_all(&target).unwrap();
        let entry = run(
            &[cand(&target)],
            &opts(&home, false, CleanMode::Permanent),
            |_| {},
        );
        assert!(target.exists());
        assert!(!entry.items[0].ok);
        assert!(entry.items[0]
            .error
            .as_deref()
            .unwrap()
            .contains("protected"));
    }

    #[test]
    fn refuses_when_owner_app_is_running() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let target = home.join("cache");
        fs::create_dir_all(&target).unwrap();
        // Any process reported by the OS is, by definition, running.
        let names = running_process_names();
        let Some(any) = names.iter().next().cloned() else {
            return;
        };
        let c = cand(&target).with_blocked_by(Some(any));
        let mut o = opts(&home, false, CleanMode::Permanent);
        o.block_running_apps = true;
        let entry = run(&[c], &o, |_| {});
        assert!(target.exists());
        assert!(entry.items[0].error.as_deref().unwrap().contains("running"));
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
        let entry = run(&[c], &opts(&home, false, CleanMode::Permanent), |_| {});
        assert!(!entry.items[0].ok);
    }
}
