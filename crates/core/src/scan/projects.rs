//! Walk project roots, detect project ecosystems and their regenerable artifacts.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::core::candidate::{Action, Candidate, Category, ProjectInfo, ProjectKind, Risk};
use crate::core::fsutil::{is_within, modified_at, newest_modified};

/// A directory recognised as a project.
#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub kinds: Vec<ProjectKind>,
    pub is_git_repo: bool,
    pub idle_days: u64,
}

/// An artifact directory found inside a project (size not computed yet).
#[derive(Clone, Debug)]
pub struct ArtifactHit {
    pub project: Project,
    pub path: PathBuf,
    pub label: String,
    pub description: &'static str,
    pub risk: Risk,
}

/// Directory names never descended into, regardless of project type.
const NEVER_DESCEND: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "Backups",
    ".Trash",
    "Library",
    ".cache",
    ".npm",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    ".pub-cache",
    ".nuget",
    ".vscode-server",
    ".local",
    "Applications",
    "Movies",
    "Music",
    "Pictures",
    "Photos",
    ".venv",
    "venv",
    "target",
    "__pycache__",
    "vendor",
    "Pods",
    "DerivedData",
    ".build",
    ".dart_tool",
    "_build",
    "deps",
    "bin",
    "obj",
    ".next",
    ".nuxt",
    ".turbo",
    "dist",
    "build",
    ".svelte-kit",
    ".parcel-cache",
    ".expo",
    ".angular",
    "coverage",
    ".tox",
    ".nox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "cmake-build-debug",
    "cmake-build-release",
    "Temp",
    "Logs",
];

struct ArtifactRule {
    dir: &'static str,
    kinds: &'static [ProjectKind],
    risk: Risk,
    description: &'static str,
}

const AR: &[ArtifactRule] = &[
    ArtifactRule {
        dir: "node_modules",
        kinds: &[
            ProjectKind::Node,
            ProjectKind::Php,
            ProjectKind::Ruby,
            ProjectKind::Python,
        ],
        risk: Risk::Low,
        description: "Installed npm/pnpm/yarn dependencies. Reinstall with your package manager.",
    },
    ArtifactRule {
        dir: ".next",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Next.js build output and cache.",
    },
    ArtifactRule {
        dir: ".nuxt",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Nuxt build output.",
    },
    ArtifactRule {
        dir: ".output",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Nitro / Nuxt production output.",
    },
    ArtifactRule {
        dir: ".svelte-kit",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "SvelteKit generated files.",
    },
    ArtifactRule {
        dir: ".turbo",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Turborepo local cache.",
    },
    ArtifactRule {
        dir: ".parcel-cache",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Parcel bundler cache.",
    },
    ArtifactRule {
        dir: ".angular",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Angular CLI cache.",
    },
    ArtifactRule {
        dir: ".expo",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Expo local cache.",
    },
    ArtifactRule {
        dir: "storybook-static",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Static Storybook build.",
    },
    ArtifactRule {
        dir: "coverage",
        kinds: &[
            ProjectKind::Node,
            ProjectKind::Python,
            ProjectKind::Php,
            ProjectKind::Ruby,
        ],
        risk: Risk::Low,
        description: "Test coverage reports.",
    },
    ArtifactRule {
        dir: "dist",
        kinds: &[ProjectKind::Node, ProjectKind::Python],
        risk: Risk::Low,
        description: "Build output.",
    },
    ArtifactRule {
        dir: "build",
        kinds: &[
            ProjectKind::Node,
            ProjectKind::Python,
            ProjectKind::Gradle,
            ProjectKind::Swift,
            ProjectKind::Flutter,
            ProjectKind::Cmake,
        ],
        risk: Risk::Low,
        description: "Build output.",
    },
    ArtifactRule {
        dir: "out",
        kinds: &[ProjectKind::Node],
        risk: Risk::Low,
        description: "Static export output.",
    },
    ArtifactRule {
        dir: "target",
        kinds: &[ProjectKind::Rust, ProjectKind::Maven],
        risk: Risk::Low,
        description: "Compiler output (cargo / maven). Rebuilt on next build.",
    },
    ArtifactRule {
        dir: ".venv",
        kinds: &[ProjectKind::Python],
        risk: Risk::Low,
        description: "Python virtual environment. Recreate with uv/poetry/pip.",
    },
    ArtifactRule {
        dir: "venv",
        kinds: &[ProjectKind::Python],
        risk: Risk::Low,
        description: "Python virtual environment.",
    },
    ArtifactRule {
        dir: ".tox",
        kinds: &[ProjectKind::Python],
        risk: Risk::Low,
        description: "tox environments.",
    },
    ArtifactRule {
        dir: ".nox",
        kinds: &[ProjectKind::Python],
        risk: Risk::Low,
        description: "nox environments.",
    },
    ArtifactRule {
        dir: ".mypy_cache",
        kinds: &[ProjectKind::Python],
        risk: Risk::Low,
        description: "mypy cache.",
    },
    ArtifactRule {
        dir: ".pytest_cache",
        kinds: &[ProjectKind::Python],
        risk: Risk::Low,
        description: "pytest cache.",
    },
    ArtifactRule {
        dir: ".ruff_cache",
        kinds: &[ProjectKind::Python],
        risk: Risk::Low,
        description: "ruff cache.",
    },
    ArtifactRule {
        dir: "vendor",
        kinds: &[ProjectKind::Php],
        risk: Risk::Low,
        description: "Composer dependencies. Reinstall with `composer install`.",
    },
    ArtifactRule {
        dir: "public/build",
        kinds: &[ProjectKind::Php],
        risk: Risk::Low,
        description: "Compiled Vite / Mix assets.",
    },
    ArtifactRule {
        dir: "storage/framework/cache",
        kinds: &[ProjectKind::Php],
        risk: Risk::Medium,
        description: "Laravel framework cache.",
    },
    ArtifactRule {
        dir: ".gradle",
        kinds: &[ProjectKind::Gradle],
        risk: Risk::Low,
        description: "Project-local Gradle cache.",
    },
    ArtifactRule {
        dir: ".build",
        kinds: &[ProjectKind::Swift],
        risk: Risk::Low,
        description: "Swift Package Manager build output.",
    },
    ArtifactRule {
        dir: "Pods",
        kinds: &[ProjectKind::Swift],
        risk: Risk::Medium,
        description: "CocoaPods dependencies. Reinstall with `pod install`.",
    },
    ArtifactRule {
        dir: "DerivedData",
        kinds: &[ProjectKind::Swift],
        risk: Risk::Low,
        description: "Project-local Xcode DerivedData.",
    },
    ArtifactRule {
        dir: ".dart_tool",
        kinds: &[ProjectKind::Flutter],
        risk: Risk::Low,
        description: "Dart tool cache.",
    },
    ArtifactRule {
        dir: "bin",
        kinds: &[ProjectKind::Dotnet],
        risk: Risk::Low,
        description: ".NET build output.",
    },
    ArtifactRule {
        dir: "obj",
        kinds: &[ProjectKind::Dotnet],
        risk: Risk::Low,
        description: ".NET intermediate output.",
    },
    ArtifactRule {
        dir: "vendor/bundle",
        kinds: &[ProjectKind::Ruby],
        risk: Risk::Low,
        description: "Bundler-installed gems.",
    },
    ArtifactRule {
        dir: "tmp/cache",
        kinds: &[ProjectKind::Ruby],
        risk: Risk::Low,
        description: "Rails tmp cache.",
    },
    ArtifactRule {
        dir: "_build",
        kinds: &[ProjectKind::Elixir],
        risk: Risk::Low,
        description: "Mix build output.",
    },
    ArtifactRule {
        dir: "deps",
        kinds: &[ProjectKind::Elixir],
        risk: Risk::Low,
        description: "Mix dependencies.",
    },
    ArtifactRule {
        dir: "cmake-build-debug",
        kinds: &[ProjectKind::Cmake],
        risk: Risk::Low,
        description: "CLion CMake build directory.",
    },
    ArtifactRule {
        dir: "cmake-build-release",
        kinds: &[ProjectKind::Cmake],
        risk: Risk::Low,
        description: "CLion CMake build directory.",
    },
    ArtifactRule {
        dir: "Library",
        kinds: &[ProjectKind::Unity],
        risk: Risk::Medium,
        description: "Unity imported asset cache. Reimport takes a while.",
    },
    ArtifactRule {
        dir: "Temp",
        kinds: &[ProjectKind::Unity],
        risk: Risk::Low,
        description: "Unity temp files.",
    },
    ArtifactRule {
        dir: "Logs",
        kinds: &[ProjectKind::Unity],
        risk: Risk::Low,
        description: "Unity logs.",
    },
];

/// Detect project ecosystems from marker files in `dir`.
pub fn detect_kinds(dir: &Path) -> Vec<ProjectKind> {
    let mut kinds = Vec::new();
    let has = |name: &str| dir.join(name).exists();
    if has("package.json") {
        kinds.push(ProjectKind::Node);
    }
    if has("Cargo.toml") {
        kinds.push(ProjectKind::Rust);
    }
    if has("go.mod") {
        kinds.push(ProjectKind::Go);
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") || has("Pipfile") {
        kinds.push(ProjectKind::Python);
    }
    if has("composer.json") || has("artisan") {
        kinds.push(ProjectKind::Php);
    }
    if has("build.gradle")
        || has("build.gradle.kts")
        || has("settings.gradle")
        || has("settings.gradle.kts")
    {
        kinds.push(ProjectKind::Gradle);
    }
    if has("pom.xml") {
        kinds.push(ProjectKind::Maven);
    }
    if has("Package.swift")
        || has("Podfile")
        || has_ext(dir, "xcodeproj")
        || has_ext(dir, "xcworkspace")
    {
        kinds.push(ProjectKind::Swift);
    }
    if has_ext(dir, "csproj") || has_ext(dir, "sln") || has_ext(dir, "fsproj") {
        kinds.push(ProjectKind::Dotnet);
    }
    if has("pubspec.yaml") {
        kinds.push(ProjectKind::Flutter);
    }
    if has("Gemfile") {
        kinds.push(ProjectKind::Ruby);
    }
    if has("mix.exs") {
        kinds.push(ProjectKind::Elixir);
    }
    if has("CMakeLists.txt") {
        kinds.push(ProjectKind::Cmake);
    }
    if has("ProjectSettings/ProjectVersion.txt") && has("Assets") {
        kinds.push(ProjectKind::Unity);
    }
    if has("project.godot") {
        kinds.push(ProjectKind::Godot);
    }
    if has_ext(dir, "uproject") {
        kinds.push(ProjectKind::Unreal);
    }
    if has("build.zig") {
        kinds.push(ProjectKind::Zig);
    }
    if has("stack.yaml") || has("cabal.project") || has_ext(dir, "cabal") {
        kinds.push(ProjectKind::Haskell);
    }
    if has_ext(dir, "tf") {
        kinds.push(ProjectKind::Terraform);
    }
    if has_ext(dir, "ipynb") {
        kinds.push(ProjectKind::Jupyter);
    }
    if has("pixi.toml") {
        kinds.push(ProjectKind::Pixi);
    }
    kinds
}

/// Nearest ancestor (or `start` itself) that is a git working tree root.
pub fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// Rust `target/` directories can be cleaned per profile when several exist.
pub fn split_cargo_target(target: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(target) else {
        return vec![];
    };
    let mut subs: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| {
            let n = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            n == "debug" || n == "release" || n == "doc" || n.contains('-')
        })
        .collect();
    subs.sort();
    if subs.len() >= 2 {
        subs
    } else {
        vec![]
    }
}

fn has_ext(dir: &Path, ext: &str) -> bool {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .any(|e| e.path().extension().map(|x| x == ext).unwrap_or(false))
        })
        .unwrap_or(false)
}

/// Estimate when the project was last worked on.
pub fn project_idle_days(root: &Path) -> u64 {
    let mut probes = vec![
        root.to_path_buf(),
        root.join(".git/index"),
        root.join(".git/HEAD"),
        root.join(".git/FETCH_HEAD"),
        root.join(".git/ORIG_HEAD"),
        root.join(".git/logs/HEAD"),
        root.join("package.json"),
        root.join("Cargo.toml"),
        root.join("go.mod"),
        root.join("pyproject.toml"),
        root.join("composer.json"),
        root.join("src"),
        root.join("lib"),
        root.join("app"),
        root.join("README.md"),
    ];
    // `.git` may be a file for linked worktrees; it still carries a useful mtime.
    probes.push(root.join(".git"));
    let newest = newest_modified(&probes).or_else(|| modified_at(root));
    match newest {
        Some(t) => Local::now().signed_duration_since(t).num_days().max(0) as u64,
        None => 0,
    }
}

/// Options for the walk.
pub struct WalkOptions<'a> {
    pub roots: &'a [PathBuf],
    pub max_depth: usize,
    pub protected: &'a [PathBuf],
    pub skip: &'a [PathBuf],
}

/// Walk all roots and return discovered projects and artifact hits.
/// `on_dir` is called for every directory visited (for progress).
pub fn walk(opts: &WalkOptions, mut on_dir: impl FnMut(&Path)) -> (Vec<Project>, Vec<ArtifactHit>) {
    let mut projects = Vec::new();
    let mut hits = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let roots = dedupe_roots(opts.roots);
    for root in &roots {
        walk_dir(
            root,
            0,
            opts,
            &mut seen,
            &mut projects,
            &mut hits,
            &mut on_dir,
        );
    }
    (projects, hits)
}

fn dedupe_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    // Canonicalize so that `Projects` and `projects` on a case-insensitive
    // filesystem (macOS, Windows) collapse into one root instead of being
    // walked twice under two spellings.
    let mut sorted: Vec<PathBuf> = roots
        .iter()
        .filter(|r| r.is_dir())
        .map(|r| canonical(r))
        .collect();
    sorted.sort();
    sorted.dedup();
    let mut out: Vec<PathBuf> = Vec::new();
    for r in sorted {
        if !out.iter().any(|o| is_within(&r, o)) {
            out.push(r);
        }
    }
    out
}

/// Canonical form of a path when it exists (resolves symlinks and Windows 8.3
/// short names), otherwise the path itself.
pub fn canonical(p: &Path) -> PathBuf {
    strip_verbatim(fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
}

/// `fs::canonicalize` on Windows yields `\\?\C:\...`; keep paths readable.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if cfg!(windows) => PathBuf::from(rest),
        _ => p,
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_dir(
    dir: &Path,
    depth: usize,
    opts: &WalkOptions,
    seen: &mut HashSet<PathBuf>,
    projects: &mut Vec<Project>,
    hits: &mut Vec<ArtifactHit>,
    on_dir: &mut impl FnMut(&Path),
) {
    if depth > opts.max_depth {
        return;
    }
    if opts.protected.iter().any(|p| is_within(dir, p))
        || opts.skip.iter().any(|p| is_within(dir, p))
    {
        return;
    }
    if !seen.insert(dir.to_path_buf()) {
        return;
    }
    on_dir(dir);

    let kinds = detect_kinds(dir);
    let mut artifact_dirs: HashSet<PathBuf> = HashSet::new();
    if !kinds.is_empty() {
        let is_git_repo = dir.join(".git").is_dir();
        let project = Project {
            root: dir.to_path_buf(),
            name: dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| dir.display().to_string()),
            kinds: kinds.clone(),
            is_git_repo,
            idle_days: project_idle_days(dir),
        };
        for rule in AR {
            if !rule.kinds.iter().any(|k| kinds.contains(k)) {
                continue;
            }
            let path = dir.join(rule.dir);
            if !is_real_dir(&path) {
                continue;
            }
            if rule.dir == "vendor" && !is_composer_vendor(dir) {
                continue;
            }
            if rule.dir == "build"
                && kinds.contains(&ProjectKind::Node)
                && !looks_like_build_output(&path)
            {
                continue;
            }
            if artifact_dirs.iter().any(|a| is_within(&path, a)) {
                continue;
            }
            artifact_dirs.insert(path.clone());
            if rule.dir == "target" && kinds.contains(&ProjectKind::Rust) {
                let parts = split_cargo_target(&path);
                if !parts.is_empty() {
                    for part in parts {
                        let name = part
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        hits.push(ArtifactHit {
                            project: project.clone(),
                            path: part,
                            label: format!("target/{name}"),
                            description: "Cargo build output for one profile or target triple. Rebuilt on next build.",
                            risk: rule.risk,
                        });
                    }
                    continue;
                }
            }
            hits.push(ArtifactHit {
                project: project.clone(),
                path,
                label: rule.dir.to_string(),
                description: rule.description,
                risk: rule.risk,
            });
        }
        projects.push(project);
    }

    let entries = match fs::read_dir(dir) {
        Ok(rd) => rd.flatten(),
        Err(_) => return,
    };
    let mut children: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if !ft.is_dir() || ft.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if NEVER_DESCEND.contains(&name.as_ref()) {
            continue;
        }
        if name.starts_with('.') {
            continue;
        }
        if artifact_dirs.iter().any(|a| is_within(&path, a)) {
            continue;
        }
        children.push(path);
    }
    children.sort();
    for child in children {
        walk_dir(&child, depth + 1, opts, seen, projects, hits, on_dir);
    }
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.is_dir() && !m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Only treat `vendor` as Composer output when a lock file or autoloader exists.
fn is_composer_vendor(project: &Path) -> bool {
    project.join("composer.lock").exists() || project.join("vendor/autoload.php").exists()
}

/// Node projects sometimes keep sources in `build/`; only accept it when it looks generated.
fn looks_like_build_output(path: &Path) -> bool {
    let markers = [
        "index.html",
        "asset-manifest.json",
        "static",
        "manifest.json",
        "bundle.js",
        "main.js",
    ];
    markers.iter().any(|m| path.join(m).exists())
        || fs::read_dir(path)
            .map(|rd| {
                rd.flatten()
                    .any(|e| e.path().extension().map(|x| x == "map").unwrap_or(false))
            })
            .unwrap_or(false)
}

/// Turn an artifact hit into a candidate (size filled in by the caller).
pub fn hit_to_candidate(hit: &ArtifactHit, stale_days: u64) -> Candidate {
    let kinds: Vec<&str> = hit.project.kinds.iter().map(|k| k.as_str()).collect();
    let description = format!(
        "{} Project: {} ({}), idle {} days.",
        hit.description,
        hit.project.name,
        kinds.join("+"),
        hit.project.idle_days
    );
    let mut c = Candidate::new(
        format!("{} · {}", hit.project.name, hit.label),
        description,
        Category::Project,
        hit.risk,
        Action::RemovePath {
            path: hit.path.clone(),
        },
    )
    .with_modified(modified_at(&hit.path))
    .with_owner(hit.project.name.clone());
    c.project = Some(ProjectInfo {
        root: hit.project.root.clone(),
        name: hit.project.name.clone(),
        kind: hit.project.kinds[0],
        idle_days: hit.project.idle_days,
    });
    for k in &hit.project.kinds {
        c.tags.push(k.as_str().to_string());
    }
    if hit.project.idle_days >= stale_days {
        c = c.with_tag("stale");
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn detects_projects_and_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("code");
        // node project with node_modules and a source "build" dir that must be ignored
        touch(&root.join("web/package.json"));
        touch(&root.join("web/node_modules/left-pad/index.js"));
        touch(&root.join("web/build/src/main.ts"));
        // rust project nested in a monorepo
        touch(&root.join("mono/Cargo.toml"));
        touch(&root.join("mono/target/debug/app"));
        touch(&root.join("mono/crates/a/Cargo.toml"));
        touch(&root.join("mono/crates/a/target/x"));
        // php project with composer vendor
        touch(&root.join("api/composer.json"));
        touch(&root.join("api/composer.lock"));
        touch(&root.join("api/vendor/autoload.php"));
        // php without lock: vendor is probably sources
        touch(&root.join("theme/composer.json"));
        touch(&root.join("theme/vendor/lib.php"));
        // plain folder named build in a non-project: ignored
        touch(&root.join("notes/build/file.txt"));
        // protected
        touch(&root.join("Dropbox/x/package.json"));
        touch(&root.join("Dropbox/x/node_modules/a"));

        let protected = vec![root.join("Dropbox")];
        let roots = vec![root.clone()];
        let (projects, hits) = walk(
            &WalkOptions {
                roots: &roots,
                max_depth: 6,
                protected: &protected,
                skip: &[],
            },
            |_| {},
        );
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"web"));
        assert!(names.contains(&"mono"));
        assert!(names.contains(&"a"));
        assert!(names.contains(&"api"));
        assert!(!names.contains(&"x"), "protected project must be skipped");

        let paths: Vec<String> = hits
            .iter()
            .map(|h| {
                h.path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(paths.contains(&"web/node_modules".to_string()));
        assert!(
            !paths.contains(&"web/build".to_string()),
            "source build dir must not be an artifact"
        );
        assert!(paths.contains(&"mono/target".to_string()));
        assert!(paths.contains(&"mono/crates/a/target".to_string()));
        assert!(paths.contains(&"api/vendor".to_string()));
        assert!(!paths.contains(&"theme/vendor".to_string()));
        assert!(!paths.iter().any(|p| p.starts_with("notes")));

        let c = hit_to_candidate(&hits[0], 0);
        assert_eq!(c.category, Category::Project);
        assert!(c.is_stale());
        assert!(c.project.is_some());
    }

    #[test]
    fn cargo_target_split_and_git_root() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path().join("target");
        fs::create_dir_all(t.join("debug")).unwrap();
        assert!(
            split_cargo_target(&t).is_empty(),
            "single profile stays whole"
        );
        fs::create_dir_all(t.join("release")).unwrap();
        fs::create_dir_all(t.join("aarch64-apple-darwin")).unwrap();
        assert_eq!(split_cargo_target(&t).len(), 3);
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        assert_eq!(find_git_root(&t.join("debug")).unwrap(), tmp.path());
        assert!(detect_kinds(tmp.path()).is_empty());
        fs::write(tmp.path().join("project.godot"), "").unwrap();
        assert_eq!(detect_kinds(tmp.path()), vec![ProjectKind::Godot]);
    }

    #[test]
    fn roots_are_deduped() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = a.join("b");
        fs::create_dir_all(&b).unwrap();
        let out = dedupe_roots(&[b.clone(), a.clone(), a.clone(), tmp.path().join("missing")]);
        assert_eq!(out, vec![canonical(&a)]);
    }
}
