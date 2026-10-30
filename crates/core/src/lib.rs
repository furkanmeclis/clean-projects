//! CleanProjects engine: candidate model, platform rules, project/git scanning
//! and the cleaner. Shared by the CLI/TUI and the desktop app.

#![forbid(unsafe_code)]

pub mod core;
pub mod scan;

pub use core::candidate::{Action, Candidate, Category, ProjectInfo, ProjectKind, Risk};
pub use core::cleaner::{self, CleanEvent, CleanMode, CleanOptions};
pub use core::disk::{disk_for, volumes, DiskInfo, Volume, VolumeKind};
pub use core::fsutil;
pub use core::i18n::{Lang, Strings};
pub use core::recovery::{self as recovery, Journal, RecoveryEntry};
pub use core::report::{self as report, Format as ReportFormat};
pub use core::state::ScanSnapshot;
pub use core::state::{Config, HistoryEntry, HistoryItem, Paths, State};
pub use scan::tree::{large_files, list_dir, tree, Node, TreeNode};
pub use scan::{load_cache, save_cache, scan, scan_blocking, ScanContext, ScanEvent, ScanResult};

/// Crate version, shared by every front end.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
