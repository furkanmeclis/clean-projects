//! Recovery bin: deleted items are moved (renamed) into a per-volume holding
//! directory and recorded in a journal, so a cleanup can be undone until the
//! bin is purged.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use super::disk::{volume_for, Volume, VolumeKind};

pub const BIN_DIR_NAME: &str = ".cleanprojects-recovery";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecoveryEntry {
    pub id: String,
    pub run_at: DateTime<Local>,
    pub candidate_id: String,
    pub label: String,
    pub original: PathBuf,
    pub stored: PathBuf,
    pub size: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Journal {
    pub entries: Vec<RecoveryEntry>,
}

impl Journal {
    pub fn load(path: &Path) -> Journal {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    /// Entries whose stored copy still exists.
    pub fn prune_missing(&mut self) {
        self.entries.retain(|e| e.stored.exists());
    }
}

/// Where items from `path` are held: on the same volume, so the move is a rename.
pub fn bin_dir_for(path: &Path, home: &Path, data_dir: &Path, vols: &[Volume]) -> PathBuf {
    match volume_for(path, vols) {
        Some(v) if v.kind != VolumeKind::System && !home.starts_with(&v.mount) => {
            v.mount.join(BIN_DIR_NAME)
        }
        _ => data_dir.join("recovery"),
    }
}

fn unique_target(bin: &Path, run_id: &str, original: &Path) -> PathBuf {
    let name = original
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".into());
    let mut n = 0;
    loop {
        let candidate = if n == 0 {
            bin.join(run_id).join(&name)
        } else {
            bin.join(run_id).join(format!("{name}~{n}"))
        };
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Error returned when the item cannot be renamed into the bin (other volume, permissions).
#[derive(Debug)]
pub struct CannotMove(pub std::io::Error);

impl std::fmt::Display for CannotMove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot move into recovery bin: {}", self.0)
    }
}

impl std::error::Error for CannotMove {}

/// Move `original` into the recovery bin and return the journal entry.
pub fn move_to_bin(
    original: &Path,
    bin: &Path,
    run_id: &str,
    candidate_id: &str,
    label: &str,
    size: u64,
) -> std::result::Result<RecoveryEntry, CannotMove> {
    let target = unique_target(bin, run_id, original);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(CannotMove)?;
    }
    fs::rename(original, &target).map_err(CannotMove)?;
    Ok(RecoveryEntry {
        id: format!(
            "{run_id}-{}",
            target
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
        run_at: Local::now(),
        candidate_id: candidate_id.to_string(),
        label: label.to_string(),
        original: original.to_path_buf(),
        stored: target,
        size,
    })
}

/// Put an entry back where it came from.
pub fn restore(entry: &RecoveryEntry) -> Result<()> {
    if !entry.stored.exists() {
        return Err(anyhow!("{} is no longer in the recovery bin", entry.label));
    }
    if entry.original.exists() {
        return Err(anyhow!(
            "{} already exists; remove it first",
            entry.original.display()
        ));
    }
    if let Some(parent) = entry.original.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::rename(&entry.stored, &entry.original)
        .with_context(|| format!("restoring {}", entry.original.display()))?;
    cleanup_empty_run_dir(&entry.stored);
    Ok(())
}

/// Permanently delete an entry from the bin.
pub fn purge_entry(entry: &RecoveryEntry) -> Result<()> {
    if entry.stored.exists() {
        super::fsutil::remove_path(&entry.stored)
            .with_context(|| format!("purging {}", entry.stored.display()))?;
    }
    cleanup_empty_run_dir(&entry.stored);
    Ok(())
}

fn cleanup_empty_run_dir(stored: &Path) {
    if let Some(run_dir) = stored.parent() {
        if fs::read_dir(run_dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false)
        {
            let _ = fs::remove_dir(run_dir);
        }
    }
}

/// Purge entries older than `days` and return how many bytes were released.
pub fn purge_older_than(journal: &mut Journal, days: u64) -> (usize, u64) {
    let cutoff = Local::now() - chrono::Duration::days(days as i64);
    let mut n = 0;
    let mut bytes = 0;
    journal.entries.retain(|e| {
        if e.run_at < cutoff && purge_entry(e).is_ok() {
            n += 1;
            bytes += e.size;
            return false;
        }
        true
    });
    (n, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_restore_purge_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        let orig = tmp.path().join("proj/node_modules");
        fs::create_dir_all(orig.join("pkg")).unwrap();
        fs::write(orig.join("pkg/x"), b"1").unwrap();
        let e = move_to_bin(&orig, &bin, "run1", "cid", "node_modules", 1).unwrap();
        assert!(!orig.exists());
        assert!(e.stored.exists());
        let e2 = move_to_bin(
            &orig.parent().unwrap().join("other"),
            &bin,
            "run1",
            "c2",
            "other",
            0,
        );
        assert!(e2.is_err(), "missing source must fail");
        restore(&e).unwrap();
        assert!(orig.join("pkg/x").exists());
        let e = move_to_bin(&orig, &bin, "run2", "cid", "node_modules", 1).unwrap();
        let mut j = Journal {
            entries: vec![e.clone()],
        };
        let (n, _) = purge_older_than(&mut j, 30);
        assert_eq!(n, 0);
        j.entries[0].run_at = Local::now() - chrono::Duration::days(40);
        let (n, _) = purge_older_than(&mut j, 30);
        assert_eq!(n, 1);
        assert!(!e.stored.exists());
        let jp = tmp.path().join("j.json");
        j.save(&jp).unwrap();
        assert_eq!(Journal::load(&jp), j);
    }
}
