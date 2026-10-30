//! Configuration (`config.toml`) and mutable state (`state.json`:
//! exclusions and cleanup history), stored in the OS config directory.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};

use super::i18n::Lang;

pub const APP_DIR_NAME: &str = "cleanprojects";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ScanConfig {
    /// Extra project roots. `~` is expanded.
    pub roots: Vec<String>,
    /// Also scan the built-in default roots (Documents, Projects, dev, ...).
    pub default_roots: bool,
    /// How deep to look for projects below a root.
    pub max_depth: usize,
    /// Candidates smaller than this are hidden.
    pub min_size_mb: u64,
    /// A project idle for this many days is tagged `stale`.
    pub stale_days: u64,
    /// Additional protected locations. `~` is expanded.
    pub protected: Vec<String>,
    /// Disable individual global rules by label substring.
    pub disabled_rules: Vec<String>,
    /// Also scan mounted external volumes (SSDs, USB drives) for projects.
    pub volumes: bool,
    /// Mount points to leave alone even when `volumes` is on.
    pub skip_volumes: Vec<String>,
    /// Treat installer files in Downloads older than this many days as candidates.
    pub installer_days: u64,
}

impl Default for ScanConfig {
    fn default() -> Self {
        ScanConfig {
            roots: Vec::new(),
            default_roots: true,
            max_depth: 6,
            min_size_mb: 1,
            stale_days: 30,
            protected: Vec::new(),
            disabled_rules: Vec::new(),
            volumes: true,
            skip_volumes: Vec::new(),
            installer_days: 7,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GitConfig {
    /// Inspect repositories for prunable / stale worktrees.
    pub enabled: bool,
    /// Offer deletion of local branches already merged into the default branch.
    pub merged_branches: bool,
    /// Offer `git gc` when a repository has many loose objects.
    pub gc: bool,
    /// Worktrees idle for fewer days than this are never offered.
    pub worktree_idle_days: u64,
}

impl Default for GitConfig {
    fn default() -> Self {
        GitConfig {
            enabled: true,
            merged_branches: false,
            gc: true,
            worktree_idle_days: 7,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CleanConfig {
    /// Legacy switch kept for old config files; `mode` wins when present.
    pub use_trash: bool,
    /// recovery (undoable bin, default), trash (OS trash) or permanent.
    pub mode: String,
    /// Items in the recovery bin older than this are purged automatically.
    pub recovery_days: u64,
    /// Refuse to clean items whose owning application is running.
    pub block_running_apps: bool,
}

impl Default for CleanConfig {
    fn default() -> Self {
        CleanConfig {
            use_trash: true,
            mode: "recovery".into(),
            recovery_days: 14,
            block_running_apps: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiConfig {
    pub language: Lang,
    /// Desktop: keep running in the system tray when the window is closed.
    pub close_to_tray: bool,
    /// Desktop: rescan in the background every N hours (0 = off).
    pub scan_interval_hours: u64,
    /// Desktop: notify when recoverable space exceeds this many GB (0 = always after a background scan).
    pub notify_threshold_gb: u64,
    /// Desktop: check GitHub releases for updates on launch.
    pub check_updates: bool,
    /// Desktop: system, light or dark.
    pub theme: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            language: Lang::default(),
            close_to_tray: true,
            scan_interval_hours: 24,
            notify_threshold_gb: 5,
            check_updates: true,
            theme: "system".into(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub scan: ScanConfig,
    pub git: GitConfig,
    pub clean: CleanConfig,
    pub ui: UiConfig,
}

impl Config {
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_toml(s: &str) -> Result<Config> {
        toml::from_str(s).context("invalid config.toml")
    }

    /// Set a dotted key such as `clean.use_trash=false` or `scan.stale_days=45`.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let v = value.trim();
        let b = |v: &str| -> Result<bool> {
            match v.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Ok(true),
                "false" | "0" | "no" | "off" => Ok(false),
                _ => anyhow::bail!("expected a boolean, got '{v}'"),
            }
        };
        let n = |v: &str| -> Result<u64> { v.parse::<u64>().context("expected a number") };
        match key {
            "scan.default_roots" => self.scan.default_roots = b(v)?,
            "scan.max_depth" => self.scan.max_depth = n(v)? as usize,
            "scan.min_size_mb" => self.scan.min_size_mb = n(v)?,
            "scan.stale_days" => self.scan.stale_days = n(v)?,
            "scan.roots" => self.scan.roots = split_list(v),
            "scan.protected" => self.scan.protected = split_list(v),
            "scan.disabled_rules" => self.scan.disabled_rules = split_list(v),
            "scan.volumes" => self.scan.volumes = b(v)?,
            "scan.skip_volumes" => self.scan.skip_volumes = split_list(v),
            "scan.installer_days" => self.scan.installer_days = n(v)?,
            "git.enabled" => self.git.enabled = b(v)?,
            "git.merged_branches" => self.git.merged_branches = b(v)?,
            "git.gc" => self.git.gc = b(v)?,
            "git.worktree_idle_days" => self.git.worktree_idle_days = n(v)?,
            "clean.use_trash" => {
                self.clean.use_trash = b(v)?;
                self.clean.mode = if self.clean.use_trash {
                    "trash".into()
                } else {
                    "permanent".into()
                };
            }
            "clean.mode" => {
                if !["recovery", "trash", "permanent"].contains(&v.to_ascii_lowercase().as_str()) {
                    anyhow::bail!("expected recovery, trash or permanent");
                }
                self.clean.mode = v.to_ascii_lowercase();
            }
            "clean.recovery_days" => self.clean.recovery_days = n(v)?,
            "clean.block_running_apps" => self.clean.block_running_apps = b(v)?,
            "ui.close_to_tray" => self.ui.close_to_tray = b(v)?,
            "ui.scan_interval_hours" => self.ui.scan_interval_hours = n(v)?,
            "ui.notify_threshold_gb" => self.ui.notify_threshold_gb = n(v)?,
            "ui.check_updates" => self.ui.check_updates = b(v)?,
            "ui.theme" => {
                if !["system", "light", "dark"].contains(&v.to_ascii_lowercase().as_str()) {
                    anyhow::bail!("expected system, light or dark");
                }
                self.ui.theme = v.to_ascii_lowercase();
            }
            "ui.language" => self.ui.language = Lang::parse(v).context("expected 'en' or 'tr'")?,
            _ => anyhow::bail!("unknown config key '{key}'"),
        }
        Ok(())
    }

    pub fn keys() -> &'static [&'static str] {
        &[
            "scan.roots",
            "scan.default_roots",
            "scan.max_depth",
            "scan.min_size_mb",
            "scan.stale_days",
            "scan.protected",
            "scan.disabled_rules",
            "scan.volumes",
            "scan.skip_volumes",
            "scan.installer_days",
            "git.enabled",
            "git.merged_branches",
            "git.gc",
            "git.worktree_idle_days",
            "clean.use_trash",
            "clean.mode",
            "clean.recovery_days",
            "clean.block_running_apps",
            "ui.language",
            "ui.close_to_tray",
            "ui.scan_interval_hours",
            "ui.notify_threshold_gb",
            "ui.check_updates",
            "ui.theme",
        ]
    }
}

fn split_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HistoryItem {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub size: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Set when the item sits in the recovery bin and can be restored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub at: DateTime<Local>,
    pub freed: u64,
    pub dry_run: bool,
    pub trashed: bool,
    /// recovery, trash or permanent.
    #[serde(default)]
    pub mode: String,
    pub items: Vec<HistoryItem>,
}

impl HistoryEntry {
    pub fn restorable(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.recovery_id.is_some())
            .count()
    }
}

/// One point of the disk trend, recorded after every scan.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScanSnapshot {
    pub at: DateTime<Local>,
    pub recoverable: u64,
    pub free: u64,
    pub total: u64,
    pub candidates: usize,
}

pub const MAX_SNAPSHOTS: usize = 90;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct State {
    pub excluded: BTreeSet<String>,
    pub history: Vec<HistoryEntry>,
    pub last_scan_at: Option<DateTime<Local>>,
    pub last_recoverable: u64,
    /// Trend of recoverable and free space, newest last.
    pub scans: Vec<ScanSnapshot>,
}

impl State {
    /// Record a finished scan in the trend (at most one point per 10 minutes).
    pub fn record_scan(&mut self, recoverable: u64, free: u64, total: u64, candidates: usize) {
        let now = Local::now();
        if let Some(last) = self.scans.last_mut() {
            if now.signed_duration_since(last.at).num_minutes() < 10 {
                *last = ScanSnapshot {
                    at: now,
                    recoverable,
                    free,
                    total,
                    candidates,
                };
                self.last_scan_at = Some(now);
                self.last_recoverable = recoverable;
                return;
            }
        }
        self.scans.push(ScanSnapshot {
            at: now,
            recoverable,
            free,
            total,
            candidates,
        });
        if self.scans.len() > MAX_SNAPSHOTS {
            let drop = self.scans.len() - MAX_SNAPSHOTS;
            self.scans.drain(..drop);
        }
        self.last_scan_at = Some(now);
        self.last_recoverable = recoverable;
    }

    pub fn toggle_excluded(&mut self, id: &str) -> bool {
        if self.excluded.remove(id) {
            false
        } else {
            self.excluded.insert(id.to_string());
            true
        }
    }

    pub fn total_freed(&self) -> u64 {
        self.history
            .iter()
            .filter(|h| !h.dry_run)
            .map(|h| h.freed)
            .sum()
    }
}

/// Resolved locations on disk.
#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub state_file: PathBuf,
    pub log_file: PathBuf,
    /// Larger, machine-local data: recovery bin and scan cache.
    pub data_dir: PathBuf,
    pub journal_file: PathBuf,
    pub scan_cache_file: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Paths> {
        let base = BaseDirs::new().context("cannot determine home directory")?;
        let home = base.home_dir().to_path_buf();
        let (config_dir, data_dir) = match std::env::var_os("CLEANPROJECTS_CONFIG_DIR") {
            Some(dir) => {
                let d = PathBuf::from(dir);
                (d.clone(), d.join("data"))
            }
            None => match ProjectDirs::from("", "", APP_DIR_NAME) {
                Some(p) => (
                    p.config_dir().to_path_buf(),
                    p.data_local_dir().to_path_buf(),
                ),
                None => (
                    home.join(".config").join(APP_DIR_NAME),
                    home.join(".local/share").join(APP_DIR_NAME),
                ),
            },
        };
        Ok(Paths::at_with_data(home, config_dir, data_dir))
    }

    pub fn at(home: PathBuf, config_dir: PathBuf) -> Paths {
        let data_dir = config_dir.join("data");
        Paths::at_with_data(home, config_dir, data_dir)
    }

    pub fn at_with_data(home: PathBuf, config_dir: PathBuf, data_dir: PathBuf) -> Paths {
        Paths {
            config_file: config_dir.join("config.toml"),
            state_file: config_dir.join("state.json"),
            log_file: config_dir.join("cleanprojects.log"),
            journal_file: data_dir.join("recovery.json"),
            scan_cache_file: data_dir.join("last-scan.json"),
            home,
            config_dir,
            data_dir,
        }
    }

    pub fn load_journal(&self) -> super::recovery::Journal {
        super::recovery::Journal::load(&self.journal_file)
    }

    pub fn save_journal(&self, journal: &super::recovery::Journal) -> Result<()> {
        journal.save(&self.journal_file)
    }

    pub fn load_config(&self) -> Result<Config> {
        match fs::read_to_string(&self.config_file) {
            Ok(s) => Config::from_toml(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).context("reading config.toml"),
        }
    }

    pub fn save_config(&self, config: &Config) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        write_atomic(&self.config_file, config.to_toml().as_bytes())
    }

    pub fn load_state(&self) -> Result<State> {
        match fs::read_to_string(&self.state_file) {
            Ok(s) => serde_json::from_str(&s).context("invalid state.json"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e).context("reading state.json"),
        }
    }

    pub fn save_state(&self, state: &State) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        let json = serde_json::to_vec_pretty(state)?;
        write_atomic(&self.state_file, &json)
    }

    pub fn append_log(&self, line: &str) {
        use std::io::Write;
        if fs::create_dir_all(&self.config_dir).is_err() {
            return;
        }
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
        {
            let _ = writeln!(f, "{} {}", Local::now().format("%Y-%m-%d %H:%M:%S"), line);
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_and_set() {
        let mut c = Config::default();
        c.set("clean.use_trash", "false").unwrap();
        c.set("scan.stale_days", "45").unwrap();
        c.set("scan.roots", "~/dev, ~/work").unwrap();
        c.set("ui.language", "tr").unwrap();
        assert!(c.set("nope.key", "1").is_err());
        assert!(c.set("git.gc", "maybe").is_err());
        let text = c.to_toml();
        let back = Config::from_toml(&text).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.scan.roots, vec!["~/dev", "~/work"]);
        assert_eq!(back.ui.language, Lang::Tr);
    }

    #[test]
    fn state_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path().join("home"), dir.path().join("cfg"));
        assert_eq!(paths.load_state().unwrap(), State::default());
        let mut st = State::default();
        assert!(st.toggle_excluded("abc"));
        st.history.push(HistoryEntry {
            at: Local::now(),
            freed: 10,
            dry_run: false,
            trashed: true,
            mode: "trash".into(),
            items: vec![],
        });
        st.record_scan(100, 50, 200, 3);
        st.record_scan(120, 40, 200, 4);
        assert_eq!(st.scans.len(), 1, "points within 10 minutes collapse");
        assert_eq!(st.scans[0].recoverable, 120);
        paths.save_state(&st).unwrap();
        let back = paths.load_state().unwrap();
        assert!(back.excluded.contains("abc"));
        assert_eq!(back.total_freed(), 10);
        assert_eq!(back.scans.len(), 1);
        paths.save_config(&Config::default()).unwrap();
        assert_eq!(paths.load_config().unwrap(), Config::default());
    }
}
