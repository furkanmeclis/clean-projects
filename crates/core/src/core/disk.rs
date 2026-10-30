use std::path::Path;

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
