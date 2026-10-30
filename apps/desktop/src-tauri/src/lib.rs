//! Tauri backend: thin command layer over `cleanprojects-core`.
//! Long-running work (scan, clean) runs on threads and streams events to the webview.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use cleanprojects_core::cleaner::{self, CleanEvent, CleanOptions};
use cleanprojects_core::fsutil::{compact_path, human_size};
use cleanprojects_core::scan::{self, ScanContext, ScanEvent, ScanResult};
use cleanprojects_core::{disk_for, Candidate, Config, DiskInfo, HistoryEntry, Paths, State};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State as TauriState};

pub struct AppState {
    paths: Paths,
    config: Mutex<Config>,
    state: Mutex<State>,
    last_scan: Mutex<Option<ScanResult>>,
    scanning: AtomicBool,
    cleaning: AtomicBool,
}

#[derive(Serialize)]
struct Bootstrap {
    version: String,
    os: String,
    home: String,
    config_path: String,
    config: Config,
    excluded: Vec<String>,
    history: Vec<HistoryEntry>,
    disk: DiskInfo,
    roots: Vec<String>,
    protected: Vec<String>,
    tools: Vec<(String, bool)>,
    candidates: Vec<Candidate>,
    scanning: bool,
}

#[derive(Serialize, Clone)]
struct ScanSummary {
    candidates: usize,
    total: u64,
    projects: usize,
    repos: usize,
    duration_ms: u128,
    disk: DiskInfo,
}

#[derive(Serialize, Clone)]
struct CleanDone {
    entry: HistoryEntry,
    disk: DiskInfo,
    removed_ids: Vec<String>,
}

#[derive(Deserialize, Default)]
pub struct ScanOptions {
    #[serde(default)]
    pub no_git: bool,
    #[serde(default)]
    pub no_tools: bool,
    #[serde(default)]
    pub no_projects: bool,
    #[serde(default)]
    pub no_global: bool,
}

#[derive(Deserialize)]
pub struct CleanRequest {
    pub ids: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
    pub use_trash: Option<bool>,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

#[tauri::command]
fn bootstrap(app: AppHandle, st: TauriState<AppState>) -> CmdResult<Bootstrap> {
    let config = st.config.lock().unwrap().clone();
    let state = st.state.lock().unwrap().clone();
    let ctx = ScanContext::new(st.paths.home.clone(), config.clone());
    let candidates = st
        .last_scan
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.candidates.clone())
        .unwrap_or_default();
    let _ = app;
    Ok(Bootstrap {
        version: cleanprojects_core::VERSION.to_string(),
        os: std::env::consts::OS.to_string(),
        home: st.paths.home.display().to_string(),
        config_path: st.paths.config_file.display().to_string(),
        config,
        excluded: state.excluded.iter().cloned().collect(),
        history: state.history,
        disk: disk_for(&st.paths.home),
        roots: ctx
            .roots
            .iter()
            .map(|p| compact_path(p, &st.paths.home))
            .collect(),
        protected: ctx
            .protected
            .iter()
            .filter(|p| p.exists())
            .map(|p| compact_path(p, &st.paths.home))
            .collect(),
        tools: scan::tools::tool_report()
            .into_iter()
            .map(|(t, ok)| (t.to_string(), ok))
            .collect(),
        candidates,
        scanning: st.scanning.load(Ordering::SeqCst),
    })
}

#[tauri::command]
fn start_scan(
    app: AppHandle,
    st: TauriState<AppState>,
    options: Option<ScanOptions>,
) -> CmdResult<()> {
    if st.scanning.swap(true, Ordering::SeqCst) {
        return Err("a scan is already running".into());
    }
    let options = options.unwrap_or_default();
    let config = st.config.lock().unwrap().clone();
    let mut ctx = ScanContext::new(st.paths.home.clone(), config);
    if options.no_git {
        ctx.include_git = false;
    }
    ctx.include_tools = !options.no_tools;
    ctx.include_projects = !options.no_projects;
    ctx.include_global = !options.no_global;
    let app2 = app.clone();
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx2 = ctx.clone();
        let worker = std::thread::spawn(move || scan::scan(&ctx2, tx));
        for ev in rx {
            match ev {
                ScanEvent::Progress { phase, message } => {
                    let _ = app2.emit(
                        "scan://progress",
                        serde_json::json!({ "phase": phase, "message": message }),
                    );
                }
                ScanEvent::Found(c) => {
                    let _ = app2.emit("scan://found", *c);
                }
                ScanEvent::Done(_) => {}
            }
        }
        let result = worker.join().unwrap_or_else(|_| ScanResult {
            candidates: vec![],
            disk: DiskInfo::default(),
            roots: vec![],
            protected: vec![],
            projects: 0,
            repos: 0,
            started_at: chrono::Local::now(),
            duration_ms: 0,
        });
        let st = app2.state::<AppState>();
        let summary = ScanSummary {
            candidates: result.candidates.len(),
            total: result.total(),
            projects: result.projects,
            repos: result.repos,
            duration_ms: result.duration_ms,
            disk: result.disk.clone(),
        };
        {
            let mut s = st.state.lock().unwrap();
            s.last_scan_at = Some(result.started_at);
            s.last_recoverable = result.total();
            let _ = st.paths.save_state(&s);
        }
        *st.last_scan.lock().unwrap() = Some(result);
        st.scanning.store(false, Ordering::SeqCst);
        let _ = app2.emit("scan://done", summary);
    });
    Ok(())
}

#[tauri::command]
fn get_candidates(st: TauriState<AppState>) -> Vec<Candidate> {
    st.last_scan
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| r.candidates.clone())
        .unwrap_or_default()
}

#[tauri::command]
fn start_clean(app: AppHandle, st: TauriState<AppState>, request: CleanRequest) -> CmdResult<()> {
    if st.cleaning.swap(true, Ordering::SeqCst) {
        return Err("a cleanup is already running".into());
    }
    let wanted: HashSet<String> = request.ids.into_iter().collect();
    let items: Vec<Candidate> = st
        .last_scan
        .lock()
        .unwrap()
        .as_ref()
        .map(|r| {
            r.candidates
                .iter()
                .filter(|c| wanted.contains(&c.id))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if items.is_empty() {
        st.cleaning.store(false, Ordering::SeqCst);
        return Err("nothing selected".into());
    }
    let config = st.config.lock().unwrap().clone();
    let ctx = ScanContext::new(st.paths.home.clone(), config.clone());
    let opts = CleanOptions {
        dry_run: request.dry_run,
        use_trash: request.use_trash.unwrap_or(config.clean.use_trash),
        home: st.paths.home.clone(),
        protected: ctx.protected,
    };
    let app2 = app.clone();
    std::thread::spawn(move || {
        let app3 = app2.clone();
        let entry = cleaner::run(&items, &opts, |ev| match ev {
            CleanEvent::Start {
                index,
                total,
                id,
                label,
            } => {
                let _ = app3.emit(
                    "clean://start",
                    serde_json::json!({ "index": index, "total": total, "id": id, "label": label }),
                );
            }
            CleanEvent::Log(line) => {
                let _ = app3.emit("clean://log", line);
            }
            CleanEvent::Item {
                id,
                ok,
                freed,
                error,
            } => {
                let _ = app3.emit("clean://item", serde_json::json!({ "id": id, "ok": ok, "freed": freed, "freed_human": human_size(freed), "error": error }));
            }
            CleanEvent::Done { .. } => {}
        });
        let st = app2.state::<AppState>();
        let removed_ids: Vec<String> = if entry.dry_run {
            vec![]
        } else {
            entry
                .items
                .iter()
                .filter(|i| i.ok)
                .map(|i| i.id.clone())
                .collect()
        };
        if let Some(scan) = st.last_scan.lock().unwrap().as_mut() {
            scan.candidates.retain(|c| !removed_ids.contains(&c.id));
        }
        {
            let mut s = st.state.lock().unwrap();
            s.history.push(entry.clone());
            let _ = st.paths.save_state(&s);
        }
        st.paths.append_log(&format!(
            "clean dry_run={} trash={} freed={} items={}",
            entry.dry_run,
            entry.trashed,
            entry.freed,
            entry.items.len()
        ));
        st.cleaning.store(false, Ordering::SeqCst);
        let _ = app2.emit(
            "clean://done",
            CleanDone {
                entry,
                disk: disk_for(&st.paths.home),
                removed_ids,
            },
        );
    });
    Ok(())
}

#[tauri::command]
fn toggle_exclude(st: TauriState<AppState>, id: String) -> CmdResult<bool> {
    let mut s = st.state.lock().unwrap();
    let now_excluded = s.toggle_excluded(&id);
    st.paths.save_state(&s).map_err(err)?;
    Ok(now_excluded)
}

#[tauri::command]
fn save_config(st: TauriState<AppState>, config: Config) -> CmdResult<Config> {
    st.paths.save_config(&config).map_err(err)?;
    *st.config.lock().unwrap() = config.clone();
    Ok(config)
}

#[tauri::command]
fn get_history(st: TauriState<AppState>) -> Vec<HistoryEntry> {
    st.state.lock().unwrap().history.clone()
}

#[tauri::command]
fn clear_history(st: TauriState<AppState>) -> CmdResult<()> {
    let mut s = st.state.lock().unwrap();
    s.history.clear();
    st.paths.save_state(&s).map_err(err)
}

#[tauri::command]
fn write_report(st: TauriState<AppState>, path: String, ids: Vec<String>) -> CmdResult<String> {
    let wanted: HashSet<String> = ids.into_iter().collect();
    let scan = st.last_scan.lock().unwrap();
    let Some(result) = scan.as_ref() else {
        return Err("run a scan first".into());
    };
    let items: Vec<&Candidate> = result
        .candidates
        .iter()
        .filter(|c| wanted.is_empty() || wanted.contains(&c.id))
        .collect();
    let mut s = String::new();
    s.push_str("# CleanProjects report\n\n");
    s.push_str(&format!(
        "Scanned {} · {} projects · {} repositories · disk {} free of {}\n\n",
        result.started_at.format("%Y-%m-%d %H:%M"),
        result.projects,
        result.repos,
        human_size(result.disk.free),
        human_size(result.disk.total)
    ));
    s.push_str("| Size | Risk | Category | Item | Path / command |\n|---:|---|---|---|---|\n");
    for c in &items {
        s.push_str(&format!(
            "| {} | {} | {} | {} | `{}` |\n",
            human_size(c.size),
            c.risk.as_str(),
            c.category.as_str(),
            c.label.replace('|', "\\|"),
            compact_path(std::path::Path::new(&c.detail()), &st.paths.home)
        ));
    }
    let total: u64 = items.iter().map(|c| c.size).sum();
    s.push_str(&format!("\n**Total recoverable: {}**\n", human_size(total)));
    std::fs::write(&path, s).map_err(err)?;
    Ok(path)
}

#[tauri::command]
fn path_exists(path: String) -> bool {
    PathBuf::from(path).exists()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = Paths::discover().expect("cannot resolve home directory");
    let config = paths.load_config().unwrap_or_default();
    let state = paths.load_state().unwrap_or_default();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            paths,
            config: Mutex::new(config),
            state: Mutex::new(state),
            last_scan: Mutex::new(None),
            scanning: AtomicBool::new(false),
            cleaning: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            start_scan,
            get_candidates,
            start_clean,
            toggle_exclude,
            save_config,
            get_history,
            clear_history,
            write_report,
            path_exists
        ])
        .run(tauri::generate_context!())
        .expect("error while running CleanProjects");
}
