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

/// Recursively compute the apparent size of a path without following symlinks.
/// Directories are walked in parallel.
pub fn dir_size(path: &Path) -> u64 {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if meta.file_type().is_symlink() {
        return 0;
    }
    if meta.is_file() {
        return meta.len();
    }
    let total = AtomicU64::new(0);
    walk_size(path, &total);
    total.load(Ordering::Relaxed)
}

fn walk_size(dir: &Path, total: &AtomicU64) {
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
        if ft.is_dir() {
            walk_size(&entry.path(), total);
        } else {
            total.fetch_add(meta.len(), Ordering::Relaxed);
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
        assert_eq!(dir_size(dir.path()), 350);
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
