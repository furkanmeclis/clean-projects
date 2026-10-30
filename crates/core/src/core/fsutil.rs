use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Local};
use rayon::prelude::*;

/// Format bytes as a short human string (1.4 GB, 320 MB, 12 KB).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// Parse strings such as "1.2GB", "512 MB", "3.4 GiB" into bytes (used for docker output).
pub fn parse_human_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let digits_end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(digits_end);
    let num: f64 = num.replace(',', ".").parse().ok()?;
    let unit = unit.trim().to_ascii_lowercase();
    let mult: f64 = match unit.as_str() {
        "" | "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((num * mult) as u64)
}

/// Result of measuring a directory tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Measure {
    /// Sum of file lengths.
    pub apparent: u64,
    /// Bytes actually allocated on disk (hardlinks counted once, sparse files honoured).
    pub allocated: u64,
    pub files: u64,
    /// A nested `.git` directory exists inside the tree.
    pub has_git: bool,
    /// Private keys were seen near the top of the tree.
    pub has_secrets: bool,
}

/// Recursively measure a path without following symlinks. Directories are walked in parallel.
pub fn measure(path: &Path) -> Measure {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return Measure::default(),
    };
    if meta.file_type().is_symlink() {
        return Measure::default();
    }
    if meta.is_file() {
        return Measure {
            apparent: meta.len(),
            allocated: allocated_len(&meta),
            files: 1,
            has_git: false,
            has_secrets: false,
        };
    }
    let acc = MeasureAcc::default();
    walk_measure(path, 0, &acc);
    Measure {
        apparent: acc.apparent.load(Ordering::Relaxed),
        allocated: acc.allocated.load(Ordering::Relaxed),
        files: acc.files.load(Ordering::Relaxed),
        has_git: acc.has_git.load(Ordering::Relaxed),
        has_secrets: acc.has_secrets.load(Ordering::Relaxed),
    }
}

/// Allocated size of a tree (what deleting it reclaims). Kept for callers that only need a number.
pub fn dir_size(path: &Path) -> u64 {
    measure(path).allocated
}

#[derive(Default)]
struct MeasureAcc {
    apparent: AtomicU64,
    allocated: AtomicU64,
    files: AtomicU64,
    has_git: std::sync::atomic::AtomicBool,
    has_secrets: std::sync::atomic::AtomicBool,
    seen: std::sync::Mutex<std::collections::HashSet<(u64, u64)>>,
}

#[cfg(unix)]
fn allocated_len(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn allocated_len(meta: &fs::Metadata) -> u64 {
    meta.len()
}

#[cfg(unix)]
fn hardlink_key(meta: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    if meta.nlink() > 1 {
        Some((meta.dev(), meta.ino()))
    } else {
        None
    }
}

#[cfg(not(unix))]
fn hardlink_key(_meta: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

const SECRET_NAMES: &[&str] = &["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"];
const SECRET_EXT: &[&str] = &["p12", "pfx"];

fn walk_measure(dir: &Path, depth: usize, acc: &MeasureAcc) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e.flatten().collect::<Vec<_>>(),
        Err(_) => return,
    };
    entries.par_iter().for_each(|entry| {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => return,
        };
        let ft = meta.file_type();
        if ft.is_symlink() {
            return;
        }
        let name = entry.file_name();
        if ft.is_dir() {
            if name == ".git" {
                acc.has_git.store(true, Ordering::Relaxed);
                return;
            }
            walk_measure(&entry.path(), depth + 1, acc);
        } else {
            if depth <= 1 {
                let n = name.to_string_lossy();
                let ext = Path::new(&*n)
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase());
                if SECRET_NAMES.contains(&n.as_ref())
                    || ext
                        .as_deref()
                        .map(|e| SECRET_EXT.contains(&e))
                        .unwrap_or(false)
                {
                    acc.has_secrets.store(true, Ordering::Relaxed);
                }
            }
            if let Some(key) = hardlink_key(&meta) {
                if !acc.seen.lock().unwrap().insert(key) {
                    return;
                }
            }
            acc.files.fetch_add(1, Ordering::Relaxed);
            acc.apparent.fetch_add(meta.len(), Ordering::Relaxed);
            acc.allocated
                .fetch_add(allocated_len(&meta), Ordering::Relaxed);
        }
    });
}

/// Modification time of a path as local datetime.
pub fn modified_at(path: &Path) -> Option<DateTime<Local>> {
    let meta = fs::symlink_metadata(path).ok()?;
    let m = meta.modified().ok()?;
    Some(DateTime::<Local>::from(m))
}

/// Newest modification time among the given paths.
pub fn newest_modified(paths: &[PathBuf]) -> Option<DateTime<Local>> {
    paths.iter().filter_map(|p| modified_at(p)).max()
}

/// Expand a leading `~` or `$HOME` to the home directory.
pub fn expand_home(input: &str, home: &Path) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        home.join(rest)
    } else if input == "~" {
        home.to_path_buf()
    } else if let Some(rest) = input.strip_prefix("$HOME/") {
        home.join(rest)
    } else {
        PathBuf::from(input)
    }
}

/// Replace the home directory prefix with `~` for compact display.
pub fn compact_path(path: &Path, home: &Path) -> String {
    if let Ok(rest) = path.strip_prefix(home) {
        if rest.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", rest.display())
        }
    } else {
        path.display().to_string()
    }
}

/// Returns true when `path` is `root` or lies inside it.
pub fn is_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

/// Cloud-synced and personal data roots that are never scanned or deleted.
pub fn default_protected_roots(home: &Path) -> Vec<PathBuf> {
    let mut v = vec![
        home.join("Library/Mobile Documents"),
        home.join("Library/CloudStorage"),
        home.join("Library/Keychains"),
        home.join("Library/Mail"),
        home.join("Library/Messages"),
        home.join("Library/Photos"),
        home.join("Pictures/Photos Library.photoslibrary"),
        home.join("Dropbox"),
        home.join("Google Drive"),
        home.join("OneDrive"),
        home.join("iCloud Drive"),
        home.join(".ssh"),
        home.join(".gnupg"),
        home.join(".aws"),
        home.join(".config/gcloud"),
        home.join(".kube"),
    ];
    if cfg!(windows) {
        v.push(home.join("OneDrive - Personal"));
    }
    v
}

/// Directory names that mark volume-level system data (never scanned or deleted).
pub const VOLUME_SYSTEM_DIRS: &[&str] = &[
    "Backups.backupdb",
    ".Spotlight-V100",
    ".fseventsd",
    ".TemporaryItems",
    ".DocumentRevisions-V100",
    ".Trashes",
    ".timemachine",
    "System Volume Information",
    "$RECYCLE.BIN",
    "lost+found",
    ".snapshots",
];

/// Reasons a path must never be deleted, independent of user configuration.
#[derive(Debug, PartialEq, Eq)]
pub enum Unsafe {
    NotAbsolute,
    IsRoot,
    IsHome,
    Protected(PathBuf),
    TooShallow,
    IsSymlink,
    InsideGitDir,
    Missing,
}

impl std::fmt::Display for Unsafe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unsafe::NotAbsolute => write!(f, "path is not absolute"),
            Unsafe::IsRoot => write!(f, "refusing to touch a filesystem root"),
            Unsafe::IsHome => write!(f, "refusing to touch the home directory itself"),
            Unsafe::Protected(p) => write!(f, "inside protected location {}", p.display()),
            Unsafe::TooShallow => write!(f, "path is too close to the filesystem root"),
            Unsafe::IsSymlink => write!(f, "path is a symlink"),
            Unsafe::InsideGitDir => write!(f, "path is inside a .git directory"),
            Unsafe::Missing => write!(f, "path no longer exists"),
        }
    }
}

/// Validate that a path is safe to delete. This is the last line of defence,
/// executed again right before deletion.
pub fn check_deletable(path: &Path, home: &Path, protected: &[PathBuf]) -> Result<(), Unsafe> {
    if !path.is_absolute() {
        return Err(Unsafe::NotAbsolute);
    }
    if path.parent().is_none() {
        return Err(Unsafe::IsRoot);
    }
    if path == home {
        return Err(Unsafe::IsHome);
    }
    // Require at least two components below the root, e.g. /Users/x/foo → ok, /tmp → no.
    let depth = path.components().count();
    if depth < 3 {
        return Err(Unsafe::TooShallow);
    }
    for p in protected {
        if is_within(path, p) {
            return Err(Unsafe::Protected(p.clone()));
        }
    }
    if path.components().any(|c| c.as_os_str() == ".git") {
        return Err(Unsafe::InsideGitDir);
    }
    if path
        .components()
        .any(|c| VOLUME_SYSTEM_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
    {
        return Err(Unsafe::Protected(path.to_path_buf()));
    }
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err(Unsafe::IsSymlink),
        Ok(_) => Ok(()),
        Err(_) => Err(Unsafe::Missing),
    }
}

/// Remove a path permanently (file or tree).
pub fn remove_path(path: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Truncate a string to `width` display columns, keeping the tail for paths.
pub fn ellipsize_middle(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if UnicodeWidthStr::width(s) <= width {
        return s.to_string();
    }
    if width < 5 {
        return s.chars().take(width).collect();
    }
    let keep = width - 1;
    let head = keep / 3;
    let tail = keep - head;
    let chars: Vec<char> = s.chars().collect();
    let h: String = chars.iter().take(head).collect();
    let t: String = chars[chars.len().saturating_sub(tail)..].iter().collect();
    format!("{h}…{t}")
}

/// Truncate a string to `width` display columns, keeping the head.
pub fn ellipsize_end(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if UnicodeWidthStr::width(s) <= width {
        return s.to_string();
    }
    if width < 2 {
        return s.chars().take(width).collect();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > width - 1 {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.00 KB");
        assert_eq!(human_size(15 * 1024 * 1024), "15.0 MB");
        assert_eq!(human_size(250 * 1024 * 1024 * 1024), "250 GB");
    }

    #[test]
    fn parse_sizes() {
        assert_eq!(
            parse_human_size("1.5GB"),
            Some((1.5 * 1024.0 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(parse_human_size("512 MB"), Some(512 * 1024 * 1024));
        assert_eq!(parse_human_size("0B"), Some(0));
        assert_eq!(parse_human_size("12kB"), Some(12 * 1024));
        assert_eq!(parse_human_size("nope"), None);
    }

    #[test]
    fn dir_size_counts_files_only() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), vec![0u8; 100]).unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/b.bin"), vec![0u8; 250]).unwrap();
        let m = measure(dir.path());
        assert_eq!(m.apparent, 350);
        assert_eq!(m.files, 2);
        assert!(m.allocated >= 350 || cfg!(not(unix)));
        assert!(!m.has_git && !m.has_secrets);
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join("id_rsa"), b"k").unwrap();
        let m = measure(dir.path());
        assert!(m.has_git && m.has_secrets);
    }

    #[cfg(unix)]
    #[test]
    fn hardlinks_counted_once() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        fs::write(&a, vec![0u8; 8192]).unwrap();
        fs::hard_link(&a, dir.path().join("b")).unwrap();
        let m = measure(dir.path());
        assert_eq!(m.files, 1);
        assert_eq!(m.apparent, 8192);
    }

    #[test]
    fn safety_checks() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        fs::create_dir_all(home.join("proj/node_modules")).unwrap();
        fs::create_dir_all(home.join("Dropbox/x")).unwrap();
        let protected = vec![home.join("Dropbox")];
        assert_eq!(
            check_deletable(&home.join("proj/node_modules"), &home, &protected),
            Ok(())
        );
        assert_eq!(
            check_deletable(&home, &home, &protected),
            Err(Unsafe::IsHome)
        );
        assert!(matches!(
            check_deletable(&home.join("Dropbox/x"), &home, &protected),
            Err(Unsafe::Protected(_))
        ));
        assert_eq!(
            check_deletable(Path::new("relative"), &home, &protected),
            Err(Unsafe::NotAbsolute)
        );
        assert_eq!(
            check_deletable(&home.join("missing"), &home, &protected),
            Err(Unsafe::Missing)
        );
        assert_eq!(
            check_deletable(&home.join("proj/.git/objects"), &home, &protected),
            Err(Unsafe::InsideGitDir)
        );
    }

    #[test]
    fn ellipsis() {
        assert_eq!(ellipsize_middle("abcdefghijkl", 7), "ab…ijkl");
        assert_eq!(ellipsize_end("abcdefghijkl", 5), "abcd…");
        assert_eq!(ellipsize_end("abc", 5), "abc");
    }

    #[test]
    fn compact_and_expand() {
        let home = Path::new("/Users/me");
        assert_eq!(compact_path(Path::new("/Users/me/dev"), home), "~/dev");
        assert_eq!(expand_home("~/dev", home), PathBuf::from("/Users/me/dev"));
        assert_eq!(expand_home("/abs", home), PathBuf::from("/abs"));
    }
}
