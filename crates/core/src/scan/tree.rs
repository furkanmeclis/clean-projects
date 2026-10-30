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

#[cfg(test)]
mod tests {
    use super::*;

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
