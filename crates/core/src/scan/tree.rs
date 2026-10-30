//! Directory explorer data: sized children of a directory, for treemaps and the TUI browser.

use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::fsutil::{measure, modified_at};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub files: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub modified: Option<chrono::DateTime<chrono::Local>>,
}

/// Children of `dir`, sized in parallel and sorted by size descending.
pub fn list_dir(dir: &Path) -> Vec<Node> {
    let entries: Vec<_> = match fs::read_dir(dir) {
        Ok(rd) => rd.flatten().collect(),
        Err(_) => return Vec::new(),
    };
    let mut nodes: Vec<Node> = entries
        .par_iter()
        .filter_map(|e| {
            let path = e.path();
            let meta = fs::symlink_metadata(&path).ok()?;
            let is_symlink = meta.file_type().is_symlink();
            let is_dir = meta.is_dir() && !is_symlink;
            let (size, files) = if is_symlink {
                (0, 0)
            } else if is_dir {
                let m = measure(&path);
                (m.allocated, m.files)
            } else {
                (meta.len(), 1)
            };
            Some(Node {
                name: e.file_name().to_string_lossy().to_string(),
                modified: modified_at(&path),
                path,
                size,
                files,
                is_dir,
                is_symlink,
            })
        })
        .collect();
    nodes.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    nodes
}

/// A nested tree limited to `depth` levels and `top` children per level (for treemaps).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeNode {
    #[serde(flatten)]
    pub node: Node,
    pub children: Vec<TreeNode>,
    /// Bytes in children that were cut off by `top`.
    pub other: u64,
}

pub fn tree(dir: &Path, depth: usize, top: usize) -> Vec<TreeNode> {
    let mut nodes = list_dir(dir);
    let other: u64 = nodes.iter().skip(top).map(|n| n.size).sum();
    nodes.truncate(top);
    let mut out: Vec<TreeNode> = nodes
        .into_par_iter()
        .map(|n| {
            let children = if depth > 1 && n.is_dir && n.size > 0 {
                tree(&n.path, depth - 1, top)
            } else {
                Vec::new()
            };
            TreeNode {
                node: n,
                children,
                other: 0,
            }
        })
        .collect();
    if other > 0 {
        if let Some(last) = out.last_mut() {
            last.other = other;
        }
    }
    out
}

/// Files of at least `min_size` bytes under `roots`, largest first, at most `limit`.
/// Protected locations and volume system folders are skipped; symlinks are not followed.
pub fn large_files(
    roots: &[PathBuf],
    min_size: u64,
    limit: usize,
    protected: &[PathBuf],
) -> Vec<Node> {
    use crate::core::fsutil::{is_within, VOLUME_SYSTEM_DIRS};
    use std::sync::Mutex;
    let found: Mutex<Vec<Node>> = Mutex::new(Vec::new());
    fn visit(
        dir: &Path,
        min_size: u64,
        protected: &[PathBuf],
        found: &Mutex<Vec<Node>>,
        depth: usize,
    ) {
        if protected.iter().any(|p| is_within(dir, p)) {
            return;
        }
        let entries: Vec<_> = match fs::read_dir(dir) {
            Ok(rd) => rd.flatten().collect(),
            Err(_) => return,
        };
        entries.par_iter().for_each(|e| {
            let Ok(meta) = e.metadata() else { return };
            let ft = meta.file_type();
            if ft.is_symlink() {
                return;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if ft.is_dir() {
                if name == ".git" || VOLUME_SYSTEM_DIRS.contains(&name.as_str()) || depth > 24 {
                    return;
                }
                visit(&e.path(), min_size, protected, found, depth + 1);
            } else if meta.len() >= min_size {
                let path = e.path();
                found.lock().unwrap().push(Node {
                    name,
                    modified: modified_at(&path),
                    path,
                    size: meta.len(),
                    files: 1,
                    is_dir: false,
                    is_symlink: false,
                });
            }
        });
    }
    for root in roots {
        visit(root, min_size, protected, &found, 0);
    }
    let mut out = found.into_inner().unwrap();
    out.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    out.dedup_by(|a, b| a.path == b.path);
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_large_files_and_skips_protected() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("a/b")).unwrap();
        fs::create_dir_all(tmp.path().join("Dropbox")).unwrap();
        fs::write(tmp.path().join("a/b/big.iso"), vec![0u8; 50_000]).unwrap();
        fs::write(tmp.path().join("a/small.txt"), vec![0u8; 10]).unwrap();
        fs::write(tmp.path().join("Dropbox/huge.bin"), vec![0u8; 90_000]).unwrap();
        let out = large_files(
            &[tmp.path().to_path_buf()],
            1000,
            10,
            &[tmp.path().join("Dropbox")],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "big.iso");
    }

    #[test]
    fn lists_sorted_children() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("big")).unwrap();
        fs::write(tmp.path().join("big/f"), vec![0u8; 100_000]).unwrap();
        fs::write(tmp.path().join("small.txt"), vec![0u8; 10]).unwrap();
        let nodes = list_dir(tmp.path());
        assert_eq!(nodes[0].name, "big");
        assert!(nodes[0].is_dir);
        assert_eq!(nodes[1].files, 1);
        let t = tree(tmp.path(), 2, 10);
        assert_eq!(t[0].children.len(), 1);
    }
}
