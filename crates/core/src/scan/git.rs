//! Git repository maintenance: prunable worktree records, stale clean worktrees,
//! merged local branches and loose-object garbage collection.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone};

use crate::core::candidate::{Action, Candidate, Category, Risk};
use crate::core::fsutil::{dir_size, modified_at};
use crate::core::state::GitConfig;

use super::tools::run_with_timeout;

const GIT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: Option<String>,
}

/// Parse `git worktree list --porcelain` output.
pub fn parse_worktree_list(output: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    let mut cur: Option<Worktree> = None;
    for line in output.lines() {
        if line.trim().is_empty() {
            if let Some(w) = cur.take() {
                out.push(w);
            }
            continue;
        }
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(w) = cur.take() {
                out.push(w);
            }
            cur = Some(Worktree {
                path: PathBuf::from(p),
                ..Default::default()
            });
            continue;
        }
        let Some(w) = cur.as_mut() else { continue };
        if let Some(h) = line.strip_prefix("HEAD ") {
            w.head = h.to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            w.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        } else if line == "bare" {
            w.bare = true;
        } else if line == "detached" {
            w.detached = true;
        } else if line.starts_with("locked") {
            w.locked = true;
        } else if let Some(p) = line.strip_prefix("prunable") {
            w.prunable = Some(p.trim_start_matches(' ').to_string());
        }
    }
    if let Some(w) = cur.take() {
        out.push(w);
    }
    out
}

/// Parse `git count-objects -v` → (loose count, loose size bytes, garbage bytes).
pub fn parse_count_objects(output: &str) -> (u64, u64, u64) {
    let mut count = 0;
    let mut size = 0;
    let mut garbage = 0;
    for line in output.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v: u64 = v.trim().parse().unwrap_or(0);
        match k.trim() {
            "count" => count = v,
            "size" => size = v * 1024,
            "size-garbage" => garbage = v * 1024,
            _ => {}
        }
    }
    (count, size, garbage)
}

fn git_output(repo: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("GIT_OPTIONAL_LOCKS", "0");
    let out = run_with_timeout(cmd, GIT_TIMEOUT)?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Determine the default branch (`main`, `master` or origin/HEAD target).
pub fn default_branch(repo: &Path) -> Option<String> {
    if let Some(s) = git_output(
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        let s = s.trim();
        if let Some(b) = s.strip_prefix("origin/") {
            return Some(b.to_string());
        }
    }
    for b in ["main", "master", "develop", "trunk"] {
        if git_output(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{b}"),
            ],
        )
        .is_some()
        {
            return Some(b.to_string());
        }
    }
    None
}

fn last_commit_time(path: &Path) -> Option<DateTime<Local>> {
    let s = git_output(path, &["log", "-1", "--format=%ct"])?;
    let ts: i64 = s.trim().parse().ok()?;
    Local.timestamp_opt(ts, 0).single()
}

fn is_clean(worktree: &Path) -> bool {
    git_output(
        worktree,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .map(|s| s.trim().is_empty())
    .unwrap_or(false)
}

/// Inspect one repository and return git maintenance candidates.
pub fn analyze_repo(repo: &Path, cfg: &GitConfig) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Some(list) = git_output(repo, &["worktree", "list", "--porcelain"]) else {
        return out;
    };
    let worktrees = parse_worktree_list(&list);
    let default = default_branch(repo);
    let merged: HashSet<String> = default
        .as_deref()
        .and_then(|d| {
            git_output(
                repo,
                &["branch", "--merged", d, "--format=%(refname:short)"],
            )
        })
        .map(|s| {
            s.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let repo_name = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo.display().to_string());

    let prunable: Vec<&Worktree> = worktrees.iter().filter(|w| w.prunable.is_some()).collect();
    if !prunable.is_empty() {
        let names: Vec<String> = prunable
            .iter()
            .map(|w| {
                w.path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect();
        out.push(
            Candidate::new(
                format!("{repo_name} · prune {} worktree record(s)", prunable.len()),
                format!(
                    "Worktree directories that no longer exist: {}. `git worktree prune` removes the dangling metadata.",
                    names.join(", ")
                ),
                Category::Git,
                Risk::Low,
                Action::GitWorktreePrune { repo: repo.to_path_buf() },
            )
            .with_size(0, false)
            .with_tag("worktree"),
        );
    }

    let mut checked_out: HashSet<String> = HashSet::new();
    for (i, w) in worktrees.iter().enumerate() {
        if let Some(b) = &w.branch {
            checked_out.insert(b.clone());
        }
        // The first entry is the main worktree; never offer it.
        if i == 0 || w.bare || w.locked || w.prunable.is_some() || !w.path.is_dir() {
            continue;
        }
        if !is_clean(&w.path) {
            continue;
        }
        let newest = [last_commit_time(&w.path), modified_at(&w.path.join(".git"))]
            .into_iter()
            .flatten()
            .max();
        let idle_days = newest
            .map(|t| Local::now().signed_duration_since(t).num_days().max(0) as u64)
            .unwrap_or(0);
        if idle_days < cfg.worktree_idle_days {
            continue;
        }
        let is_merged = w
            .branch
            .as_ref()
            .map(|b| merged.contains(b))
            .unwrap_or(w.detached);
        let branch_label = w.branch.clone().unwrap_or_else(|| "detached HEAD".into());
        let (risk, note) = if is_merged {
            (Risk::Medium, "Branch is merged into the default branch.")
        } else {
            (
                Risk::High,
                "Branch is NOT merged; commits stay in the repository but the checkout is removed.",
            )
        };
        let size = dir_size(&w.path);
        let mut c = Candidate::new(
            format!(
                "{repo_name} · worktree {}",
                w.path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
            format!("Clean worktree on `{branch_label}`, idle {idle_days} days. {note}"),
            Category::Git,
            risk,
            Action::GitWorktreeRemove {
                repo: repo.to_path_buf(),
                worktree: w.path.clone(),
            },
        )
        .with_size(size, true)
        .with_modified(newest)
        .with_tag("worktree");
        if is_merged {
            c = c.with_tag("merged");
        }
        out.push(c);
    }

    if cfg.merged_branches {
        let current =
            git_output(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string());
        for b in &merged {
            if Some(b) == default.as_ref() || Some(b) == current.as_ref() || checked_out.contains(b)
            {
                continue;
            }
            if b.starts_with('(') {
                continue;
            }
            out.push(
                Candidate::new(
                    format!("{repo_name} · branch {b}"),
                    format!("Local branch `{b}` is fully merged into `{}`. Deleted with `git branch -d` (safe delete).", default.clone().unwrap_or_default()),
                    Category::Git,
                    Risk::Medium,
                    Action::GitBranchDelete { repo: repo.to_path_buf(), branch: b.clone() },
                )
                .with_size(0, true)
                .with_tag("branch"),
            );
        }
    }

    if cfg.gc {
        if let Some(s) = git_output(repo, &["count-objects", "-v"]) {
            let (count, size, garbage) = parse_count_objects(&s);
            if count >= 2000 || size >= 64 * 1024 * 1024 || garbage > 0 {
                out.push(
                    Candidate::new(
                        format!("{repo_name} · git gc"),
                        format!("{count} loose objects ({}) and {} garbage. `git gc --prune=now` repacks them; typical savings 50-90% of loose size.", crate::core::fsutil::human_size(size), crate::core::fsutil::human_size(garbage)),
                        Category::Git,
                        Risk::Low,
                        Action::GitGc { repo: repo.to_path_buf() },
                    )
                    .with_size(size / 2 + garbage, false)
                    .with_tag("gc"),
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_worktrees() {
        let text = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /repo-wt\nHEAD def\nbranch refs/heads/feat\nlocked\n\nworktree /gone\nHEAD 123\ndetached\nprunable gitdir file points to non-existent location\n";
        let wts = parse_worktree_list(text);
        assert_eq!(wts.len(), 3);
        assert_eq!(wts[0].branch.as_deref(), Some("main"));
        assert!(wts[1].locked);
        assert!(wts[2].detached);
        assert!(wts[2].prunable.as_deref().unwrap().contains("non-existent"));
    }

    #[test]
    fn parses_count_objects() {
        let (c, s, g) =
            parse_count_objects("count: 1500\nsize: 2048\nin-pack: 10\nsize-garbage: 4\n");
        assert_eq!(c, 1500);
        assert_eq!(s, 2048 * 1024);
        assert_eq!(g, 4096);
    }

    #[test]
    fn analyzes_real_repo_with_prunable_worktree() {
        if which::which("git").is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let st = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap();
            assert!(
                st.status.success(),
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&st.stderr)
            );
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a"), "1").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        git(&["branch", "merged-feature"]);
        let wt = tmp.path().join("wt");
        git(&[
            "worktree",
            "add",
            "-q",
            wt.to_str().unwrap(),
            "merged-feature",
        ]);
        std::fs::remove_dir_all(&wt).unwrap();
        let cfg = GitConfig {
            enabled: true,
            merged_branches: true,
            gc: true,
            worktree_idle_days: 0,
        };
        let cands = analyze_repo(&repo, &cfg);
        assert!(
            cands
                .iter()
                .any(|c| matches!(c.action, Action::GitWorktreePrune { .. })),
            "{cands:?}"
        );
        // merged-feature is checked out in the (missing) worktree → not offered as branch delete
        assert!(!cands.iter().any(|c| matches!(&c.action, Action::GitBranchDelete { branch, .. } if branch == "merged-feature")));
        git(&["worktree", "prune"]);
        let cands = analyze_repo(&repo, &cfg);
        assert!(cands.iter().any(|c| matches!(&c.action, Action::GitBranchDelete { branch, .. } if branch == "merged-feature")), "{cands:?}");
    }
}
