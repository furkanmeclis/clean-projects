use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sysinfo::Disks;

/// Snapshot of the volume that contains the home directory.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiskInfo {
    pub mount: String,
    pub total: u64,
    pub free: u64,
    pub used: u64,
}

impl DiskInfo {
    pub fn used_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f64 / self.total as f64 * 100.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeKind {
    /// The volume that holds the home directory / operating system.
    System,
    /// A mounted drive (external SSD, USB stick, second internal disk).
    External,
    /// Network share.
    Network,
}

/// A mounted volume worth showing to the user.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub mount: PathBuf,
    pub total: u64,
    pub free: u64,
    pub used: u64,
    pub kind: VolumeKind,
    pub removable: bool,
    pub read_only: bool,
    pub file_system: String,
}

impl Volume {
    pub fn used_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f64 / self.total as f64 * 100.0
        }
    }

    pub fn info(&self) -> DiskInfo {
        DiskInfo {
            mount: self.mount.display().to_string(),
            total: self.total,
            free: self.free,
            used: self.used,
        }
    }
}

/// Mount points that are never real user volumes.
fn is_noise_mount(mount: &Path) -> bool {
    let m = mount.to_string_lossy();
    let noise = [
        "/dev",
        "/proc",
        "/sys",
        "/boot",
        "/snap/",
        "/var/lib/docker",
        "/var/lib/snapd",
        "/run/",
        "/System/Volumes/",
        "/private/var/vm",
        "/Library/Developer/CoreSimulator",
    ];
    if noise
        .iter()
        .any(|n| m == n.trim_end_matches('/') || m.starts_with(n))
        && !m.starts_with("/run/media/")
    {
        return true;
    }
    false
}

/// All volumes a user can work with: the system volume first, then external and network ones.
pub fn volumes(home: &Path) -> Vec<Volume> {
    let Ok(disks) = Disks::new_with_refreshed_list() else {
        return Vec::new();
    };
    let mut out: Vec<Volume> = Vec::new();
    let mut home_mount: Option<PathBuf> = None;
    let mut home_len = 0usize;
    for d in disks.iter() {
        let mp = d.mount_point();
        if home.starts_with(mp) && mp.as_os_str().len() >= home_len {
            home_len = mp.as_os_str().len();
            home_mount = Some(mp.to_path_buf());
        }
    }
    for d in disks.iter() {
        let mp = d.mount_point().to_path_buf();
        let total = d.total_space();
        if total == 0 || is_noise_mount(&mp) {
            continue;
        }
        let is_home = home_mount.as_ref().map(|h| h == &mp).unwrap_or(false);
        let fs = d.file_system().to_string_lossy().to_string();
        let network = matches!(
            fs.to_ascii_lowercase().as_str(),
            "smbfs" | "nfs" | "cifs" | "afpfs" | "webdav" | "sshfs" | "fuse.sshfs"
        );
        let kind = if is_home {
            VolumeKind::System
        } else if network {
            VolumeKind::Network
        } else {
            VolumeKind::External
        };
        if out.iter().any(|v| v.mount == mp) {
            continue;
        }
        let name = {
            let n = d.name().to_string_lossy().to_string();
            if n.is_empty() || n.starts_with("/dev") {
                mp.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| mp.display().to_string())
            } else {
                n
            }
        };
        let free = d.available_space();
        out.push(Volume {
            name,
            mount: mp,
            total,
            free,
            used: total.saturating_sub(free),
            kind,
            removable: d.is_removable(),
            read_only: d.is_read_only(),
            file_system: fs,
        });
    }
    out.sort_by(|a, b| {
        (a.kind != VolumeKind::System)
            .cmp(&(b.kind != VolumeKind::System))
            .then(a.mount.cmp(&b.mount))
    });
    out
}

/// The volume whose mount point is the longest prefix of `path`.
pub fn volume_for<'a>(path: &Path, vols: &'a [Volume]) -> Option<&'a Volume> {
    vols.iter()
        .filter(|v| path.starts_with(&v.mount))
        .max_by_key(|v| v.mount.as_os_str().len())
}

/// Find the disk whose mount point is the longest prefix of `path`.
pub fn disk_for(path: &Path) -> DiskInfo {
    let Ok(disks) = Disks::new_with_refreshed_list() else {
        return DiskInfo::default();
    };
    let mut best: Option<DiskInfo> = None;
    let mut best_len = 0usize;
    for d in disks.iter() {
        let mp = d.mount_point();
        if path.starts_with(mp) {
            let len = mp.as_os_str().len();
            if len >= best_len {
                best_len = len;
                let total = d.total_space();
                let free = d.available_space();
                best = Some(DiskInfo {
                    mount: mp.display().to_string(),
                    total,
                    free,
                    used: total.saturating_sub(free),
                });
            }
        }
    }
    best.unwrap_or_default()
}

/// Mount points of external volumes that can be scanned for projects.
pub fn external_roots(vols: &[Volume]) -> Vec<PathBuf> {
    vols.iter()
        .filter(|v| v.kind == VolumeKind::External && !v.read_only)
        .map(|v| v.mount.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_filter() {
        assert!(is_noise_mount(Path::new("/dev")));
        assert!(is_noise_mount(Path::new("/System/Volumes/Preboot")));
        assert!(is_noise_mount(Path::new("/snap/core/1234")));
        assert!(!is_noise_mount(Path::new("/run/media/me/SSD")));
        assert!(!is_noise_mount(Path::new("/Volumes/Work")));
        assert!(!is_noise_mount(Path::new("/")));
    }

    #[test]
    fn lists_system_volume_first() {
        let home = std::env::temp_dir();
        let vols = volumes(&home);
        if let Some(first) = vols.first() {
            assert_eq!(first.kind, VolumeKind::System);
            assert!(volume_for(&home, &vols).is_some());
        }
    }
}
