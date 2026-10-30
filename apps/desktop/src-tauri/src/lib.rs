//! Tauri backend: thin command layer over `cleanprojects-core`.
//! Long-running work (scan, clean, directory sizing) runs on threads and
//! streams events to the webview. A system tray keeps the app resident for
//! scheduled background scans.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use cleanprojects_core::cleaner::{self, CleanEvent, CleanMode, CleanOptions};
use cleanprojects_core::fsutil::{compact_path, human_size};
use cleanprojects_core::recovery;
use cleanprojects_core::routine::{self, Routine, RoutineRun};
use cleanprojects_core::scan::{self, ScanContext, ScanEvent, ScanResult};
use cleanprojects_core::shell_integration::{self, IntegrationStatus, LaunchAction};
use cleanprojects_core::ScanSnapshot;
use cleanprojects_core::{
    disk_for, volumes, Candidate, Config, DiskInfo, HistoryEntry, Journal, Node, Paths, State,
    TreeNode, Volume,
};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, State as TauriState, WindowEvent};
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    paths: Paths,
    config: Mutex<Config>,
    state: Mutex<State>,
    last_scan: Mutex<Option<ScanResult>>,
    scanning: AtomicBool,
    cleaning: AtomicBool,
    tray: Mutex<Option<TrayIcon>>,
    /// `--scan DIR` / `--clean DIR` received on launch, consumed by the first bootstrap.
    launch: Mutex<Option<LaunchAction>>,
    routine_running: AtomicBool,
}

#[derive(Serialize)]
struct Bootstrap {
    version: String,
    os: String,
    home: String,
    config_path: String,
    data_dir: String,
    config: Config,
    excluded: Vec<String>,
    history: Vec<HistoryEntry>,
    disk: DiskInfo,
    volumes: Vec<Volume>,
    roots: Vec<String>,
    protected: Vec<String>,
    tools: Vec<(String, bool)>,
    candidates: Vec<Candidate>,
    cached: bool,
    guarded: usize,
    scanning: bool,
    recovery: RecoverySummary,
    scans: Vec<ScanSnapshot>,
    launch: Option<LaunchAction>,
    routines: Vec<Routine>,
    routine_runs: Vec<RoutineRun>,
    integration: IntegrationStatus,
    exe: String,
}

#[derive(Serialize, Clone)]
struct RecoverySummary {
    items: usize,
    bytes: u64,
    entries: Vec<recovery::RecoveryEntry>,
}

#[derive(Serialize, Clone)]
struct ScanSummary {
    candidates: usize,
    total: u64,
    projects: usize,
    repos: usize,
    guarded: usize,
    duration_ms: u128,
    disk: DiskInfo,
    volumes: Vec<Volume>,
    background: bool,
}

#[derive(Serialize, Clone)]
struct CleanDone {
    entry: HistoryEntry,
    disk: DiskInfo,
    removed_ids: Vec<String>,
    recovery: RecoverySummary,
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
    #[serde(default)]
    pub no_extras: bool,
    #[serde(default)]
    pub background: bool,
    /// Scan only these directories (context-menu launch).
    #[serde(default)]
    pub roots: Vec<String>,
}

#[derive(Deserialize)]
pub struct CleanRequest {
    pub ids: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
    pub mode: Option<String>,
    #[serde(default)]
    pub force: bool,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e:#}")
}

fn recovery_summary(paths: &Paths) -> RecoverySummary {
    let mut j = paths.load_journal();
    j.prune_missing();
    RecoverySummary {
        items: j.entries.len(),
        bytes: j.total_size(),
        entries: j.entries,
    }
}

fn set_tray_title(app: &AppHandle, total: u64) {
    let st = app.state::<AppState>();
    let guard = st.tray.lock().unwrap();
    if let Some(tray) = guard.as_ref() {
        let tooltip = if total > 0 {
            format!("CleanProjects · {} recoverable", human_size(total))
        } else {
            "CleanProjects".to_string()
        };
        #[cfg(target_os = "macos")]
        {
            let text = if total > 0 {
                human_size(total)
            } else {
                String::new()
            };
            let _ = tray.set_title(Some(text));
        }
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn bootstrap(st: TauriState<AppState>) -> CmdResult<Bootstrap> {
    let config = st.config.lock().unwrap().clone();
    let state = st.state.lock().unwrap().clone();
    let ctx = ScanContext::new(st.paths.home.clone(), config.clone());
    let mut cached = false;
    let (candidates, guarded) = {
        let mut last = st.last_scan.lock().unwrap();
        if last.is_none() {
            if let Some(c) = cleanprojects_core::load_cache(&st.paths.scan_cache_file) {
                cached = true;
                *last = Some(c);
            }
        }
        last.as_ref()
            .map(|r| (r.candidates.clone(), r.guarded))
            .unwrap_or_default()
    };
    Ok(Bootstrap {
        version: cleanprojects_core::VERSION.to_string(),
        os: std::env::consts::OS.to_string(),
        home: st.paths.home.display().to_string(),
        config_path: st.paths.config_file.display().to_string(),
        data_dir: st.paths.data_dir.display().to_string(),
        config,
        excluded: state.excluded.iter().cloned().collect(),
        history: state.history,
        disk: disk_for(&st.paths.home),
        volumes: ctx.volumes.clone(),
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
        cached,
        guarded,
        scanning: st.scanning.load(Ordering::SeqCst),
        recovery: recovery_summary(&st.paths),
        scans: state.scans,
        launch: st.launch.lock().unwrap().take(),
        routines: config_routines(&st),
        routine_runs: state.routine_runs,
        integration: shell_integration::status(&st.paths.home),
        exe: std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    })
}

fn config_routines(st: &AppState) -> Vec<Routine> {
    st.config.lock().unwrap().routines.clone()
}

fn run_scan_thread(app: AppHandle, ctx: ScanContext, background: bool) {
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx2 = ctx.clone();
        let worker = std::thread::spawn(move || scan::scan(&ctx2, tx));
        for ev in rx {
            match ev {
                ScanEvent::Progress { phase, message } => {
                    let _ = app.emit(
                        "scan://progress",
                        serde_json::json!({ "phase": phase, "message": message }),
                    );
                }
                ScanEvent::Found(c) => {
                    let _ = app.emit("scan://found", *c);
                }
                ScanEvent::Done(_) => {}
            }
        }
        let Ok(result) = worker.join() else {
            app.state::<AppState>()
                .scanning
                .store(false, Ordering::SeqCst);
            return;
        };
        let st = app.state::<AppState>();
        let _ = cleanprojects_core::save_cache(&st.paths.scan_cache_file, &result);
        let total = result.total();
        let summary = ScanSummary {
            candidates: result.candidates.len(),
            total,
            projects: result.projects,
            repos: result.repos,
            guarded: result.guarded,
            duration_ms: result.duration_ms,
            disk: result.disk.clone(),
            volumes: result.volumes.clone(),
            background,
        };
        {
            let mut s = st.state.lock().unwrap();
            s.record_scan(
                total,
                result.disk.free,
                result.disk.total,
                result.candidates.len(),
            );
            let _ = st.paths.save_state(&s);
        }
        *st.last_scan.lock().unwrap() = Some(result);
        st.scanning.store(false, Ordering::SeqCst);
        set_tray_title(&app, total);
        let _ = app.emit("scan://done", summary);
        if background {
            let threshold = st.config.lock().unwrap().ui.notify_threshold_gb * 1024 * 1024 * 1024;
            if total >= threshold {
                let _ = app
                    .notification()
                    .builder()
                    .title("CleanProjects")
                    .body(format!(
                        "{} can be reclaimed. Open the app to review.",
                        human_size(total)
                    ))
                    .show();
            }
        }
    });
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
    let mut config = st.config.lock().unwrap().clone();
    if !options.roots.is_empty() {
        config.scan.default_roots = false;
        config.scan.roots = options.roots.clone();
        config.scan.volumes = false;
    }
    let mut ctx = ScanContext::new(st.paths.home.clone(), config);
    if !options.roots.is_empty() {
        ctx.include_global = false;
        ctx.include_tools = false;
        ctx.include_extras = false;
    }
    if options.no_git {
        ctx.include_git = false;
    }
    ctx.include_tools = !options.no_tools;
    ctx.include_projects = !options.no_projects;
    ctx.include_global = !options.no_global;
    ctx.include_extras = !options.no_extras;
    let previous = st.last_scan.lock().unwrap().clone();
    let ctx = ctx.with_previous(previous.as_ref());
    run_scan_thread(app, ctx, options.background);
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
    let mode = request
        .mode
        .as_deref()
        .map(CleanMode::parse)
        .unwrap_or_else(|| CleanMode::parse(&config.clean.mode));
    let opts = CleanOptions {
        dry_run: request.dry_run,
        mode,
        home: st.paths.home.clone(),
        protected: ctx.protected,
        data_dir: st.paths.data_dir.clone(),
        volumes: ctx.volumes,
        block_running_apps: config.clean.block_running_apps && !request.force,
    };
    let app2 = app.clone();
    std::thread::spawn(move || {
        let app3 = app2.clone();
        let outcome = cleaner::run_with_journal(&items, &opts, &mut |ev| match ev {
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
        if !outcome.recovered.is_empty() {
            let mut journal = st.paths.load_journal();
            cleaner::record_recovered(&mut journal, outcome.recovered);
            let _ = st.paths.save_journal(&journal);
        }
        let entry = outcome.entry;
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
        let mut total = 0;
        if let Some(scan) = st.last_scan.lock().unwrap().as_mut() {
            scan.candidates.retain(|c| !removed_ids.contains(&c.id));
            total = scan.total();
            let _ = cleanprojects_core::save_cache(&st.paths.scan_cache_file, scan);
        }
        {
            let mut s = st.state.lock().unwrap();
            s.history.push(entry.clone());
            let _ = st.paths.save_state(&s);
        }
        st.paths.append_log(&format!(
            "clean dry_run={} mode={} freed={} items={}",
            entry.dry_run,
            entry.mode,
            entry.freed,
            entry.items.len()
        ));
        st.cleaning.store(false, Ordering::SeqCst);
        set_tray_title(&app2, total);
        let _ = app2.emit(
            "clean://done",
            CleanDone {
                entry,
                disk: disk_for(&st.paths.home),
                removed_ids,
                recovery: recovery_summary(&st.paths),
            },
        );
    });
    Ok(())
}

#[tauri::command]
fn undo_run(st: TauriState<AppState>, index: usize) -> CmdResult<(usize, Vec<String>)> {
    let mut state = st.state.lock().unwrap();
    let Some(entry) = state.history.get(index).cloned() else {
        return Err("no such run".into());
    };
    let mut journal = st.paths.load_journal();
    let (restored, errors) = cleaner::undo(&entry, &mut journal);
    st.paths.save_journal(&journal).map_err(err)?;
    if let Some(e) = state.history.get_mut(index) {
        for item in e.items.iter_mut() {
            if item.recovery_id.is_some() && !errors.iter().any(|m| m.contains(&item.label)) {
                item.recovery_id = None;
            }
        }
    }
    st.paths.save_state(&state).map_err(err)?;
    Ok((restored, errors))
}

#[tauri::command]
fn restore_entry(st: TauriState<AppState>, id: String) -> CmdResult<RecoverySummary> {
    let mut journal = st.paths.load_journal();
    let Some(pos) = journal.entries.iter().position(|e| e.id == id) else {
        return Err("not in recovery bin".into());
    };
    recovery::restore(&journal.entries[pos]).map_err(err)?;
    journal.entries.remove(pos);
    st.paths.save_journal(&journal).map_err(err)?;
    let mut state = st.state.lock().unwrap();
    for h in state.history.iter_mut() {
        for item in h.items.iter_mut() {
            if item.recovery_id.as_deref() == Some(&id) {
                item.recovery_id = None;
            }
        }
    }
    let _ = st.paths.save_state(&state);
    Ok(recovery_summary(&st.paths))
}

#[tauri::command]
fn recovery_purge(st: TauriState<AppState>, all: bool) -> CmdResult<RecoverySummary> {
    let mut journal: Journal = st.paths.load_journal();
    if all {
        for e in &journal.entries {
            let _ = recovery::purge_entry(e);
        }
        journal.entries.clear();
    } else {
        let days = st.config.lock().unwrap().clean.recovery_days;
        recovery::purge_older_than(&mut journal, days);
    }
    st.paths.save_journal(&journal).map_err(err)?;
    let ids: HashSet<String> = journal.entries.iter().map(|e| e.id.clone()).collect();
    let mut state = st.state.lock().unwrap();
    for h in state.history.iter_mut() {
        for item in h.items.iter_mut() {
            if item
                .recovery_id
                .as_ref()
                .map(|i| !ids.contains(i))
                .unwrap_or(false)
            {
                item.recovery_id = None;
            }
        }
    }
    let _ = st.paths.save_state(&state);
    Ok(recovery_summary(&st.paths))
}

#[tauri::command]
fn toggle_exclude(st: TauriState<AppState>, id: String) -> CmdResult<bool> {
    let mut s = st.state.lock().unwrap();
    let now_excluded = s.toggle_excluded(&id);
    st.paths.save_state(&s).map_err(err)?;
    Ok(now_excluded)
}

#[tauri::command]
fn save_config(app: AppHandle, st: TauriState<AppState>, config: Config) -> CmdResult<Config> {
    st.paths.save_config(&config).map_err(err)?;
    *st.config.lock().unwrap() = config.clone();
    apply_autostart(
        &app,
        config.ui.scan_interval_hours > 0 && config.ui.close_to_tray,
    );
    Ok(config)
}

fn apply_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let m = app.autolaunch();
    let _ = if enabled { m.enable() } else { m.disable() };
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
fn list_volumes(st: TauriState<AppState>) -> Vec<Volume> {
    volumes(&st.paths.home)
}

#[tauri::command]
async fn explore_dir(path: String) -> Vec<Node> {
    let p = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || cleanprojects_core::list_dir(&p))
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn explore_tree(path: String, depth: Option<usize>, top: Option<usize>) -> Vec<TreeNode> {
    let p = PathBuf::from(path);
    let depth = depth.unwrap_or(2).clamp(1, 4);
    let top = top.unwrap_or(24).clamp(4, 60);
    tauri::async_runtime::spawn_blocking(move || cleanprojects_core::tree(&p, depth, top))
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn add_manual_candidate(st: TauriState<AppState>, path: String) -> CmdResult<Candidate> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("path does not exist".into());
    }
    let m = cleanprojects_core::fsutil::measure(&p);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or(path.clone());
    let c = Candidate::new(
        name,
        "Picked in the explorer. Review carefully: this was not matched by a cleanup rule.",
        cleanprojects_core::Category::Temp,
        cleanprojects_core::Risk::High,
        cleanprojects_core::Action::RemovePath { path: p.clone() },
    )
    .with_measure(m)
    .with_modified(cleanprojects_core::fsutil::modified_at(&p))
    .with_owner("Explorer")
    .with_tag("manual");
    let mut last = st.last_scan.lock().unwrap();
    let scan = last.get_or_insert_with(|| ScanResult {
        candidates: vec![],
        disk: disk_for(&st.paths.home),
        volumes: vec![],
        roots: vec![],
        protected: vec![],
        projects: 0,
        repos: 0,
        guarded: 0,
        started_at: chrono::Local::now(),
        duration_ms: 0,
    });
    if !scan.candidates.iter().any(|x| x.id == c.id) {
        scan.candidates.push(c.clone());
    }
    Ok(c)
}

#[tauri::command]
fn write_report(
    st: TauriState<AppState>,
    path: String,
    ids: Vec<String>,
    format: Option<String>,
) -> CmdResult<String> {
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
    let fmt = format
        .map(|f| cleanprojects_core::ReportFormat::parse(&f))
        .unwrap_or_else(|| {
            cleanprojects_core::ReportFormat::from_path(std::path::Path::new(&path))
        });
    std::fs::write(
        &path,
        cleanprojects_core::report::render(result, &items, &st.paths.home, fmt),
    )
    .map_err(err)?;
    Ok(path)
}

#[tauri::command]
async fn large_files(
    st: TauriState<'_, AppState>,
    path: Option<String>,
    min_mb: Option<u64>,
    top: Option<usize>,
) -> CmdResult<Vec<Node>> {
    let config = st.config.lock().unwrap().clone();
    let ctx = ScanContext::new(st.paths.home.clone(), config);
    let roots = match path {
        Some(p) => vec![PathBuf::from(p)],
        None => {
            let mut r = vec![st.paths.home.clone()];
            r.extend(
                ctx.roots
                    .iter()
                    .filter(|p| !p.starts_with(&st.paths.home))
                    .cloned(),
            );
            r
        }
    };
    let protected = ctx.protected.clone();
    let min = min_mb.unwrap_or(256) * 1024 * 1024;
    let top = top.unwrap_or(60).clamp(5, 500);
    Ok(tauri::async_runtime::spawn_blocking(move || {
        cleanprojects_core::large_files(&roots, min, top, &protected)
    })
    .await
    .unwrap_or_default())
}

/// Add a path to the protected list and drop any candidates inside it.
#[tauri::command]
fn protect_path(app: AppHandle, st: TauriState<AppState>, path: String) -> CmdResult<Config> {
    let p = PathBuf::from(&path);
    let shown = compact_path(&p, &st.paths.home);
    let mut config = st.config.lock().unwrap().clone();
    if !config
        .scan
        .protected
        .iter()
        .any(|x| x == &shown || x == &path)
    {
        config.scan.protected.push(shown);
    }
    st.paths.save_config(&config).map_err(err)?;
    *st.config.lock().unwrap() = config.clone();
    if let Some(scan) = st.last_scan.lock().unwrap().as_mut() {
        scan.candidates.retain(|c| {
            c.action
                .path()
                .map(|cp| !cp.starts_with(&p))
                .unwrap_or(true)
        });
        let _ = cleanprojects_core::save_cache(&st.paths.scan_cache_file, scan);
    }
    let _ = app.emit("candidates://changed", ());
    Ok(config)
}

#[tauri::command]
fn get_routines(st: TauriState<AppState>) -> (Vec<Routine>, Vec<RoutineRun>) {
    (
        config_routines(&st),
        st.state.lock().unwrap().routine_runs.clone(),
    )
}

#[tauri::command]
fn routine_templates() -> Vec<Routine> {
    routine::templates()
}

#[tauri::command]
fn save_routine(st: TauriState<AppState>, mut routine: Routine) -> CmdResult<Vec<Routine>> {
    if routine.id.is_empty() {
        routine.id = routine::new_id();
    }
    if routine.mode != "trash" {
        routine.mode = "recovery".into();
    }
    if routine.max_risk > cleanprojects_core::Risk::Medium {
        routine.max_risk = cleanprojects_core::Risk::Medium;
    }
    let mut config = st.config.lock().unwrap().clone();
    match config.routines.iter_mut().find(|r| r.id == routine.id) {
        Some(r) => *r = routine,
        None => config.routines.push(routine),
    }
    st.paths.save_config(&config).map_err(err)?;
    *st.config.lock().unwrap() = config.clone();
    Ok(config.routines)
}

#[tauri::command]
fn delete_routine(st: TauriState<AppState>, id: String) -> CmdResult<Vec<Routine>> {
    let mut config = st.config.lock().unwrap().clone();
    config.routines.retain(|r| r.id != id);
    st.paths.save_config(&config).map_err(err)?;
    *st.config.lock().unwrap() = config.clone();
    Ok(config.routines)
}

fn run_routines_thread(app: AppHandle, routines: Vec<Routine>, dry_run: bool) {
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        for r in &routines {
            let config = st.config.lock().unwrap().clone();
            let mut state = st.state.lock().unwrap().clone();
            let run = routine::run(&st.paths, &config, &mut state, r, dry_run);
            *st.state.lock().unwrap() = state;
            if r.notify && !dry_run {
                let _ = app
                    .notification()
                    .builder()
                    .title(format!("CleanProjects · {}", r.name))
                    .body(if run.items == 0 {
                        "Nothing to clean this time.".to_string()
                    } else {
                        format!(
                            "{} freed ({} items). Undo from History.",
                            human_size(run.freed),
                            run.items
                        )
                    })
                    .show();
            }
            let _ = app.emit("routine://done", run);
        }
        // Reload the cached scan the routine just saved so the window reflects it.
        if let Some(c) = cleanprojects_core::load_cache(&st.paths.scan_cache_file) {
            set_tray_title(&app, c.total());
            *st.last_scan.lock().unwrap() = Some(c);
        }
        st.routine_running.store(false, Ordering::SeqCst);
        let _ = app.emit("candidates://changed", ());
    });
}

#[tauri::command]
fn run_routine(
    app: AppHandle,
    st: TauriState<AppState>,
    id: String,
    dry_run: Option<bool>,
) -> CmdResult<()> {
    let Some(r) = config_routines(&st).into_iter().find(|r| r.id == id) else {
        return Err("no such routine".into());
    };
    if st.routine_running.swap(true, Ordering::SeqCst) {
        return Err("a routine is already running".into());
    }
    run_routines_thread(app, vec![r], dry_run.unwrap_or(false));
    Ok(())
}

#[tauri::command]
fn integration_status(st: TauriState<AppState>) -> IntegrationStatus {
    shell_integration::status(&st.paths.home)
}

#[tauri::command]
fn integration_install(st: TauriState<AppState>) -> CmdResult<IntegrationStatus> {
    let exe = std::env::current_exe().map_err(err)?;
    shell_integration::install(&st.paths.home, &exe).map_err(err)
}

#[tauri::command]
fn integration_uninstall(st: TauriState<AppState>) -> CmdResult<IntegrationStatus> {
    shell_integration::uninstall(&st.paths.home).map_err(err)
}

#[tauri::command]
fn path_exists(path: String) -> bool {
    PathBuf::from(path).exists()
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let open = MenuItem::with_id(app, "open", "Open CleanProjects", true, None::<&str>)?;
    let rescan = MenuItem::with_id(app, "rescan", "Rescan now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &rescan, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("CleanProjects")
        .show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(false);
    }
    builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "rescan" => {
                show_main(app);
                let st = app.state::<AppState>();
                if !st.scanning.swap(true, Ordering::SeqCst) {
                    let cfg = st.config.lock().unwrap().clone();
                    run_scan_thread(
                        app.clone(),
                        ScanContext::new(st.paths.home.clone(), cfg),
                        false,
                    );
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)
}

fn start_routine_scheduler(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(300));
        let st = app.state::<AppState>();
        if st.scanning.load(Ordering::SeqCst)
            || st.cleaning.load(Ordering::SeqCst)
            || st.routine_running.load(Ordering::SeqCst)
        {
            continue;
        }
        let due: Vec<Routine> = {
            let config = st.config.lock().unwrap();
            let state = st.state.lock().unwrap();
            routine::due(&config, &state, chrono::Local::now())
                .into_iter()
                .cloned()
                .collect()
        };
        if due.is_empty() {
            continue;
        }
        if st.routine_running.swap(true, Ordering::SeqCst) {
            continue;
        }
        run_routines_thread(app.clone(), due, false);
    });
}

fn start_scheduler(app: AppHandle) {
    std::thread::spawn(move || loop {
        let hours = app
            .state::<AppState>()
            .config
            .lock()
            .unwrap()
            .ui
            .scan_interval_hours;
        if hours == 0 {
            std::thread::sleep(Duration::from_secs(600));
            continue;
        }
        std::thread::sleep(Duration::from_secs(hours * 3600));
        let st = app.state::<AppState>();
        let hours_now = st.config.lock().unwrap().ui.scan_interval_hours;
        if hours_now == 0 || st.scanning.swap(true, Ordering::SeqCst) {
            continue;
        }
        let cfg = st.config.lock().unwrap().clone();
        run_scan_thread(
            app.clone(),
            ScanContext::new(st.paths.home.clone(), cfg),
            true,
        );
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = Paths::discover().expect("cannot resolve home directory");
    let config = paths.load_config().unwrap_or_default();
    let state = paths.load_state().unwrap_or_default();
    // Purge recovery bin entries past retention on every launch.
    {
        let mut journal = paths.load_journal();
        journal.prune_missing();
        recovery::purge_older_than(&mut journal, config.clean.recovery_days.max(1));
        let _ = paths.save_journal(&journal);
    }
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(action) = shell_integration::parse_launch_args(args) {
                let _ = app.emit("launch://action", action);
            }
            show_main(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ));
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder
        .manage(AppState {
            paths,
            config: Mutex::new(config),
            state: Mutex::new(state),
            last_scan: Mutex::new(None),
            scanning: AtomicBool::new(false),
            cleaning: AtomicBool::new(false),
            tray: Mutex::new(None),
            launch: Mutex::new(shell_integration::parse_launch_args(std::env::args())),
            routine_running: AtomicBool::new(false),
        })
        .setup(|app| {
            let handle = app.handle().clone();
            match build_tray(&handle) {
                Ok(tray) => *app.state::<AppState>().tray.lock().unwrap() = Some(tray),
                Err(e) => eprintln!("tray unavailable: {e}"),
            }
            let st = app.state::<AppState>();
            let cached_total = st.state.lock().unwrap().last_recoverable;
            set_tray_title(&handle, cached_total);
            start_scheduler(handle.clone());
            start_routine_scheduler(handle.clone());
            if std::env::args().any(|a| a == "--background") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let st = window.app_handle().state::<AppState>();
                let to_tray = st.config.lock().unwrap().ui.close_to_tray;
                if to_tray && st.tray.lock().unwrap().is_some() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            start_scan,
            get_candidates,
            start_clean,
            undo_run,
            restore_entry,
            recovery_purge,
            toggle_exclude,
            save_config,
            get_history,
            clear_history,
            list_volumes,
            explore_dir,
            explore_tree,
            add_manual_candidate,
            write_report,
            large_files,
            protect_path,
            get_routines,
            routine_templates,
            save_routine,
            delete_routine,
            run_routine,
            integration_status,
            integration_install,
            integration_uninstall,
            path_exists,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running CleanProjects");
}
