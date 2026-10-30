//! Interactive terminal UI built on ratatui.

mod theme;
mod ui;

use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use cleanprojects_core::cleaner::{self, CleanEvent, CleanMode};
use cleanprojects_core::scan::{self, ScanContext, ScanEvent};
use cleanprojects_core::{
    Candidate, Category, DiskInfo, HistoryEntry, Lang, Node, Risk, Strings, Volume,
};

use crate::cli::commands::{build_context, clean_options, Env};
use crate::cli::ScanArgs;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    Candidates,
    Git,
    Explore,
    History,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Overview,
        Tab::Candidates,
        Tab::Git,
        Tab::Explore,
        Tab::History,
        Tab::Settings,
    ];
    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
    fn next(self) -> Tab {
        Tab::ALL[(self.index() + 1) % Tab::ALL.len()]
    }
    fn prev(self) -> Tab {
        Tab::ALL[(self.index() + Tab::ALL.len() - 1) % Tab::ALL.len()]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    Size,
    Name,
    Age,
    Risk,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewFilter {
    All,
    Stale,
    Excluded,
}

pub enum Modal {
    Confirm,
    Cleaning,
    Done(HistoryEntry),
    Help,
}

pub struct App {
    pub env: Env,
    pub args: ScanArgs,
    pub lang: Lang,
    pub tab: Tab,
    pub candidates: Vec<Candidate>,
    pub disk: DiskInfo,
    pub scanning: bool,
    pub scan_phase: String,
    pub scan_msg: String,
    pub scan_rx: Option<Receiver<ScanEvent>>,
    pub scan_stats: Option<(usize, usize, u128)>,
    pub selected: HashSet<String>,
    pub cursor: usize,
    pub git_cursor: usize,
    pub history_cursor: usize,
    pub settings_cursor: usize,
    pub search: String,
    pub searching: bool,
    pub risk_filter: Option<Risk>,
    pub category_filter: Option<Category>,
    pub view_filter: ViewFilter,
    pub sort: Sort,
    pub show_detail: bool,
    pub modal: Option<Modal>,
    pub mode: CleanMode,
    pub dry_run: bool,
    pub volumes: Vec<Volume>,
    pub guarded: usize,
    pub cached: bool,
    pub explore_dir: std::path::PathBuf,
    pub explore_nodes: Vec<Node>,
    pub explore_cursor: usize,
    pub explore_loading: bool,
    pub explore_rx: Option<Receiver<(std::path::PathBuf, Vec<Node>)>>,
    pub clean_rx: Option<Receiver<CleanEvent>>,
    pub clean_log: Vec<String>,
    pub clean_progress: (usize, usize, String),
    pub toast: Option<(String, Instant)>,
    pub tick: usize,
    pub should_quit: bool,
    pub pending_state: Option<std::sync::Arc<std::sync::Mutex<cleanprojects_core::State>>>,
}

impl App {
    fn new(env: Env, args: ScanArgs) -> App {
        let lang = env.config.ui.language;
        let mode = CleanMode::parse(&env.config.clean.mode);
        let disk = cleanprojects_core::disk_for(&env.paths.home);
        let volumes = cleanprojects_core::volumes(&env.paths.home);
        let home = env.paths.home.clone();
        let cached = cleanprojects_core::load_cache(&env.paths.scan_cache_file);
        let mut app = App {
            env,
            args,
            lang,
            tab: Tab::Overview,
            candidates: Vec::new(),
            disk,
            scanning: false,
            scan_phase: String::new(),
            scan_msg: String::new(),
            scan_rx: None,
            scan_stats: None,
            selected: HashSet::new(),
            cursor: 0,
            git_cursor: 0,
            history_cursor: 0,
            settings_cursor: 0,
            search: String::new(),
            searching: false,
            risk_filter: None,
            category_filter: None,
            view_filter: ViewFilter::All,
            sort: Sort::Size,
            show_detail: false,
            modal: None,
            mode,
            dry_run: false,
            volumes,
            guarded: 0,
            cached: false,
            explore_dir: home,
            explore_nodes: Vec::new(),
            explore_cursor: 0,
            explore_loading: false,
            explore_rx: None,
            clean_rx: None,
            clean_log: Vec::new(),
            clean_progress: (0, 0, String::new()),
            toast: None,
            tick: 0,
            should_quit: false,
            pending_state: None,
        };
        if let Some(c) = cached {
            app.candidates = c.candidates;
            app.guarded = c.guarded;
            app.scan_stats = Some((c.projects, c.repos, c.duration_ms));
            app.cached = true;
        }
        app
    }

    pub fn s(&self) -> &'static Strings {
        self.lang.strings()
    }

    pub fn ctx(&self) -> ScanContext {
        build_context(&self.env, &self.args)
    }

    fn start_scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        if !self.cached {
            self.candidates.clear();
        }
        self.selected.clear();
        self.cursor = 0;
        self.scan_stats = None;
        self.scan_phase = "scan".into();
        self.scan_msg.clear();
        let ctx = self.ctx();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            scan::scan(&ctx, tx);
        });
        self.scan_rx = Some(rx);
    }

    fn pump(&mut self) {
        if let Some(rx) = &self.scan_rx {
            let mut done = None;
            loop {
                match rx.try_recv() {
                    Ok(ScanEvent::Progress { phase, message }) => {
                        self.scan_phase = phase.to_string();
                        self.scan_msg = message;
                    }
                    Ok(ScanEvent::Found(c)) => {
                        if self.cached {
                            self.cached = false;
                            self.candidates.clear();
                        }
                        if !self.candidates.iter().any(|x| x.id == c.id) {
                            self.candidates.push(*c);
                        }
                    }
                    Ok(ScanEvent::Done(r)) => {
                        done = Some(r);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.scanning = false;
                        self.scan_rx = None;
                        break;
                    }
                }
            }
            if let Some(r) = done {
                let _ = cleanprojects_core::save_cache(&self.env.paths.scan_cache_file, &r);
                self.env.state.record_scan(
                    r.total(),
                    r.disk.free,
                    r.disk.total,
                    r.candidates.len(),
                );
                self.cached = false;
                self.guarded = r.guarded;
                self.volumes = r.volumes.clone();
                self.candidates = r.candidates;
                self.disk = r.disk;
                self.scan_stats = Some((r.projects, r.repos, r.duration_ms));
                self.scanning = false;
                self.scan_rx = None;
                self.env.state.last_scan_at = Some(r.started_at);
                self.env.state.last_recoverable = self.candidates.iter().map(|c| c.size).sum();
                let _ = self.env.paths.save_state(&self.env.state);
            }
        }
        if let Some(rx) = &self.explore_rx {
            if let Ok((dir, nodes)) = rx.try_recv() {
                self.explore_dir = dir;
                self.explore_nodes = nodes;
                self.explore_cursor = 0;
                self.explore_loading = false;
                self.explore_rx = None;
            }
        }
        if let Some(rx) = &self.clean_rx {
            let mut finished = None;
            loop {
                match rx.try_recv() {
                    Ok(CleanEvent::Start {
                        index,
                        total,
                        label,
                        ..
                    }) => {
                        self.clean_progress = (index, total, label.clone());
                        self.clean_log
                            .push(format!("[{}/{}] {}", index + 1, total, label));
                    }
                    Ok(CleanEvent::Log(l)) => self.clean_log.push(l),
                    Ok(CleanEvent::Item {
                        ok, freed, error, ..
                    }) => {
                        if ok {
                            self.clean_log.push(format!(
                                "  ✓ {}",
                                cleanprojects_core::fsutil::human_size(freed)
                            ));
                        } else {
                            self.clean_log
                                .push(format!("  ✗ {}", error.unwrap_or_default()));
                        }
                    }
                    Ok(CleanEvent::Done { .. }) => {}
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        finished = Some(());
                        break;
                    }
                }
            }
            if finished.is_some() {
                self.clean_rx = None;
                if let Some(p) = self.pending_state.take() {
                    self.env.state = p.lock().unwrap().clone();
                }
                if let Some(entry) = self.env.state.history.last().cloned() {
                    if !entry.dry_run {
                        let gone: HashSet<&str> = entry
                            .items
                            .iter()
                            .filter(|i| i.ok)
                            .map(|i| i.id.as_str())
                            .collect();
                        self.candidates.retain(|c| !gone.contains(c.id.as_str()));
                        self.selected.retain(|id| !gone.contains(id.as_str()));
                        self.disk = cleanprojects_core::disk_for(&self.env.paths.home);
                    }
                    self.modal = Some(Modal::Done(entry));
                } else {
                    self.modal = None;
                }
            }
        }
    }

    fn start_clean(&mut self) {
        let items: Vec<Candidate> = self
            .visible_all()
            .into_iter()
            .filter(|c| self.selected.contains(&c.id))
            .cloned()
            .collect();
        if items.is_empty() {
            self.modal = None;
            return;
        }
        let ctx = self.ctx();
        let opts = clean_options(&self.env, &ctx, self.dry_run, self.mode, false);
        let (tx, rx) = std::sync::mpsc::channel();
        let paths = self.env.paths.clone();
        let state_file_state = std::sync::Arc::new(std::sync::Mutex::new(self.env.state.clone()));
        let shared = state_file_state.clone();
        std::thread::spawn(move || {
            let outcome = cleaner::run_with_journal(&items, &opts, &mut |ev| {
                let _ = tx.send(ev);
            });
            if !outcome.recovered.is_empty() {
                let mut journal = paths.load_journal();
                cleaner::record_recovered(&mut journal, outcome.recovered);
                let _ = paths.save_journal(&journal);
            }
            let mut st = shared.lock().unwrap();
            st.history.push(outcome.entry);
            let _ = paths.save_state(&st);
            paths.append_log("clean finished (tui)");
        });
        self.clean_rx = Some(rx);
        self.clean_log.clear();
        self.clean_progress = (0, 0, String::new());
        self.modal = Some(Modal::Cleaning);
        // The thread owns a copy of state; reload after it finishes.
        self.pending_state = Some(state_file_state);
    }

    /// Candidates of the current tab after filters, in display order.
    pub fn visible_all(&self) -> Vec<&Candidate> {
        let excluded = &self.env.state.excluded;
        let q = self.search.to_ascii_lowercase();
        let mut rows: Vec<&Candidate> = self
            .candidates
            .iter()
            .filter(|c| match self.tab {
                Tab::Git => c.category == Category::Git,
                _ => true,
            })
            .filter(|c| match self.view_filter {
                ViewFilter::All => !excluded.contains(&c.id),
                ViewFilter::Stale => c.is_stale() && !excluded.contains(&c.id),
                ViewFilter::Excluded => excluded.contains(&c.id),
            })
            .filter(|c| self.risk_filter.map(|r| c.risk == r).unwrap_or(true))
            .filter(|c| {
                self.category_filter
                    .map(|k| c.category == k)
                    .unwrap_or(true)
            })
            .filter(|c| {
                q.is_empty() || {
                    let hay = format!(
                        "{} {} {} {}",
                        c.label,
                        c.detail(),
                        c.category.as_str(),
                        c.project.as_ref().map(|p| p.name.as_str()).unwrap_or("")
                    )
                    .to_ascii_lowercase();
                    hay.contains(&q)
                }
            })
            .collect();
        match self.sort {
            Sort::Size => rows.sort_by_key(|c| std::cmp::Reverse(c.size)),
            Sort::Name => rows.sort_by(|a, b| a.label.cmp(&b.label)),
            Sort::Age => rows.sort_by_key(|c| std::cmp::Reverse(c.age_days())),
            Sort::Risk => rows.sort_by(|a, b| b.risk.cmp(&a.risk).then(b.size.cmp(&a.size))),
        }
        rows
    }

    pub fn selected_total(&self) -> (usize, u64) {
        let n = self.selected.len();
        let total = self
            .candidates
            .iter()
            .filter(|c| self.selected.contains(&c.id))
            .map(|c| c.size)
            .sum();
        (n, total)
    }

    fn explore(&mut self, dir: std::path::PathBuf) {
        if self.explore_loading {
            return;
        }
        self.explore_loading = true;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let nodes = cleanprojects_core::list_dir(&dir);
            let _ = tx.send((dir, nodes));
        });
        self.explore_rx = Some(rx);
    }

    fn undo_selected_history(&mut self) {
        let n = self.env.state.history.len();
        if n == 0 {
            return;
        }
        let idx = n - 1 - self.history_cursor.min(n - 1);
        let entry = self.env.state.history[idx].clone();
        if entry.restorable() == 0 {
            self.toast("nothing to restore in this run");
            return;
        }
        let mut journal = self.env.paths.load_journal();
        let (restored, errors) = cleaner::undo(&entry, &mut journal);
        let _ = self.env.paths.save_journal(&journal);
        for item in self.env.state.history[idx].items.iter_mut() {
            if item.recovery_id.is_some() && !errors.iter().any(|e| e.contains(&item.label)) {
                item.recovery_id = None;
            }
        }
        let _ = self.env.paths.save_state(&self.env.state);
        self.toast(format!("↺ {restored} restored, {} failed", errors.len()));
    }

    fn cursor_mut(&mut self) -> &mut usize {
        match self.tab {
            Tab::Explore => &mut self.explore_cursor,
            Tab::Git => &mut self.git_cursor,
            Tab::History => &mut self.history_cursor,
            Tab::Settings => &mut self.settings_cursor,
            _ => &mut self.cursor,
        }
    }

    fn list_len(&self) -> usize {
        match self.tab {
            Tab::Candidates | Tab::Git => self.visible_all().len(),
            Tab::History => self.env.state.history.len(),
            Tab::Settings => ui::SETTINGS_ROWS,
            Tab::Explore => self.explore_nodes.len(),
            Tab::Overview => 0,
        }
    }

    fn move_cursor(&mut self, delta: isize) {
        let len = self.list_len();
        if len == 0 {
            *self.cursor_mut() = 0;
            return;
        }
        let cur = *self.cursor_mut() as isize;
        let next = (cur + delta).clamp(0, len as isize - 1) as usize;
        *self.cursor_mut() = next;
    }

    pub fn current(&self) -> Option<&Candidate> {
        let idx = match self.tab {
            Tab::Git => self.git_cursor,
            _ => self.cursor,
        };
        self.visible_all().get(idx).copied()
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn save_config(&mut self) {
        let saved = self.s().saved;
        match self.env.paths.save_config(&self.env.config) {
            Ok(()) => self.toast(saved),
            Err(e) => self.toast(format!("{e:#}")),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        // Modal handling first.
        match &self.modal {
            Some(Modal::Cleaning) => return,
            Some(Modal::Confirm) => {
                match key.code {
                    KeyCode::Enter | KeyCode::Char('y') => self.start_clean(),
                    KeyCode::Char('t') => self.mode = next_mode(self.mode),
                    KeyCode::Char('n') if key.modifiers.is_empty() => self.modal = None,
                    KeyCode::Char('d') => self.dry_run = !self.dry_run,
                    KeyCode::Esc => self.modal = None,
                    _ => {}
                }
                return;
            }
            Some(Modal::Done(_)) | Some(Modal::Help) => {
                if matches!(
                    key.code,
                    KeyCode::Esc
                        | KeyCode::Enter
                        | KeyCode::Char('q')
                        | KeyCode::Char('?')
                        | KeyCode::Char('i')
                ) {
                    self.modal = None;
                }
                return;
            }
            None => {}
        }
        if self.searching {
            match key.code {
                KeyCode::Esc => {
                    self.searching = false;
                    self.search.clear();
                }
                KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.search.pop();
                }
                KeyCode::Char(c) => {
                    self.search.push(c);
                    self.cursor = 0;
                }
                _ => {}
            }
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if ctrl => self.should_quit = true,
            KeyCode::Tab => self.tab = self.tab.next(),
            KeyCode::BackTab => self.tab = self.tab.prev(),
            KeyCode::Char('1') => self.tab = Tab::Overview,
            KeyCode::Char('2') => self.tab = Tab::Candidates,
            KeyCode::Char('3') => self.tab = Tab::Git,
            KeyCode::Char('4') => {
                self.tab = Tab::Explore;
                if self.explore_nodes.is_empty() {
                    let d = self.explore_dir.clone();
                    self.explore(d);
                }
            }
            KeyCode::Char('5') => self.tab = Tab::History,
            KeyCode::Char('6') => self.tab = Tab::Settings,
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char('r') => self.start_scan(),
            KeyCode::Char('L') => {
                self.lang = self.lang.toggle();
                self.env.config.ui.language = self.lang;
                self.save_config();
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::PageDown => self.move_cursor(15),
            KeyCode::PageUp => self.move_cursor(-15),
            KeyCode::Char('g') | KeyCode::Home => *self.cursor_mut() = 0,
            KeyCode::Char('G') | KeyCode::End => {
                let len = self.list_len();
                *self.cursor_mut() = len.saturating_sub(1);
            }
            _ => {}
        }
        match self.tab {
            Tab::Candidates | Tab::Git => self.handle_list_key(key),
            Tab::Settings => self.handle_settings_key(key),
            Tab::Explore => self.handle_explore_key(key),
            Tab::History if key.code == KeyCode::Char('u') => self.undo_selected_history(),
            _ => {}
        }
        if self.tab == Tab::Explore && self.explore_nodes.is_empty() && !self.explore_loading {
            let d = self.explore_dir.clone();
            self.explore(d);
        }
    }

    fn handle_explore_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(n) = self.explore_nodes.get(self.explore_cursor) {
                    if n.is_dir {
                        let d = n.path.clone();
                        self.explore(d);
                    }
                }
            }
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                if let Some(parent) = self.explore_dir.parent() {
                    let d = parent.to_path_buf();
                    self.explore(d);
                }
            }
            KeyCode::Char('~') => {
                let d = self.env.paths.home.clone();
                self.explore(d);
            }
            KeyCode::Char('V') => {
                // jump to the next volume root
                if !self.volumes.is_empty() {
                    let cur = self
                        .volumes
                        .iter()
                        .position(|v| self.explore_dir.starts_with(&v.mount))
                        .unwrap_or(0);
                    let next = &self.volumes[(cur + 1) % self.volumes.len()];
                    let d = next.mount.clone();
                    self.explore(d);
                }
            }
            KeyCode::Char('d') => {
                if let Some(n) = self.explore_nodes.get(self.explore_cursor).cloned() {
                    let c = Candidate::new(
                        n.name.clone(),
                        "Selected in the explorer.",
                        Category::Temp,
                        Risk::High,
                        cleanprojects_core::Action::RemovePath {
                            path: n.path.clone(),
                        },
                    )
                    .with_size(n.size, true);
                    if !self.candidates.iter().any(|x| x.id == c.id) {
                        self.candidates.push(c.clone());
                    }
                    self.selected.insert(c.id);
                    self.toast("added to selection (Candidates tab, d to clean)");
                }
            }
            _ => {}
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(' ') | KeyCode::Enter
                if !matches!(key.code, KeyCode::Enter) || self.selected.is_empty() =>
            {
                if let Some(c) = self.current() {
                    let id = c.id.clone();
                    if self.env.state.excluded.contains(&id) {
                        return;
                    }
                    if !self.selected.remove(&id) {
                        self.selected.insert(id);
                    }
                }
            }
            KeyCode::Char('a') => {
                let ids: Vec<String> = self
                    .visible_all()
                    .iter()
                    .filter(|c| c.risk == Risk::Low)
                    .map(|c| c.id.clone())
                    .collect();
                let excluded = self.env.state.excluded.clone();
                for id in ids {
                    if !excluded.contains(&id) {
                        self.selected.insert(id);
                    }
                }
            }
            KeyCode::Char('A') => self.selected.clear(),
            KeyCode::Char('x') => {
                if let Some(c) = self.current() {
                    let id = c.id.clone();
                    let now = self.env.state.toggle_excluded(&id);
                    if now {
                        self.selected.remove(&id);
                    }
                    let _ = self.env.paths.save_state(&self.env.state);
                    let msg = if now { self.s().excluded } else { "restored" };
                    self.toast(msg);
                }
            }
            KeyCode::Char('/') => {
                self.searching = true;
            }
            KeyCode::Esc => {
                self.search.clear();
                self.risk_filter = None;
                self.category_filter = None;
                self.view_filter = ViewFilter::All;
            }
            KeyCode::Char('s') => {
                self.sort = match self.sort {
                    Sort::Size => Sort::Name,
                    Sort::Name => Sort::Age,
                    Sort::Age => Sort::Risk,
                    Sort::Risk => Sort::Size,
                };
            }
            KeyCode::Char('f') => {
                self.risk_filter = match self.risk_filter {
                    None => Some(Risk::Low),
                    Some(Risk::Low) => Some(Risk::Medium),
                    Some(Risk::Medium) => Some(Risk::High),
                    Some(Risk::High) => None,
                };
                self.cursor = 0;
            }
            KeyCode::Char('c') => {
                self.category_filter = match self.category_filter {
                    None => Some(Category::ALL[0]),
                    Some(cur) => {
                        let i = Category::ALL.iter().position(|c| *c == cur).unwrap_or(0);
                        Category::ALL.get(i + 1).copied()
                    }
                };
                self.cursor = 0;
            }
            KeyCode::Char('v') => {
                self.view_filter = match self.view_filter {
                    ViewFilter::All => ViewFilter::Stale,
                    ViewFilter::Stale => ViewFilter::Excluded,
                    ViewFilter::Excluded => ViewFilter::All,
                };
                self.cursor = 0;
            }
            KeyCode::Char('i') => {
                if self.current().is_some() {
                    self.show_detail = !self.show_detail;
                }
            }
            KeyCode::Char('t') => {
                self.mode = next_mode(self.mode);
                let msg = mode_label(self.mode, self.s());
                self.toast(msg);
            }
            KeyCode::Char('d') | KeyCode::Enter => {
                if !self.selected.is_empty() {
                    self.dry_run = false;
                    self.modal = Some(Modal::Confirm);
                }
            }
            KeyCode::Char('E') => {
                let path = self.env.paths.home.join(format!(
                    "cleanprojects-report-{}.md",
                    chrono::Local::now().format("%Y-%m-%d")
                ));
                let ctx = self.ctx();
                let result = cleanprojects_core::ScanResult {
                    candidates: self.candidates.clone(),
                    disk: self.disk.clone(),
                    volumes: self.volumes.clone(),
                    guarded: self.guarded,
                    roots: ctx.roots,
                    protected: ctx.protected,
                    projects: self.scan_stats.map(|s| s.0).unwrap_or(0),
                    repos: self.scan_stats.map(|s| s.1).unwrap_or(0),
                    started_at: self
                        .env
                        .state
                        .last_scan_at
                        .unwrap_or_else(chrono::Local::now),
                    duration_ms: self.scan_stats.map(|s| s.2).unwrap_or(0),
                };
                let items: Vec<&Candidate> = self.visible_all();
                let md = crate::cli::commands::markdown_report(&self.env, &result, &items);
                match std::fs::write(&path, md) {
                    Ok(()) => self.toast(format!("→ {}", path.display())),
                    Err(e) => self.toast(format!("{e}")),
                }
            }
            _ => {}
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        let dir: i64 = match key.code {
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right | KeyCode::Char('l') => 1,
            KeyCode::Left | KeyCode::Char('h') => -1,
            _ => return,
        };
        let cfg = &mut self.env.config;
        match self.settings_cursor {
            0 => {
                let m = next_mode(CleanMode::parse(&cfg.clean.mode));
                cfg.clean.mode = m.as_str().to_string();
                cfg.clean.use_trash = m != CleanMode::Permanent;
            }
            1 => cfg.git.enabled = !cfg.git.enabled,
            2 => cfg.git.merged_branches = !cfg.git.merged_branches,
            3 => cfg.git.gc = !cfg.git.gc,
            4 => {
                cfg.ui.language = cfg.ui.language.toggle();
                self.lang = cfg.ui.language;
            }
            5 => cfg.scan.stale_days = (cfg.scan.stale_days as i64 + 5 * dir).clamp(1, 3650) as u64,
            6 => {
                cfg.scan.min_size_mb = (cfg.scan.min_size_mb as i64 + dir).clamp(0, 100_000) as u64
            }
            7 => cfg.scan.max_depth = (cfg.scan.max_depth as i64 + dir).clamp(1, 16) as usize,
            8 => cfg.scan.default_roots = !cfg.scan.default_roots,
            9 => cfg.scan.volumes = !cfg.scan.volumes,
            10 => cfg.clean.block_running_apps = !cfg.clean.block_running_apps,
            11 => {
                cfg.clean.recovery_days =
                    (cfg.clean.recovery_days as i64 + dir).clamp(1, 365) as u64
            }
            _ => {}
        }
        self.mode = CleanMode::parse(&self.env.config.clean.mode);
        self.save_config();
    }
}

pub fn next_mode(m: CleanMode) -> CleanMode {
    match m {
        CleanMode::Recovery => CleanMode::Trash,
        CleanMode::Trash => CleanMode::Permanent,
        CleanMode::Permanent => CleanMode::Recovery,
    }
}

pub fn mode_label(m: CleanMode, s: &Strings) -> &'static str {
    match m {
        CleanMode::Recovery => s.confirm_recovery,
        CleanMode::Trash => s.confirm_trash,
        CleanMode::Permanent => s.confirm_permanent,
    }
}

/// Entry point for the `tui` subcommand (and the default).
pub fn run(env: Env, args: ScanArgs) -> Result<()> {
    let mut app = App::new(env, args);
    app.start_scan();
    let mut terminal = ratatui::init();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        hook(info);
    }));
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let tick = Duration::from_millis(60);
    loop {
        app.pump();
        if let Some((_, at)) = &app.toast {
            if at.elapsed() > Duration::from_millis(2500) {
                app.toast = None;
            }
        }
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(tick)? {
            match event::read()? {
                Event::Key(key) => app.handle_key(key),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        app.tick = app.tick.wrapping_add(1);
        if app.should_quit && app.clean_rx.is_none() {
            return Ok(());
        }
    }
}
