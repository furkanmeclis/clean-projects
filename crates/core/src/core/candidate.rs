use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// How dangerous it is to remove a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Fully regenerable (build output, download caches).
    Low,
    /// Regenerable but costly (package stores, device symbols, merged worktrees).
    Medium,
    /// May reset application state or lose local-only work. Close the app first.
    High,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
        }
    }

    pub fn parse(s: &str) -> Option<Risk> {
        match s.to_ascii_lowercase().as_str() {
            "low" | "l" => Some(Risk::Low),
            "medium" | "med" | "m" => Some(Risk::Medium),
            "high" | "h" => Some(Risk::High),
            _ => None,
        }
    }
}

/// Broad grouping used for filtering and charts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    /// Build output / dependencies inside a project directory.
    Project,
    /// Global package manager stores (npm, cargo registry, pip, gradle...).
    PackageCache,
    /// Compiler and tool build caches (go-build, DerivedData, sccache...).
    BuildCache,
    /// Editor and IDE caches.
    Ide,
    /// Browser caches.
    Browser,
    /// Docker / Podman and other container engines.
    Container,
    /// Mobile SDKs, simulators, emulators.
    Mobile,
    /// Git repository maintenance (worktrees, merged branches, gc).
    Git,
    /// Application state that is regenerable but may reset the app.
    AppData,
    /// Trash bins and temp folders.
    Temp,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Project,
        Category::PackageCache,
        Category::BuildCache,
        Category::Ide,
        Category::Browser,
        Category::Container,
        Category::Mobile,
        Category::Git,
        Category::AppData,
        Category::Temp,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Project => "project",
            Category::PackageCache => "package-cache",
            Category::BuildCache => "build-cache",
            Category::Ide => "ide",
            Category::Browser => "browser",
            Category::Container => "container",
            Category::Mobile => "mobile",
            Category::Git => "git",
            Category::AppData => "app-data",
            Category::Temp => "temp",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Category::Project => "Project",
            Category::PackageCache => "Packages",
            Category::BuildCache => "Build",
            Category::Ide => "IDE",
            Category::Browser => "Browser",
            Category::Container => "Container",
            Category::Mobile => "Mobile",
            Category::Git => "Git",
            Category::AppData => "App data",
            Category::Temp => "Temp",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        let s = s.to_ascii_lowercase();
        Category::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == s || c.short().to_ascii_lowercase() == s)
    }
}

/// Which ecosystem a project belongs to (drives artifact rules).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectKind {
    Node,
    Rust,
    Go,
    Python,
    Php,
    Gradle,
    Maven,
    Swift,
    Dotnet,
    Flutter,
    Ruby,
    Elixir,
    Cmake,
    Unity,
    Generic,
}

impl ProjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectKind::Node => "node",
            ProjectKind::Rust => "rust",
            ProjectKind::Go => "go",
            ProjectKind::Python => "python",
            ProjectKind::Php => "php",
            ProjectKind::Gradle => "gradle",
            ProjectKind::Maven => "maven",
            ProjectKind::Swift => "swift",
            ProjectKind::Dotnet => "dotnet",
            ProjectKind::Flutter => "flutter",
            ProjectKind::Ruby => "ruby",
            ProjectKind::Elixir => "elixir",
            ProjectKind::Cmake => "cmake",
            ProjectKind::Unity => "unity",
            ProjectKind::Generic => "generic",
        }
    }
}

/// What the cleaner has to do to reclaim the space.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Action {
    /// Delete a file or directory tree (to trash when enabled).
    RemovePath { path: PathBuf },
    /// Run an external program.
    RunCommand {
        program: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
    },
    /// `git worktree prune` inside a repository.
    GitWorktreePrune { repo: PathBuf },
    /// `git worktree remove <path>` for a clean worktree.
    GitWorktreeRemove { repo: PathBuf, worktree: PathBuf },
    /// `git branch -d <branch>` for a branch merged into the default branch.
    GitBranchDelete { repo: PathBuf, branch: String },
    /// `git gc --prune=now`.
    GitGc { repo: PathBuf },
}

impl Action {
    /// Human readable representation (path or command line).
    pub fn display(&self) -> String {
        match self {
            Action::RemovePath { path } => path.display().to_string(),
            Action::RunCommand { program, args, .. } => {
                let mut s = program.clone();
                for a in args {
                    s.push(' ');
                    s.push_str(a);
                }
                s
            }
            Action::GitWorktreePrune { repo } => {
                format!("git -C {} worktree prune", repo.display())
            }
            Action::GitWorktreeRemove { repo, worktree } => format!(
                "git -C {} worktree remove {}",
                repo.display(),
                worktree.display()
            ),
            Action::GitBranchDelete { repo, branch } => {
                format!("git -C {} branch -d {}", repo.display(), branch)
            }
            Action::GitGc { repo } => format!("git -C {} gc --prune=now", repo.display()),
        }
    }

    /// Primary filesystem path this action touches, if any.
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            Action::RemovePath { path } => Some(path),
            Action::GitWorktreeRemove { worktree, .. } => Some(worktree),
            Action::GitWorktreePrune { repo }
            | Action::GitBranchDelete { repo, .. }
            | Action::GitGc { repo } => Some(repo),
            Action::RunCommand { cwd, .. } => cwd.as_ref(),
        }
    }

    pub fn is_git(&self) -> bool {
        !matches!(self, Action::RemovePath { .. } | Action::RunCommand { .. })
    }
}

/// Project context attached to in-project artifacts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub root: PathBuf,
    pub name: String,
    pub kind: ProjectKind,
    /// Days since the project itself was last touched (sources or git activity).
    pub idle_days: u64,
}

/// A single thing that can be cleaned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Stable identifier derived from the action, survives rescans.
    pub id: String,
    pub label: String,
    pub description: String,
    pub category: Category,
    pub risk: Risk,
    /// Bytes that are expected to be reclaimed.
    pub size: u64,
    /// `false` when `size` is an estimate (docker, git gc).
    pub size_exact: bool,
    pub modified: Option<DateTime<Local>>,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl Candidate {
    pub fn new(
        label: impl Into<String>,
        description: impl Into<String>,
        category: Category,
        risk: Risk,
        action: Action,
    ) -> Self {
        let id = stable_id(&action);
        Candidate {
            id,
            label: label.into(),
            description: description.into(),
            category,
            risk,
            size: 0,
            size_exact: true,
            modified: None,
            action,
            project: None,
            tags: Vec::new(),
        }
    }

    pub fn with_size(mut self, size: u64, exact: bool) -> Self {
        self.size = size;
        self.size_exact = exact;
        self
    }

    pub fn with_modified(mut self, modified: Option<DateTime<Local>>) -> Self {
        self.modified = modified;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn detail(&self) -> String {
        self.action.display()
    }

    pub fn is_stale(&self) -> bool {
        self.tags.iter().any(|t| t == "stale")
    }

    /// Days since the candidate itself was modified.
    pub fn age_days(&self) -> Option<u64> {
        self.modified.map(|m| {
            let d = Local::now().signed_duration_since(m);
            d.num_days().max(0) as u64
        })
    }
}

/// Deterministic id for an action (16 hex chars).
pub fn stable_id(action: &Action) -> String {
    let mut h = DefaultHasher::new();
    // DefaultHasher is SipHash with fixed keys — stable across runs of the same build.
    action.display().hash(&mut h);
    match action {
        Action::RemovePath { .. } => 1u8.hash(&mut h),
        Action::RunCommand { .. } => 2u8.hash(&mut h),
        Action::GitWorktreePrune { .. } => 3u8.hash(&mut h),
        Action::GitWorktreeRemove { .. } => 4u8.hash(&mut h),
        Action::GitBranchDelete { .. } => 5u8.hash(&mut h),
        Action::GitGc { .. } => 6u8.hash(&mut h),
    }
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_distinct() {
        let a = Action::RemovePath {
            path: PathBuf::from("/tmp/a"),
        };
        let b = Action::RemovePath {
            path: PathBuf::from("/tmp/b"),
        };
        assert_eq!(stable_id(&a), stable_id(&a));
        assert_ne!(stable_id(&a), stable_id(&b));
        assert_eq!(stable_id(&a).len(), 16);
    }

    #[test]
    fn risk_and_category_parse() {
        assert_eq!(Risk::parse("HIGH"), Some(Risk::High));
        assert_eq!(Risk::parse("nope"), None);
        assert_eq!(
            Category::parse("package-cache"),
            Some(Category::PackageCache)
        );
        assert_eq!(Category::parse("IDE"), Some(Category::Ide));
        assert!(Risk::Low < Risk::High);
    }
}
