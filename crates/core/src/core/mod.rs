//! Platform-independent domain model: candidates, sizes, safety rules,
//! persistent state and the cleaner that executes actions.

pub mod candidate;
pub mod cleaner;
pub mod disk;
pub mod fsutil;
pub mod i18n;
pub mod process;
pub mod recovery;
pub mod report;
pub mod state;

pub use candidate::{Action, Candidate, Category, Risk};
