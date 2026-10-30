//! Command-line interface. `cleanprojects` without a subcommand opens the TUI.

pub mod commands;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use cleanprojects_core::core::i18n::Lang;

#[derive(Parser, Debug)]
#[command(
    name = "cleanprojects",
    version,
    about = "Developer disk cleanup: caches, build artifacts, Docker and git worktrees.",
    long_about = "CleanProjects v2 scans package caches, build outputs, IDE and browser caches, \
container engines and git repositories, then lets you remove only what you select. \
Run without a subcommand for the interactive terminal UI.",
    propagate_version = true
)]
pub struct Cli {
    /// UI language (en, tr). Overrides the config file.
    #[arg(long, global = true, env = "CLEANPROJECTS_LANG")]
    pub lang: Option<String>,

    /// Use a different config directory (default: OS config dir).
    #[arg(
        long,
        global = true,
        env = "CLEANPROJECTS_CONFIG_DIR",
        value_name = "DIR"
    )]
    pub config_dir: Option<PathBuf>,

    /// Disable colours in CLI output (also honours the NO_COLOR environment variable).
    #[arg(long, global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Open the interactive terminal UI (default).
    Tui(ScanArgs),
    /// Scan and print candidates without changing anything.
    Scan(ScanCmd),
    /// Scan, then clean the matching candidates.
    Clean(CleanCmd),
    /// Exclude a candidate id or path from future cleanups.
    Exclude { targets: Vec<String> },
    /// Remove an exclusion (or `--all`).
    Include {
        targets: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Show previous cleanups.
    History {
        #[arg(long)]
        json: bool,
        /// Show the most recent N entries.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Show or edit configuration.
    Config(ConfigCmd),
    /// Check optional tools, scan roots, volumes and protected paths.
    Doctor,
    /// Restore the items of the last cleanup (or `--run N`) from the recovery bin.
    Undo {
        /// 1 = last run, 2 = the one before, ...
        #[arg(long, default_value_t = 1)]
        run: usize,
        /// Restore a single recovery id instead of a whole run.
        #[arg(long, value_name = "ID")]
        id: Option<String>,
    },
    /// Show, purge or empty the recovery bin.
    Recovery {
        /// Delete entries older than the configured `clean.recovery_days`.
        #[arg(long)]
        purge: bool,
        /// Delete everything in the recovery bin.
        #[arg(long)]
        empty: bool,
        #[arg(long)]
        json: bool,
    },
    /// List mounted volumes and how they are scanned.
    Volumes {
        #[arg(long)]
        json: bool,
    },
    /// Browse a directory by size (prints the largest entries).
    Explore {
        /// Directory to inspect (default: home).
        path: Option<PathBuf>,
        /// Number of entries to show.
        #[arg(long, default_value_t = 30)]
        top: usize,
        #[arg(long)]
        json: bool,
    },
    /// Find the largest files under your home (or a directory), skipping protected locations.
    Large {
        path: Option<PathBuf>,
        /// Minimum file size in MB.
        #[arg(long, default_value_t = 256)]
        min_size: u64,
        #[arg(long, default_value_t = 40)]
        top: usize,
        #[arg(long)]
        json: bool,
    },
    /// Print or install the agent skill (Claude Code, Codex, Cursor) describing how to drive this CLI.
    Skill {
        /// Write the skill into ~/.claude/skills/cleanprojects/SKILL.md.
        #[arg(long)]
        install: bool,
    },
    /// Generate shell completions.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Args, Debug, Clone, Default)]
pub struct ScanArgs {
    /// Extra project root(s) to scan (repeatable).
    #[arg(short = 'r', long = "root", value_name = "DIR")]
    pub roots: Vec<PathBuf>,
    /// Only scan the given roots, ignore defaults and config roots.
    #[arg(long)]
    pub only_roots: bool,
    /// Skip git maintenance analysis.
    #[arg(long)]
    pub no_git: bool,
    /// Skip global cache rules.
    #[arg(long)]
    pub no_global: bool,
    /// Skip project artifact scan.
    #[arg(long)]
    pub no_projects: bool,
    /// Skip docker/pnpm/simctl probes.
    #[arg(long)]
    pub no_tools: bool,
    /// Skip installers, superseded versions and volume trash.
    #[arg(long)]
    pub no_extras: bool,
    /// Do not scan external volumes.
    #[arg(long)]
    pub no_volumes: bool,
    /// Minimum candidate size in MB.
    #[arg(long, value_name = "MB")]
    pub min_size: Option<u64>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct FilterArgs {
    /// Only these categories (repeatable): project, package-cache, build-cache, ide, browser, container, mobile, git, app-data, temp.
    #[arg(short = 'c', long = "category", value_name = "CAT")]
    pub categories: Vec<String>,
    /// Maximum risk to include: low, medium, high.
    #[arg(long, value_enum, default_value_t = RiskArg::Medium)]
    pub max_risk: RiskArg,
    /// Only artifacts of projects idle for the configured stale period.
    #[arg(long)]
    pub stale: bool,
    /// Only candidates whose label or path contains this text.
    #[arg(short = 'q', long, value_name = "TEXT")]
    pub query: Option<String>,
    /// Include candidates that were excluded earlier.
    #[arg(long)]
    pub include_excluded: bool,
    /// Specific candidate ids (from `scan`), repeatable.
    #[arg(long = "id", value_name = "ID")]
    pub ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq, Default)]
pub enum RiskArg {
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum ModeArg {
    Recovery,
    Trash,
    Permanent,
}

#[derive(Args, Debug, Clone)]
pub struct ScanCmd {
    #[command(flatten)]
    pub scan: ScanArgs,
    #[command(flatten)]
    pub filter: FilterArgs,
    /// Print JSON instead of a table.
    #[arg(long)]
    pub json: bool,
    /// Print one JSON object per line as candidates are found.
    #[arg(long, conflicts_with = "json")]
    pub json_stream: bool,
    /// Write a report to this file; the extension picks the format (.md, .json, .csv).
    #[arg(long, value_name = "FILE")]
    pub report: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct CleanCmd {
    #[command(flatten)]
    pub scan: ScanArgs,
    #[command(flatten)]
    pub filter: FilterArgs,
    /// Show what would be removed and exit.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
    /// Do not ask for confirmation.
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Delete permanently instead of using the recovery bin or trash.
    #[arg(long)]
    pub permanent: bool,
    /// Disposal mode: recovery (undoable), trash, permanent. Overrides the config.
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,
    /// Clean even when the owning application is running.
    #[arg(long)]
    pub force: bool,
    /// Print the result as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ConfigCmd {
    /// Print the config file path.
    #[arg(long)]
    pub path: bool,
    /// Write a config file with defaults (if missing).
    #[arg(long)]
    pub init: bool,
    /// Set a value: `--set clean.use_trash=false` (repeatable).
    #[arg(long = "set", value_name = "KEY=VALUE")]
    pub sets: Vec<String>,
    /// Reset to defaults.
    #[arg(long)]
    pub reset: bool,
}

impl Cli {
    pub fn lang_override(&self) -> Option<Lang> {
        self.lang.as_deref().and_then(Lang::parse)
    }
}
