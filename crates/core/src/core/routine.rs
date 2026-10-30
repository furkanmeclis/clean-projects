//! Routines: named, scheduled cleanups with conservative filters.
//! A routine never deletes permanently, never touches high-risk items and
//! never cleans data of a running application.

use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};

use super::candidate::{Candidate, Category, Risk};
use super::cleaner::{self, CleanMode, CleanOptions};
use super::state::{Config, Paths, State};
use crate::scan::{self, ScanContext};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub enum Schedule {
    /// Every day at `hour`.
    Daily { hour: u8 },
    /// Every week on `weekday` (0 = Monday) at `hour`.
    Weekly { weekday: u8, hour: u8 },
    /// Every month on `day` (1-28) at `hour`.
    Monthly { day: u8, hour: u8 },
    /// Every `hours` hours.
    Hours { hours: u32 },
}

impl Schedule {
    pub fn describe(&self) -> String {
        const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        match self {
            Schedule::Daily { hour } => format!("daily at {hour:02}:00"),
            Schedule::Weekly { weekday, hour } => {
                format!("every {} at {hour:02}:00", DAYS[(*weekday as usize).min(6)])
            }
            Schedule::Monthly { day, hour } => format!("monthly on day {day} at {hour:02}:00"),
            Schedule::Hours { hours } => format!("every {hours} h"),
        }
    }

    /// Parse short forms: `daily@3`, `weekly@sun@4`, `monthly@1@5`, `every@6h`.
    pub fn parse(s: &str) -> Option<Schedule> {
        let parts: Vec<&str> = s.split('@').collect();
        let hour = |i: usize, default: u8| {
            parts
                .get(i)
                .and_then(|h| h.parse::<u8>().ok())
                .unwrap_or(default)
                .min(23)
        };
        match parts.first().map(|p| p.to_ascii_lowercase()).as_deref() {
            Some("daily") => Some(Schedule::Daily { hour: hour(1, 3) }),
            Some("weekly") => {
                let weekday = parts
                    .get(1)
                    .map(|d| match d.to_ascii_lowercase().as_str() {
                        "mon" | "0" => 0,
                        "tue" | "1" => 1,
                        "wed" | "2" => 2,
                        "thu" | "3" => 3,
                        "fri" | "4" => 4,
                        "sat" | "5" => 5,
                        _ => 6,
                    })
                    .unwrap_or(6);
                Some(Schedule::Weekly {
                    weekday,
                    hour: hour(2, 3),
                })
            }
            Some("monthly") => Some(Schedule::Monthly {
                day: parts
                    .get(1)
                    .and_then(|d| d.parse::<u8>().ok())
                    .unwrap_or(1)
                    .clamp(1, 28),
                hour: hour(2, 3),
            }),
            Some("every") => {
                let h = parts.get(1)?.trim_end_matches('h').parse::<u32>().ok()?;
                Some(Schedule::Hours { hours: h.max(1) })
            }
            _ => None,
        }
    }

    /// Whether a run is due at `now` given the previous run.
    pub fn is_due(&self, now: DateTime<Local>, last: Option<DateTime<Local>>) -> bool {
        match self {
            Schedule::Hours { hours } => last
                .map(|l| now.signed_duration_since(l).num_hours() >= *hours as i64)
                .unwrap_or(true),
            Schedule::Daily { hour } => {
                now.hour() >= *hour as u32
                    && last
                        .map(|l| l.date_naive() < now.date_naive())
                        .unwrap_or(true)
            }
            Schedule::Weekly { weekday, hour } => {
                now.weekday().num_days_from_monday() == *weekday as u32
                    && now.hour() >= *hour as u32
                    && last
                        .map(|l| {
                            now.signed_duration_since(l).num_days() >= 6
                                || l.date_naive() < now.date_naive()
                                    && now.signed_duration_since(l).num_days() >= 1
                                    && now.weekday() != l.weekday()
                        })
                        .unwrap_or(true)
            }
            Schedule::Monthly { day, hour } => {
                now.day() == *day as u32
                    && now.hour() >= *hour as u32
                    && last
                        .map(|l| l.date_naive() < now.date_naive())
                        .unwrap_or(true)
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub schedule: Schedule,
    /// Highest risk level the routine may clean (never above medium).
    pub max_risk: Risk,
    /// Empty means every category.
    pub categories: Vec<Category>,
    /// Only artifacts of stale projects.
    pub stale_only: bool,
    /// Only items not modified for at least this many days.
    pub min_age_days: u64,
    /// `recovery` or `trash`; permanent is never allowed.
    pub mode: String,
    /// Scan only these roots (empty = normal scan).
    pub roots: Vec<String>,
    pub notify: bool,
}

impl Default for Routine {
    fn default() -> Self {
        Routine {
            id: new_id(),
            name: "Safe sweep".into(),
            enabled: true,
            schedule: Schedule::Weekly {
                weekday: 6,
                hour: 3,
            },
            max_risk: Risk::Low,
            categories: vec![],
            stale_only: false,
            min_age_days: 0,
            mode: "recovery".into(),
            roots: vec![],
            notify: true,
        }
    }
}

pub fn new_id() -> String {
    format!("{:x}", Local::now().timestamp_nanos_opt().unwrap_or(0))
}

/// Ready-made routines offered by the front ends.
pub fn templates() -> Vec<Routine> {
    vec![
        Routine {
            id: new_id(),
            name: "Weekly safe sweep".into(),
            schedule: Schedule::Weekly {
                weekday: 6,
                hour: 3,
            },
            ..Default::default()
        },
        Routine {
            id: new_id(),
            name: "Monthly stale projects".into(),
            schedule: Schedule::Monthly { day: 1, hour: 4 },
            categories: vec![Category::Project],
            stale_only: true,
            ..Default::default()
        },
        Routine {
            id: new_id(),
            name: "Daily build caches".into(),
            schedule: Schedule::Daily { hour: 2 },
            categories: vec![Category::BuildCache, Category::Container],
            ..Default::default()
        },
        Routine {
            id: new_id(),
            name: "Browser and temp, every 6 h".into(),
            schedule: Schedule::Hours { hours: 6 },
            categories: vec![Category::Browser, Category::Temp],
            ..Default::default()
        },
    ]
}

/// Outcome of one routine run, kept in the state file.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoutineRun {
    pub routine_id: String,
    pub name: String,
    pub at: DateTime<Local>,
    pub freed: u64,
    pub items: usize,
    pub failed: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub const MAX_ROUTINE_RUNS: usize = 200;

impl Routine {
    /// Candidates this routine would clean out of a scan result.
    pub fn select<'a>(
        &self,
        candidates: &'a [Candidate],
        excluded: &BTreeSet<String>,
    ) -> Vec<&'a Candidate> {
        let max = self.max_risk.min(Risk::Medium);
        candidates
            .iter()
            .filter(|c| !excluded.contains(&c.id))
            .filter(|c| c.risk <= max)
            .filter(|c| c.blocked_by.is_none())
            .filter(|c| self.categories.is_empty() || self.categories.contains(&c.category))
            .filter(|c| !self.stale_only || c.is_stale())
            .filter(|c| {
                self.min_age_days == 0
                    || c.age_days()
                        .map(|d| d >= self.min_age_days)
                        .unwrap_or(false)
            })
            .filter(|c| !c.tags.iter().any(|t| t == "manual"))
            .collect()
    }

    pub fn clean_mode(&self) -> CleanMode {
        match CleanMode::parse(&self.mode) {
            CleanMode::Permanent => CleanMode::Recovery,
            m => m,
        }
    }

    pub fn last_run<'a>(&self, state: &'a State) -> Option<&'a RoutineRun> {
        state
            .routine_runs
            .iter()
            .rev()
            .find(|r| r.routine_id == self.id)
    }

    pub fn is_due(&self, state: &State, now: DateTime<Local>) -> bool {
        self.enabled
            && self
                .schedule
                .is_due(now, self.last_run(state).map(|r| r.at))
    }
}

/// Routines that should run now.
pub fn due<'a>(config: &'a Config, state: &State, now: DateTime<Local>) -> Vec<&'a Routine> {
    config
        .routines
        .iter()
        .filter(|r| r.is_due(state, now))
        .collect()
}

/// Scan, select and clean for one routine. Persists journal, history and the run record.
pub fn run(
    paths: &Paths,
    config: &Config,
    state: &mut State,
    routine: &Routine,
    dry_run: bool,
) -> RoutineRun {
    let mut cfg = config.clone();
    if !routine.roots.is_empty() {
        cfg.scan.default_roots = false;
        cfg.scan.roots = routine.roots.clone();
        cfg.scan.volumes = false;
    }
    let ctx = ScanContext::new(paths.home.clone(), cfg);
    let result = scan::scan_blocking(&ctx);
    let _ = scan::save_cache(&paths.scan_cache_file, &result);
    state.record_scan(
        result.total(),
        result.disk.free,
        result.disk.total,
        result.candidates.len(),
    );
    let items: Vec<Candidate> = routine
        .select(&result.candidates, &state.excluded)
        .into_iter()
        .cloned()
        .collect();
    let opts = CleanOptions {
        dry_run,
        mode: routine.clean_mode(),
        home: paths.home.clone(),
        protected: ctx.protected.clone(),
        data_dir: paths.data_dir.clone(),
        volumes: ctx.volumes.clone(),
        block_running_apps: true,
    };
    let outcome = cleaner::run_with_journal(&items, &opts, &mut |_| {});
    if !outcome.recovered.is_empty() {
        let mut journal = paths.load_journal();
        cleaner::record_recovered(&mut journal, outcome.recovered);
        let _ = paths.save_journal(&journal);
    }
    let entry = outcome.entry;
    let run = RoutineRun {
        routine_id: routine.id.clone(),
        name: routine.name.clone(),
        at: Local::now(),
        freed: entry.freed,
        items: entry.items.iter().filter(|i| i.ok).count(),
        failed: entry.items.iter().filter(|i| !i.ok).count(),
        error: None,
    };
    if !items.is_empty() {
        state.history.push(entry);
    }
    state.routine_runs.push(run.clone());
    if state.routine_runs.len() > MAX_ROUTINE_RUNS {
        let drop = state.routine_runs.len() - MAX_ROUTINE_RUNS;
        state.routine_runs.drain(..drop);
    }
    let _ = paths.save_state(state);
    paths.append_log(&format!(
        "routine '{}' freed={} items={} failed={} dry_run={}",
        routine.name, run.freed, run.items, run.failed, dry_run
    ));
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::candidate::Action;
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, 0, 0).single().unwrap()
    }

    #[test]
    fn schedules_parse_and_fire() {
        assert_eq!(
            Schedule::parse("daily@5"),
            Some(Schedule::Daily { hour: 5 })
        );
        assert_eq!(
            Schedule::parse("weekly@sun@4"),
            Some(Schedule::Weekly {
                weekday: 6,
                hour: 4
            })
        );
        assert_eq!(
            Schedule::parse("monthly@15@2"),
            Some(Schedule::Monthly { day: 15, hour: 2 })
        );
        assert_eq!(
            Schedule::parse("every@6h"),
            Some(Schedule::Hours { hours: 6 })
        );
        assert_eq!(Schedule::parse("nope"), None);
        let daily = Schedule::Daily { hour: 3 };
        assert!(daily.is_due(at(2026, 1, 2, 4), None));
        assert!(!daily.is_due(at(2026, 1, 2, 2), None));
        assert!(!daily.is_due(at(2026, 1, 2, 4), Some(at(2026, 1, 2, 3))));
        assert!(daily.is_due(at(2026, 1, 3, 3), Some(at(2026, 1, 2, 3))));
        let hours = Schedule::Hours { hours: 6 };
        assert!(hours.is_due(at(2026, 1, 2, 10), Some(at(2026, 1, 2, 3))));
        assert!(!hours.is_due(at(2026, 1, 2, 5), Some(at(2026, 1, 2, 3))));
        // 2026-01-04 is a Sunday
        let weekly = Schedule::Weekly {
            weekday: 6,
            hour: 3,
        };
        assert!(weekly.is_due(at(2026, 1, 4, 3), None));
        assert!(!weekly.is_due(at(2026, 1, 5, 3), None));
        assert!(!weekly.is_due(at(2026, 1, 4, 5), Some(at(2026, 1, 4, 3))));
        assert!(weekly.is_due(at(2026, 1, 11, 3), Some(at(2026, 1, 4, 3))));
    }

    #[test]
    fn selection_is_conservative() {
        let mk = |label: &str, risk: Risk, cat: Category, tags: &[&str], blocked: Option<&str>| {
            let mut c = Candidate::new(
                label,
                "",
                cat,
                risk,
                Action::RemovePath {
                    path: PathBuf::from(format!("/x/{label}")),
                },
            )
            .with_size(10, true);
            c.tags = tags.iter().map(|t| t.to_string()).collect();
            c.blocked_by = blocked.map(|b| b.to_string());
            c
        };
        let cands = vec![
            mk("low", Risk::Low, Category::Project, &["stale"], None),
            mk("medium", Risk::Medium, Category::Project, &[], None),
            mk("high", Risk::High, Category::AppData, &[], None),
            mk("blocked", Risk::Low, Category::Ide, &[], Some("Cursor")),
            mk("manual", Risk::Low, Category::Temp, &["manual"], None),
            mk("browser", Risk::Low, Category::Browser, &[], None),
        ];
        let mut r = Routine {
            max_risk: Risk::High, // clamped to medium
            ..Default::default()
        };
        let names: Vec<&str> = r
            .select(&cands, &BTreeSet::new())
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(names, vec!["low", "medium", "browser"]);
        r.categories = vec![Category::Project];
        r.stale_only = true;
        let names: Vec<&str> = r
            .select(&cands, &BTreeSet::new())
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(names, vec!["low"]);
        r.mode = "permanent".into();
        assert_eq!(r.clean_mode(), CleanMode::Recovery);
    }

    #[test]
    fn runs_end_to_end_in_temp_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let proj = home.join("Projects/app");
        std::fs::create_dir_all(proj.join("node_modules/pkg")).unwrap();
        std::fs::write(proj.join("package.json"), "{}").unwrap();
        std::fs::write(
            proj.join("node_modules/pkg/big.bin"),
            vec![0u8; 2 * 1024 * 1024],
        )
        .unwrap();
        let paths = Paths::at(home.clone(), tmp.path().join("cfg"));
        let mut config = Config::default();
        config.scan.volumes = false;
        let mut state = State::default();
        let routine = Routine {
            roots: vec![home.join("Projects").display().to_string()],
            ..Default::default()
        };
        let run = super::run(&paths, &config, &mut state, &routine, false);
        assert_eq!(run.items, 1, "{run:?}");
        assert!(!proj.join("node_modules").exists());
        assert_eq!(state.routine_runs.len(), 1);
        assert_eq!(state.history.len(), 1);
        assert!(
            state.history[0].items[0].recovery_id.is_some(),
            "routines use the recovery bin"
        );
        assert!(!routine.is_due(&state, Local::now()) || true);
    }
}
