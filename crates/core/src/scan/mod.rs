//! Scan orchestration: global rules + project walk + git + tools + extras, in
//! parallel, reporting progress through a channel.

pub mod extras;
pub mod git;
pub mod projects;
pub mod rules;
pub mod tools;
pub mod tree;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::candidate::{Action, Candidate};
use crate::core::disk::{disk_for, external_roots, volume_for, volumes, DiskInfo, Volume};
use crate::core::fsutil::{
    compact_path, default_protected_roots, expand_home, is_within, measure, modified_at,
};
use crate::core::process::{running_app, running_process_names};
use crate::core::state::Config;

/// Everything the scanner needs to know about the machine.
#[derive(Clone, Debug)]
pub struct ScanContext {
    pub home: PathBuf,
    pub config: Config,
    pub protected: Vec<PathBuf>,
    pub roots: Vec<PathBuf>,
    /// Mounted volumes (system first).
    pub volumes: Vec<Volume>,
    /// Paths that are skipped because they belong to this program.
    pub skip: Vec<PathBuf>,
    pub include_git: bool,
    pub include_tools: bool,
    pub include_projects: bool,
    pub include_global: bool,
    pub include_extras: bool,
}

impl ScanContext {
    pub fn new(home: PathBuf, config: Config) -> Self {
        let mut protected = default_protected_roots(&home);
        for p in &config.scan.protected {
            protected.push(expand_home(p, &home));
        }
        let vols = volumes(&home);
        let mut roots: Vec<PathBuf> = Vec::new();
        if config.scan.default_roots {
            roots.extend(default_roots(&home));
        }
        for r in &config.scan.roots {
            roots.push(expand_home(r, &home));
        }
        if config.scan.volumes {
            let skip: Vec<PathBuf> = config
                .scan
                .skip_volumes
                .iter()
                .map(|s| expand_home(s, &home))
                .collect();
            for mount in external_roots(&vols) {
                if !skip.iter().any(|s| s == &mount) && !roots.iter().any(|r| is_within(r, &mount))
                {
                    roots.push(mount);
                }
            }
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
            include_extras: true,
            home,
            config,
            protected,
            roots,
            volumes: vols,
            skip,
        }
    }

    pub fn min_size(&self) -> u64 {
        self.config.scan.min_size_mb * 1024 * 1024
    }

    fn volume_name(&self, path: &Path) -> Option<String> {
        volume_for(path, &self.volumes).map(|v| v.mount.display().to_string())
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
        "Desktop",
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
    #[serde(default)]
    pub volumes: Vec<Volume>,
    pub roots: Vec<PathBuf>,
    pub protected: Vec<PathBuf>,
    pub projects: usize,
    pub repos: usize,
    /// Artifact directories skipped because git tracks files inside them or they hold keys.
    #[serde(default)]
    pub guarded: usize,
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
    let guarded = std::sync::atomic::AtomicUsize::new(0);
    let running = if ctx.config.clean.block_running_apps {
        running_process_names()
    } else {
        HashSet::new()
    };
    let emit = |mut c: Candidate| {
        if c.volume.is_none() {
            if let Some(p) = c.action.path() {
                c.volume = ctx.volume_name(p);
            }
        }
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
                    let m = measure(&path);
                    if m.allocated == 0 {
                        return;
                    }
                    let (owner, app) = rules::owner_of(rule.label);
                    let blocked = running_app(app, &running);
                    let c = Candidate::new(
                        rule.label,
                        rule.description,
                        rule.category,
                        rule.risk,
                        Action::RemovePath { path: path.clone() },
                    )
                    .with_measure(m)
                    .with_modified(modified_at(&path))
                    .with_owner(if owner.is_empty() {
                        rule.label.split(' ').next().unwrap_or("")
                    } else {
                        owner
                    })
                    .with_blocked_by(blocked);
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
        if ctx.include_extras {
            s.spawn(|_| {
                progress(
                    "extras",
                    "installers, superseded versions, volume trash".into(),
                );
                for c in extras::installers(&ctx.home, ctx.config.scan.installer_days) {
                    emit(c);
                }
                for c in extras::superseded_versions(&ctx.home) {
                    emit(c);
                }
                for c in extras::volume_extras(&ctx.volumes) {
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
                            progress("projects", compact_path(dir, &ctx.home));
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
                let git_available = which::which("git").is_ok();
                hits.par_iter().for_each(|hit| {
                    let m = measure(&hit.path);
                    if m.has_git || m.has_secrets {
                        guarded.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        return;
                    }
                    if git_available {
                        if let Some(repo) = projects::find_git_root(&hit.project.root) {
                            if git::has_tracked_files(&repo, &hit.path) {
                                guarded.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                return;
                            }
                        }
                    }
                    let c =
                        projects::hit_to_candidate(hit, ctx.config.scan.stale_days).with_measure(m);
                    emit(c);
                });
                if ctx.include_git && git_available {
                    let repos: Vec<&projects::Project> =
                        projects.iter().filter(|p| p.is_git_repo).collect();
                    repos_count = repos.len();
                    progress("git", format!("inspecting {} repositories", repos.len()));
                    repos.par_iter().for_each(|p| {
                        for c in git::analyze_repo(&p.root, &ctx.config.git) {
                            emit(c.with_owner(p.name.clone()));
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
        volumes: ctx.volumes.clone(),
        roots: ctx.roots.clone(),
        protected: ctx.protected.clone(),
        projects: projects_count,
        repos: repos_count,
        guarded: guarded.load(std::sync::atomic::Ordering::Relaxed),
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

/// Persist the last scan so front ends can show it instantly on the next launch.
pub fn save_cache(path: &Path, result: &ScanResult) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(result)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Load a cached scan, dropping candidates whose path no longer exists.
pub fn load_cache(path: &Path) -> Option<ScanResult> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut r: ScanResult = serde_json::from_str(&text).ok()?;
    r.candidates.retain(|c| match &c.action {
        Action::RemovePath { path } => path.exists(),
        Action::GitWorktreeRemove { worktree, .. } => worktree.exists(),
        _ => true,
    });
    Some(r)
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
            vec![1u8; 2 * 1024 * 1024],
        )
        .unwrap();
        // a committed-looking vendor dir with a nested .git must be guarded
        fs::create_dir_all(proj.join("vendor/.git")).unwrap();
        fs::write(proj.join("composer.json"), "{}").unwrap();
        fs::write(proj.join("composer.lock"), "{}").unwrap();
        fs::write(proj.join("vendor/big.bin"), vec![1u8; 2 * 1024 * 1024]).unwrap();
        let mut cfg = Config::default();
        cfg.scan.default_roots = true;
        cfg.scan.volumes = false;
        let mut ctx = ScanContext::new(home.clone(), cfg);
        ctx.include_global = false;
        ctx.include_tools = false;
        ctx.include_git = false;
        ctx.include_extras = false;
        let res = scan_blocking(&ctx);
        assert_eq!(res.projects, 1);
        assert_eq!(res.candidates.len(), 1, "{:?}", res.candidates);
        assert_eq!(res.guarded, 1);
        assert_eq!(res.candidates[0].apparent_size, 2 * 1024 * 1024);
        assert_eq!(res.candidates[0].owner, "app");
        assert!(res.total() > 0);
        let cache = tmp.path().join("cache.json");
        save_cache(&cache, &res).unwrap();
        let back = load_cache(&cache).unwrap();
        assert_eq!(back.candidates.len(), 1);
        fs::remove_dir_all(proj.join("node_modules")).unwrap();
        assert_eq!(load_cache(&cache).unwrap().candidates.len(), 0);
    }
}
