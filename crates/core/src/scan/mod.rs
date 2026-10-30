//! Scan orchestration: global rules + project walk + git + tools, in parallel,
//! reporting progress through a channel.

pub mod git;
pub mod projects;
pub mod rules;
pub mod tools;

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::candidate::{Action, Candidate};
use crate::core::disk::{disk_for, DiskInfo};
use crate::core::fsutil::{default_protected_roots, dir_size, expand_home, is_within, modified_at};
use crate::core::state::Config;

/// Everything the scanner needs to know about the machine.
#[derive(Clone, Debug)]
pub struct ScanContext {
    pub home: PathBuf,
    pub config: Config,
    pub protected: Vec<PathBuf>,
    pub roots: Vec<PathBuf>,
    /// Paths that are skipped because they belong to this program.
    pub skip: Vec<PathBuf>,
    pub include_git: bool,
    pub include_tools: bool,
    pub include_projects: bool,
    pub include_global: bool,
}

impl ScanContext {
    pub fn new(home: PathBuf, config: Config) -> Self {
        let mut protected = default_protected_roots(&home);
        for p in &config.scan.protected {
            protected.push(expand_home(p, &home));
        }
        let mut roots: Vec<PathBuf> = Vec::new();
        if config.scan.default_roots {
            roots.extend(default_roots(&home));
        }
        for r in &config.scan.roots {
            roots.push(expand_home(r, &home));
        }
        let mut skip = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                skip.push(dir.to_path_buf());
            }
        }
        ScanContext {
            include_git: config.git.enabled,
            include_tools: true,
            include_projects: true,
            include_global: true,
            home,
            config,
            protected,
            roots,
            skip,
        }
    }

    pub fn min_size(&self) -> u64 {
        self.config.scan.min_size_mb * 1024 * 1024
    }
}

/// Built-in locations where developers keep code.
pub fn default_roots(home: &Path) -> Vec<PathBuf> {
    [
        "Documents",
        "Projects",
        "projects",
        "Developer",
        "dev",
        "Dev",
        "code",
        "Code",
        "src",
        "work",
        "Work",
        "workspace",
        "Workspace",
        "repos",
        "Repos",
        "git",
        "github",
        "GitHub",
        "Sites",
        "www",
        "go/src",
    ]
    .iter()
    .map(|r| home.join(r))
    .filter(|p| p.is_dir())
    .collect()
}

#[derive(Clone, Debug)]
pub enum ScanEvent {
    /// Short status line for the UI.
    Progress {
        phase: &'static str,
        message: String,
    },
    /// A candidate that passed the size filter.
    Found(Box<Candidate>),
    Done(Box<ScanResult>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub candidates: Vec<Candidate>,
    pub disk: DiskInfo,
    pub roots: Vec<PathBuf>,
    pub protected: Vec<PathBuf>,
    pub projects: usize,
    pub repos: usize,
    pub started_at: DateTime<Local>,
    pub duration_ms: u128,
}

impl ScanResult {
    pub fn total(&self) -> u64 {
        self.candidates.iter().map(|c| c.size).sum()
    }
}

/// Run a scan, emitting events. Blocks until done.
pub fn scan(ctx: &ScanContext, tx: Sender<ScanEvent>) -> ScanResult {
    let started = Instant::now();
    let started_at = Local::now();
    let min_size = ctx.min_size();
    let found: Mutex<Vec<Candidate>> = Mutex::new(Vec::new());
    let emit = |c: Candidate| {
        if c.size >= min_size || !c.size_exact || c.action.is_git() {
            let _ = tx.send(ScanEvent::Found(Box::new(c.clone())));
            found.lock().unwrap().push(c);
        }
    };
    let progress = |phase: &'static str, message: String| {
        let _ = tx.send(ScanEvent::Progress { phase, message });
    };

    progress("disk", "reading disk usage".into());
    let disk = disk_for(&ctx.home);

    let mut projects_count = 0usize;
    let mut repos_count = 0usize;

    rayon::scope(|s| {
        if ctx.include_global {
            s.spawn(|_| {
                progress("global", "checking known cache locations".into());
                let disabled = &ctx.config.scan.disabled_rules;
                let rules: Vec<_> = rules::rules_for_current_os()
                    .filter(|r| {
                        !disabled.iter().any(|d| {
                            r.label
                                .to_ascii_lowercase()
                                .contains(&d.to_ascii_lowercase())
                        })
                    })
                    .collect();
                rules.par_iter().for_each(|rule| {
                    let Some(path) = rules::resolve_template(rule.path, &ctx.home) else {
                        return;
                    };
                    if !path.exists() || ctx.protected.iter().any(|p| is_within(&path, p)) {
                        return;
                    }
                    let size = dir_size(&path);
                    if size == 0 {
                        return;
                    }
                    let c = Candidate::new(
                        rule.label,
                        rule.description,
                        rule.category,
                        rule.risk,
                        Action::RemovePath { path: path.clone() },
                    )
                    .with_size(size, true)
                    .with_modified(modified_at(&path));
                    emit(c);
                });
            });
        }
        if ctx.include_tools {
            s.spawn(|_| {
                progress("tools", "asking docker, pnpm, simctl".into());
                for c in tools::tool_candidates() {
                    emit(c);
                }
            });
        }
        if ctx.include_projects {
            s.spawn(|_| {
                let mut last = Instant::now();
                let (projects, hits) = projects::walk(
                    &projects::WalkOptions {
                        roots: &ctx.roots,
                        max_depth: ctx.config.scan.max_depth,
                        protected: &ctx.protected,
                        skip: &ctx.skip,
                    },
                    |dir| {
                        if last.elapsed() > Duration::from_millis(80) {
                            last = Instant::now();
                            progress(
                                "projects",
                                crate::core::fsutil::compact_path(dir, &ctx.home),
                            );
                        }
                    },
                );
                projects_count = projects.len();
                progress(
                    "projects",
                    format!(
                        "{} projects, sizing {} artifacts",
                        projects.len(),
                        hits.len()
                    ),
                );
                hits.par_iter().for_each(|hit| {
                    let size = dir_size(&hit.path);
                    let c = projects::hit_to_candidate(hit, ctx.config.scan.stale_days)
                        .with_size(size, true);
                    emit(c);
                });
                if ctx.include_git && which::which("git").is_ok() {
                    let repos: Vec<&projects::Project> =
                        projects.iter().filter(|p| p.is_git_repo).collect();
                    repos_count = repos.len();
                    progress("git", format!("inspecting {} repositories", repos.len()));
                    repos.par_iter().for_each(|p| {
                        for c in git::analyze_repo(&p.root, &ctx.config.git) {
                            emit(c);
                        }
                    });
                }
            });
        }
    });

    let mut candidates = found.into_inner().unwrap();
    candidates.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.label.cmp(&b.label)));
    candidates.dedup_by(|a, b| a.id == b.id);
    let result = ScanResult {
        candidates,
        disk,
        roots: ctx.roots.clone(),
        protected: ctx.protected.clone(),
        projects: projects_count,
        repos: repos_count,
        started_at,
        duration_ms: started.elapsed().as_millis(),
    };
    let _ = tx.send(ScanEvent::Done(Box::new(result.clone())));
    result
}

/// Convenience: run a scan synchronously without progress.
pub fn scan_blocking(ctx: &ScanContext) -> ScanResult {
    let (tx, rx) = std::sync::mpsc::channel();
    let result = scan(ctx, tx);
    drop(rx);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scan_finds_project_artifacts_in_temp_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let proj = home.join("Projects/app");
        fs::create_dir_all(proj.join("node_modules/pkg")).unwrap();
        fs::write(proj.join("package.json"), "{}").unwrap();
        fs::write(
            proj.join("node_modules/pkg/big.bin"),
            vec![0u8; 2 * 1024 * 1024],
        )
        .unwrap();
        let mut cfg = Config::default();
        cfg.scan.default_roots = true;
        let mut ctx = ScanContext::new(home.clone(), cfg);
        ctx.include_global = false;
        ctx.include_tools = false;
        ctx.include_git = false;
        let res = scan_blocking(&ctx);
        assert_eq!(res.projects, 1);
        assert_eq!(res.candidates.len(), 1, "{:?}", res.candidates);
        assert_eq!(res.candidates[0].size, 2 * 1024 * 1024);
        assert!(res.total() > 0);
    }
}
