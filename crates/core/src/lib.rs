//! CleanProjects engine: candidate model, platform rules, project/git scanning
//! and the cleaner. Shared by the CLI/TUI and the desktop app.

#![forbid(unsafe_code)]

pub mod core;
pub mod scan;

pub use core::candidate::{Action, Candidate, Category, ProjectInfo, ProjectKind, Risk};
pub use core::cleaner::{self, CleanEvent, CleanOptions};
pub use core::disk::{disk_for, DiskInfo};
pub use core::fsutil;
pub use core::i18n::{Lang, Strings};
pub use core::state::{Config, HistoryEntry, HistoryItem, Paths, State};
pub use scan::{scan, scan_blocking, ScanContext, ScanEvent, ScanResult};

/// Crate version, shared by every front end.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
