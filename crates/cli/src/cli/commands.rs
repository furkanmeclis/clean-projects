//! Implementations of the non-interactive subcommands.

use std::io::{self, IsTerminal, Write};

/// `println!` that ignores a closed pipe (`cleanprojects scan | head`).
macro_rules! out {
    ($($arg:tt)*) => {{
        let mut lock = io::stdout().lock();
        let _ = writeln!(lock, $($arg)*);
    }};
}
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Local;
use clap::CommandFactory;

use crate::cli::{
    CleanCmd, Cli, ConfigCmd, FilterArgs, ModeArg, RiskArg, RoutineCmd, ScanArgs, ScanCmd,
};
use cleanprojects_core::core::cleaner::{self, CleanEvent, CleanMode, CleanOptions};
use cleanprojects_core::core::fsutil::{compact_path, ellipsize_end, ellipsize_middle, human_size};
use cleanprojects_core::core::recovery;
use cleanprojects_core::core::state::{Config, Paths, State};
use cleanprojects_core::core::{Candidate, Category, Risk};
use cleanprojects_core::routine::{self, Routine, Schedule};
use cleanprojects_core::scan::{self, ScanContext, ScanEvent, ScanResult};
use cleanprojects_core::{Volume, VolumeKind};

pub struct Env {
    pub paths: Paths,
    pub config: Config,
    pub state: State,
    pub color: bool,
}

impl Env {
    pub fn load(cli: &Cli) -> Result<Env> {
        let mut paths = Paths::discover()?;
        if let Some(dir) = &cli.config_dir {
            paths = Paths::at(paths.home.clone(), dir.clone());
        }
        let mut config = paths.load_config()?;
        if let Some(lang) = cli.lang_override() {
            config.ui.language = lang;
        }
        let state = paths.load_state()?;
        let color =
            !cli.no_color && std::env::var_os("NO_COLOR").is_none() && io::stdout().is_terminal();
        Ok(Env {
            paths,
            config,
            state,
            color,
        })
    }

    fn c(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn bold(&self, s: &str) -> String {
        self.c("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.c("2", s)
    }
    pub fn orange(&self, s: &str) -> String {
        self.c("38;5;208", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.c("31", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.c("32", s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.c("33", s)
    }
}

/// Build a scan context from config + CLI overrides.
pub fn build_context(env: &Env, args: &ScanArgs) -> ScanContext {
    let mut config = env.config.clone();
    if args.only_roots {
        config.scan.default_roots = false;
        config.scan.roots.clear();
    }
    if let Some(mb) = args.min_size {
        config.scan.min_size_mb = mb;
    }
    for r in &args.roots {
        config.scan.roots.push(r.display().to_string());
    }
    if args.no_volumes || args.only_roots {
        config.scan.volumes = false;
    }
    let mut ctx = ScanContext::new(env.paths.home.clone(), config);
    if args.no_git {
        ctx.include_git = false;
    }
    ctx.include_global = !args.no_global;
    ctx.include_projects = !args.no_projects;
    ctx.include_tools = !args.no_tools;
    ctx.include_extras = !args.no_extras && !args.only_roots;
    let cache = cleanprojects_core::load_cache(&env.paths.scan_cache_file);
    ctx.with_previous(cache.as_ref())
}

/// The configured disposal mode, with CLI overrides.
pub fn clean_mode(env: &Env, permanent: bool, mode: Option<ModeArg>) -> CleanMode {
    if permanent {
        return CleanMode::Permanent;
    }
    match mode {
        Some(ModeArg::Recovery) => CleanMode::Recovery,
        Some(ModeArg::Trash) => CleanMode::Trash,
        Some(ModeArg::Permanent) => CleanMode::Permanent,
        None => CleanMode::parse(&env.config.clean.mode),
    }
}

/// Build cleaner options from the environment.
pub fn clean_options(
    env: &Env,
    ctx: &ScanContext,
    dry_run: bool,
    mode: CleanMode,
    force: bool,
) -> CleanOptions {
    CleanOptions {
        dry_run,
        mode,
        home: env.paths.home.clone(),
        protected: ctx.protected.clone(),
        data_dir: env.paths.data_dir.clone(),
        volumes: ctx.volumes.clone(),
        block_running_apps: env.config.clean.block_running_apps && !force,
    }
}

/// Run a scan with a live progress line on stderr.
pub fn run_scan(env: &Env, ctx: &ScanContext, quiet: bool) -> ScanResult {
    run_scan_with(env, ctx, quiet, false)
}

/// Run a scan; with `stream` every candidate is printed as JSON when found.
pub fn run_scan_with(env: &Env, ctx: &ScanContext, quiet: bool, stream: bool) -> ScanResult {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx2 = ctx.clone();
    let handle = std::thread::spawn(move || scan::scan(&ctx2, tx));
    let mut found = 0usize;
    let mut total = 0u64;
    let show = !quiet && io::stderr().is_terminal();
    for ev in rx {
        match ev {
            ScanEvent::Progress { phase, message } => {
                if show {
                    eprint!(
                        "\r\x1b[2K{} {} {}",
                        env.orange("●"),
                        env.bold(phase),
                        env.dim(&ellipsize_middle(&message, 70))
                    );
                }
            }
            ScanEvent::Found(c) => {
                found += 1;
                total += c.size;
                if stream {
                    if let Ok(line) = serde_json::to_string(&*c) {
                        out!("{line}");
                    }
                }
                if show {
                    eprint!(
                        "\r\x1b[2K{} {found} candidates · {}",
                        env.orange("●"),
                        human_size(total)
                    );
                }
            }
            ScanEvent::Done(_) => break,
        }
    }
    if show {
        eprint!("\r\x1b[2K");
    }
    let result = handle.join().expect("scan thread panicked");
    let _ = cleanprojects_core::save_cache(&env.paths.scan_cache_file, &result);
    if let Ok(mut st) = env.paths.load_state() {
        st.record_scan(
            result.total(),
            result.disk.free,
            result.disk.total,
            result.candidates.len(),
        );
        let _ = env.paths.save_state(&st);
    }
    result
}

fn max_risk(r: RiskArg) -> Risk {
    match r {
        RiskArg::Low => Risk::Low,
        RiskArg::Medium => Risk::Medium,
        RiskArg::High => Risk::High,
    }
}

/// Apply CLI filters to scan results.
pub fn apply_filter<'a>(
    env: &Env,
    result: &'a ScanResult,
    f: &FilterArgs,
) -> Result<Vec<&'a Candidate>> {
    let mut cats: Vec<Category> = Vec::new();
    for c in &f.categories {
        cats.push(Category::parse(c).with_context(|| format!("unknown category '{c}'"))?);
    }
    let max = max_risk(f.max_risk);
    let q = f.query.as_ref().map(|q| q.to_ascii_lowercase());
    Ok(result
        .candidates
        .iter()
        .filter(|c| {
            if !f.ids.is_empty() {
                return f.ids.iter().any(|id| id == &c.id);
            }
            if !f.include_excluded && env.state.excluded.contains(&c.id) {
                return false;
            }
            if !cats.is_empty() && !cats.contains(&c.category) {
                return false;
            }
            if c.risk > max {
                return false;
            }
            if f.stale && !c.is_stale() {
                return false;
            }
            if let Some(q) = &q {
                let hay = format!("{} {} {}", c.label, c.detail(), c.category.as_str())
                    .to_ascii_lowercase();
                if !hay.contains(q) {
                    return false;
                }
            }
            true
        })
        .collect())
}

fn risk_str(env: &Env, r: Risk) -> String {
    match r {
        Risk::Low => env.green("low   "),
        Risk::Medium => env.yellow("medium"),
        Risk::High => env.red("HIGH  "),
    }
}

pub fn print_table(env: &Env, items: &[&Candidate]) {
    let width = terminal_width();
    let path_w = width
        .saturating_sub(16 + 2 + 12 + 2 + 8 + 2 + 9 + 2 + 30)
        .clamp(20, 90);
    out!(
        "{}",
        env.bold(&format!(
            "{:<16}  {:<14}  {:<8}  {:>9}  {:<30}  {}",
            "ID", "CATEGORY", "RISK", "SIZE", "ITEM", "PATH / COMMAND"
        ))
    );
    for c in items {
        let size = if c.size_exact {
            human_size(c.size)
        } else if c.size == 0 {
            "?".to_string()
        } else {
            format!("~{}", human_size(c.size))
        };
        let mut label = c.label.clone();
        if c.is_stale() {
            label.push_str(" ⏳");
        }
        if env.state.excluded.contains(&c.id) {
            label.push_str(" (excluded)");
        }
        if let Some(app) = &c.blocked_by {
            label.push_str(&format!(" ⛔{app}"));
        }
        if c.tags.iter().any(|t| t == "new") {
            label.push_str(" ✦");
        }
        out!(
            "{}  {:<14}  {}  {:>9}  {:<30}  {}",
            env.dim(&c.id),
            c.category.as_str(),
            risk_str(env, c.risk),
            size,
            ellipsize_middle(&label, 30),
            env.dim(&ellipsize_middle(
                &compact_path(std::path::Path::new(&c.detail()), &env.paths.home),
                path_w
            ))
        );
    }
}

fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(120)
}

pub fn cmd_scan(env: &Env, cmd: &ScanCmd) -> Result<()> {
    let ctx = build_context(env, &cmd.scan);
    let result = run_scan_with(env, &ctx, cmd.json || cmd.json_stream, cmd.json_stream);
    let items = apply_filter(env, &result, &cmd.filter)?;
    if cmd.json_stream {
        return Ok(());
    }
    if cmd.json {
        let out = serde_json::json!({
            "scanned_at": result.started_at,
            "duration_ms": result.duration_ms,
            "disk": result.disk,
            "roots": result.roots,
            "projects": result.projects,
            "repos": result.repos,
            "guarded": result.guarded,
            "volumes": result.volumes,
            "total_bytes": items.iter().map(|c| c.size).sum::<u64>(),
            "candidates": items,
        });
        out!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        print_table(env, &items);
        let total: u64 = items.iter().map(|c| c.size).sum();
        out!();
        out!(
            "{} candidates · {} recoverable · disk {} free of {} · {} projects, {} repos, {} guarded · {} volumes · {:.1}s",
            env.bold(&items.len().to_string()),
            env.orange(&human_size(total)),
            human_size(result.disk.free),
            human_size(result.disk.total),
            result.projects,
            result.repos,
            result.guarded,
            result.volumes.len(),
            result.duration_ms as f64 / 1000.0
        );
        if items.len() < result.candidates.len() {
            out!(
                "{}",
                env.dim(&format!(
                    "{} more hidden by filters (risk > {:?}, excluded, category). Use --max-risk high / --include-excluded.",
                    result.candidates.len() - items.len(),
                    cmd.filter.max_risk
                ))
            );
        }
    }
    if let Some(path) = &cmd.report {
        let format = cleanprojects_core::ReportFormat::from_path(path);
        std::fs::write(
            path,
            cleanprojects_core::report::render(&result, &items, &env.paths.home, format),
        )?;
        eprintln!("report written to {} ({:?})", path.display(), format);
    }
    Ok(())
}

pub fn markdown_report(env: &Env, result: &ScanResult, items: &[&Candidate]) -> String {
    cleanprojects_core::report::render(
        result,
        items,
        &env.paths.home,
        cleanprojects_core::ReportFormat::Markdown,
    )
}

pub fn cmd_clean(env: &mut Env, cmd: &CleanCmd) -> Result<()> {
    let ctx = build_context(env, &cmd.scan);
    let result = run_scan(env, &ctx, cmd.json);
    let items: Vec<Candidate> = apply_filter(env, &result, &cmd.filter)?
        .into_iter()
        .cloned()
        .collect();
    if items.is_empty() {
        if cmd.json {
            out!("{{\"freed\":0,\"items\":[]}}");
        } else {
            out!("Nothing matches. Run `cleanprojects scan` to see candidates.");
        }
        return Ok(());
    }
    let total: u64 = items.iter().map(|c| c.size).sum();
    let mode = clean_mode(env, cmd.permanent, cmd.mode);
    if !cmd.json {
        let refs: Vec<&Candidate> = items.iter().collect();
        print_table(env, &refs);
        out!();
        let mode = if cmd.dry_run {
            env.dim("dry run")
        } else {
            match mode {
                CleanMode::Recovery => env.green("recovery bin (undo with `cleanprojects undo`)"),
                CleanMode::Trash => env.green("move to trash"),
                CleanMode::Permanent => env.red("PERMANENT delete"),
            }
        };
        out!(
            "{} items · {} · mode: {}",
            env.bold(&items.len().to_string()),
            env.orange(&human_size(total)),
            mode
        );
        if items.iter().any(|c| c.risk == Risk::High) {
            out!(
                "{}",
                env.red("Contains HIGH risk items. Close the related applications first.")
            );
        }
    }
    if !cmd.yes && !cmd.dry_run {
        if !io::stdin().is_terminal() {
            anyhow::bail!("refusing to clean without --yes when stdin is not a terminal");
        }
        print!("Proceed? [y/N] ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if !matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            out!("Cancelled.");
            return Ok(());
        }
    }
    let opts = clean_options(env, &ctx, cmd.dry_run, mode, cmd.force);
    let json = cmd.json;
    let env_ref = &*env;
    let outcome = cleaner::run_with_journal(&items, &opts, &mut |ev| {
        if json {
            return;
        }
        match ev {
            CleanEvent::Start {
                index,
                total,
                label,
                ..
            } => {
                out!(
                    "{} {}",
                    env_ref.orange(&format!("[{}/{}]", index + 1, total)),
                    env_ref.bold(&label)
                );
            }
            CleanEvent::Log(line) => out!("{}", env_ref.dim(&line)),
            CleanEvent::Item { ok, freed, .. } => {
                if ok {
                    out!("  {} {}", env_ref.green("✓"), human_size(freed));
                }
            }
            CleanEvent::Done { freed, ok, failed } => {
                out!();
                out!(
                    "{} {} freed · {} ok · {} failed",
                    env_ref.green("done:"),
                    env_ref.orange(&human_size(freed)),
                    ok,
                    failed
                );
            }
        }
    });
    let entry = outcome.entry;
    if !outcome.recovered.is_empty() {
        let mut journal = env.paths.load_journal();
        cleaner::record_recovered(&mut journal, outcome.recovered);
        env.paths.save_journal(&journal)?;
    }
    env.paths.append_log(&format!(
        "clean dry_run={} mode={} freed={} ok={} failed={}",
        entry.dry_run,
        entry.mode,
        entry.freed,
        entry.items.iter().filter(|i| i.ok).count(),
        entry.items.iter().filter(|i| !i.ok).count()
    ));
    if json {
        out!("{}", serde_json::to_string_pretty(&entry)?);
    }
    env.state.history.push(entry);
    env.paths.save_state(&env.state)?;
    Ok(())
}

pub fn cmd_exclude(env: &mut Env, targets: &[String]) -> Result<()> {
    if targets.is_empty() {
        anyhow::bail!("give at least one candidate id or path");
    }
    for t in targets {
        let id = if t.len() == 16 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            t.clone()
        } else {
            let p = PathBuf::from(t);
            let p = if p.is_absolute() {
                p
            } else {
                std::env::current_dir()?.join(p)
            };
            cleanprojects_core::core::candidate::stable_id(
                &cleanprojects_core::core::Action::RemovePath { path: p },
            )
        };
        env.state.excluded.insert(id.clone());
        out!("excluded {id}");
    }
    env.paths.save_state(&env.state)
}

pub fn cmd_include(env: &mut Env, targets: &[String], all: bool) -> Result<()> {
    if all {
        let n = env.state.excluded.len();
        env.state.excluded.clear();
        out!("removed {n} exclusions");
    } else {
        for t in targets {
            if env.state.excluded.remove(t) {
                out!("included {t}");
            } else {
                out!("{t} was not excluded");
            }
        }
    }
    env.paths.save_state(&env.state)
}

pub fn cmd_history(env: &Env, json: bool, limit: usize) -> Result<()> {
    let entries: Vec<_> = env.state.history.iter().rev().take(limit).collect();
    if json {
        out!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }
    if entries.is_empty() {
        out!("No cleanups yet.");
        print_trend(env);
        return Ok(());
    }
    for e in entries {
        let ok = e.items.iter().filter(|i| i.ok).count();
        let failed = e.items.len() - ok;
        out!(
            "{}  {}  {} items, {} failed{}{}",
            env.bold(&e.at.format("%Y-%m-%d %H:%M").to_string()),
            env.orange(&format!("{:>9}", human_size(e.freed))),
            ok,
            failed,
            if e.dry_run { "  (dry run)" } else { "" },
            if e.trashed { "  (trash)" } else { "" }
        );
        for i in &e.items {
            let mark = if i.ok {
                env.green("✓")
            } else {
                env.red("✗")
            };
            out!(
                "   {mark} {:>9}  {}  {}",
                human_size(i.size),
                i.label,
                env.dim(&i.detail)
            );
        }
    }
    out!(
        "\nTotal freed over time: {}",
        env.orange(&human_size(env.state.total_freed()))
    );
    print_trend(env);
    Ok(())
}

/// Recoverable / free space over the last scans as a compact sparkline.
pub fn print_trend(env: &Env) {
    let scans = &env.state.scans;
    if scans.len() < 2 {
        return;
    }
    let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let spark = |vals: Vec<u64>| -> String {
        let max = vals.iter().copied().max().unwrap_or(1).max(1);
        vals.iter()
            .map(|v| bars[((*v as f64 / max as f64) * 7.0).round() as usize])
            .collect()
    };
    let recent: Vec<_> = scans
        .iter()
        .rev()
        .take(30)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let first = recent.first().unwrap();
    let last = recent.last().unwrap();
    out!(
        "\n{} {}  {} → {}  ({} scans since {})",
        env.bold("recoverable"),
        env.orange(&spark(recent.iter().map(|s| s.recoverable).collect())),
        human_size(first.recoverable),
        human_size(last.recoverable),
        recent.len(),
        first.at.format("%Y-%m-%d")
    );
    out!(
        "{} {}  {} → {}",
        env.bold("free space "),
        env.green(&spark(recent.iter().map(|s| s.free).collect())),
        human_size(first.free),
        human_size(last.free)
    );
}

pub fn cmd_large(
    env: &Env,
    path: Option<PathBuf>,
    min_mb: u64,
    top: usize,
    json: bool,
) -> Result<()> {
    let ctx = ScanContext::new(env.paths.home.clone(), env.config.clone());
    let roots = match path {
        Some(p) => vec![p],
        None => {
            let mut r = vec![env.paths.home.clone()];
            r.extend(
                ctx.roots
                    .iter()
                    .filter(|p| !p.starts_with(&env.paths.home))
                    .cloned(),
            );
            r
        }
    };
    let files = cleanprojects_core::large_files(&roots, min_mb * 1024 * 1024, top, &ctx.protected);
    if json {
        out!("{}", serde_json::to_string_pretty(&files)?);
        return Ok(());
    }
    if files.is_empty() {
        out!(
            "No files of {min_mb} MB or more under {}.",
            roots
                .iter()
                .map(|r| compact_path(r, &env.paths.home))
                .collect::<Vec<_>>()
                .join(", ")
        );
        return Ok(());
    }
    let width = terminal_width().clamp(60, 160);
    for f in &files {
        out!(
            "{:>9}  {}  {}",
            env.orange(&human_size(f.size)),
            env.dim(
                &f.modified
                    .map(|m| m.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "          ".into())
            ),
            ellipsize_middle(
                &compact_path(&f.path, &env.paths.home),
                width.saturating_sub(24)
            )
        );
    }
    out!(
        "\n{} files · {}",
        files.len(),
        env.orange(&human_size(files.iter().map(|f| f.size).sum()))
    );
    Ok(())
}

pub fn cmd_config(env: &mut Env, cmd: &ConfigCmd) -> Result<()> {
    if cmd.path {
        out!("{}", env.paths.config_file.display());
        return Ok(());
    }
    if cmd.reset {
        env.config = Config::default();
        env.paths.save_config(&env.config)?;
        out!("config reset to defaults");
    }
    if cmd.init && !env.paths.config_file.exists() {
        env.paths.save_config(&env.config)?;
        out!("wrote {}", env.paths.config_file.display());
    }
    if !cmd.sets.is_empty() {
        for kv in &cmd.sets {
            let (k, v) = kv.split_once('=').context("expected KEY=VALUE")?;
            env.config.set(k.trim(), v)?;
        }
        env.paths.save_config(&env.config)?;
        out!("saved {}", env.paths.config_file.display());
    }
    if !cmd.reset && !cmd.init && cmd.sets.is_empty() {
        out!(
            "{}",
            env.dim(&format!("# {}", env.paths.config_file.display()))
        );
        print!("{}", env.config.to_toml());
        out!(
            "{}",
            env.dim(&format!("# keys: {}", Config::keys().join(", ")))
        );
    }
    Ok(())
}

pub fn cmd_doctor(env: &Env) -> Result<()> {
    out!("{}", env.bold("CleanProjects doctor"));
    out!("version      {}", env!("CARGO_PKG_VERSION"));
    out!(
        "os           {} / {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    out!("home         {}", env.paths.home.display());
    out!(
        "config       {}{}",
        env.paths.config_file.display(),
        if env.paths.config_file.exists() {
            ""
        } else {
            "  (defaults)"
        }
    );
    out!("state        {}", env.paths.state_file.display());
    out!("log          {}", env.paths.log_file.display());
    let ctx = ScanContext::new(env.paths.home.clone(), env.config.clone());
    out!("\n{}", env.bold("tools"));
    for (t, ok) in scan::tools::tool_report() {
        out!("  {} {}", if ok { env.green("✓") } else { env.dim("·") }, t);
    }
    out!("\n{}", env.bold("project roots"));
    if ctx.roots.is_empty() {
        out!(
            "  {}",
            env.yellow(
                "none found — add roots with `cleanprojects config --set scan.roots=~/code`"
            )
        );
    }
    for r in &ctx.roots {
        out!("  {}", compact_path(r, &env.paths.home));
    }
    out!("\n{}", env.bold("protected (never scanned or deleted)"));
    for p in &ctx.protected {
        if p.exists() {
            out!("  {}", compact_path(p, &env.paths.home));
        }
    }
    out!("\n{}", env.bold("global rules for this OS"));
    let n = scan::rules::rules_for_current_os().count();
    let present = scan::rules::rules_for_current_os()
        .filter_map(|r| scan::rules::resolve_template(r.path, &env.paths.home))
        .filter(|p| p.exists())
        .count();
    out!("  {present} of {n} locations exist on this machine");
    out!("\n{}", env.bold("volumes"));
    for v in &ctx.volumes {
        out!("  {}", volume_line(env, v));
    }
    let journal = env.paths.load_journal();
    out!("\n{}", env.bold("recovery bin"));
    out!(
        "  {} items · {} · purge after {} days · {}",
        journal.entries.len(),
        human_size(journal.total_size()),
        env.config.clean.recovery_days,
        env.paths.data_dir.join("recovery").display()
    );
    out!(
        "\n{}",
        env.dim(&format!(
            "generated {}",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        ))
    );
    Ok(())
}

pub fn cmd_completions(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "cleanprojects", &mut io::stdout());
}

fn volume_line(env: &Env, v: &Volume) -> String {
    let kind = match v.kind {
        VolumeKind::System => env.dim("system  "),
        VolumeKind::External => env.orange("external"),
        VolumeKind::Network => env.yellow("network "),
    };
    format!(
        "{kind}  {:<24} {:>9} free of {:>9} ({:.0}% used)  {}{}",
        ellipsize_middle(&v.name, 24),
        human_size(v.free),
        human_size(v.total),
        v.used_percent(),
        env.dim(&v.mount.display().to_string()),
        if v.read_only {
            env.dim("  read-only")
        } else {
            String::new()
        }
    )
}

pub fn cmd_volumes(env: &Env, json: bool) -> Result<()> {
    let vols = cleanprojects_core::volumes(&env.paths.home);
    if json {
        out!("{}", serde_json::to_string_pretty(&vols)?);
        return Ok(());
    }
    if vols.is_empty() {
        out!("No volumes reported by the operating system.");
    }
    for v in &vols {
        out!("{}", volume_line(env, v));
    }
    let scanned = if env.config.scan.volumes {
        "external volumes are scanned for projects"
    } else {
        "external volumes are not scanned (scan.volumes = false)"
    };
    out!("\n{}", env.dim(scanned));
    Ok(())
}

pub fn cmd_undo(env: &mut Env, run: usize, id: Option<String>) -> Result<()> {
    let mut journal = env.paths.load_journal();
    if let Some(id) = id {
        let Some(pos) = journal.entries.iter().position(|e| e.id == id) else {
            anyhow::bail!("{id} is not in the recovery bin");
        };
        recovery::restore(&journal.entries[pos])?;
        let e = journal.entries.remove(pos);
        env.paths.save_journal(&journal)?;
        out!("{} restored to {}", env.green("✓"), e.original.display());
        return Ok(());
    }
    let n = env.state.history.len();
    if run == 0 || run > n {
        anyhow::bail!("no such run; {n} runs in history");
    }
    let entry = env.state.history[n - run].clone();
    if entry.restorable() == 0 {
        anyhow::bail!(
            "that run has nothing in the recovery bin (mode: {})",
            if entry.mode.is_empty() {
                "unknown"
            } else {
                &entry.mode
            }
        );
    }
    let (restored, errors) = cleaner::undo(&entry, &mut journal);
    env.paths.save_journal(&journal)?;
    for e in &errors {
        out!("{} {e}", env.red("✗"));
    }
    out!(
        "{} {restored} item(s) restored, {} failed",
        env.green("✓"),
        errors.len()
    );
    for item in env.state.history[n - run].items.iter_mut() {
        if item.recovery_id.is_some() && !errors.iter().any(|e| e.contains(&item.label)) {
            item.recovery_id = None;
        }
    }
    env.paths.save_state(&env.state)?;
    Ok(())
}

pub fn cmd_recovery(env: &Env, purge: bool, empty: bool, json: bool) -> Result<()> {
    let mut journal = env.paths.load_journal();
    journal.prune_missing();
    if empty {
        let mut n = 0;
        let mut bytes = 0;
        for e in &journal.entries {
            if recovery::purge_entry(e).is_ok() {
                n += 1;
                bytes += e.size;
            }
        }
        journal.entries.clear();
        env.paths.save_journal(&journal)?;
        out!("emptied recovery bin: {n} items, {}", human_size(bytes));
        return Ok(());
    }
    if purge {
        let (n, bytes) = recovery::purge_older_than(&mut journal, env.config.clean.recovery_days);
        env.paths.save_journal(&journal)?;
        out!(
            "purged {n} items older than {} days, {}",
            env.config.clean.recovery_days,
            human_size(bytes)
        );
        return Ok(());
    }
    if json {
        out!("{}", serde_json::to_string_pretty(&journal)?);
        return Ok(());
    }
    if journal.entries.is_empty() {
        out!("Recovery bin is empty.");
        return Ok(());
    }
    for e in journal.entries.iter().rev() {
        out!(
            "{}  {}  {:>9}  {}  {}",
            env.dim(&e.id),
            e.run_at.format("%Y-%m-%d %H:%M"),
            human_size(e.size),
            env.bold(&e.label),
            env.dim(&compact_path(&e.original, &env.paths.home))
        );
    }
    out!(
        "\n{} items · {} · `cleanprojects undo --id <ID>` restores one, `cleanprojects recovery --purge` frees old ones",
        journal.entries.len(),
        env.orange(&human_size(journal.total_size()))
    );
    Ok(())
}

pub fn cmd_explore(env: &Env, path: Option<PathBuf>, top: usize, json: bool) -> Result<()> {
    let dir = path.unwrap_or_else(|| env.paths.home.clone());
    let nodes = cleanprojects_core::list_dir(&dir);
    if json {
        out!(
            "{}",
            serde_json::to_string_pretty(&nodes.iter().take(top).collect::<Vec<_>>())?
        );
        return Ok(());
    }
    let total: u64 = nodes.iter().map(|n| n.size).sum();
    out!(
        "{}  {}",
        env.bold(&dir.display().to_string()),
        env.orange(&human_size(total))
    );
    let width = terminal_width().clamp(60, 140);
    let bar_w = width.saturating_sub(12 + 2 + 8 + 2 + 40).clamp(10, 40);
    let max = nodes.first().map(|n| n.size).unwrap_or(1).max(1);
    for n in nodes.iter().take(top) {
        let filled = ((n.size as f64 / max as f64) * bar_w as f64).round() as usize;
        out!(
            "{:>9}  {}{}  {}{}",
            human_size(n.size),
            env.orange(&"█".repeat(filled)),
            env.dim(&"░".repeat(bar_w - filled)),
            if n.is_dir {
                env.bold(&n.name)
            } else {
                n.name.clone()
            },
            if n.is_dir { "/" } else { "" }
        );
    }
    if nodes.len() > top {
        out!(
            "{}",
            env.dim(&format!("… {} more entries", nodes.len() - top))
        );
    }
    Ok(())
}

pub const SKILL_MD: &str = include_str!("../../../../docs/agent-skill/SKILL.md");

pub fn cmd_skill(env: &Env, install: bool) -> Result<()> {
    if !install {
        out!("{}", SKILL_MD);
        return Ok(());
    }
    let dir = env.paths.home.join(".claude/skills/cleanprojects");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("SKILL.md");
    std::fs::write(&file, SKILL_MD)?;
    out!("installed {}", file.display());
    Ok(())
}

fn find_routine<'a>(env: &'a Env, name: &str) -> Result<&'a Routine> {
    env.config
        .routines
        .iter()
        .find(|r| r.id == name || r.name.eq_ignore_ascii_case(name))
        .with_context(|| format!("no routine named '{name}'; see `cleanprojects routine list`"))
}

pub fn cmd_routine(env: &mut Env, cmd: RoutineCmd) -> Result<()> {
    match cmd {
        RoutineCmd::List { json } => {
            if json {
                out!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({ "routines": env.config.routines, "runs": env.state.routine_runs })
                    )?
                );
                return Ok(());
            }
            if env.config.routines.is_empty() {
                out!("No routines. Try `cleanprojects routine add --templates` or `cleanprojects routine add --name \"Safe sweep\" --every weekly@sun@3`.");
                return Ok(());
            }
            let now = chrono::Local::now();
            for r in &env.config.routines {
                let last = r.last_run(&env.state);
                out!(
                    "{} {:<28} {:<24} risk≤{:<6} {:<18} {}  {}",
                    if r.enabled {
                        env.green("●")
                    } else {
                        env.dim("○")
                    },
                    ellipsize_end(&r.name, 28),
                    r.schedule.describe(),
                    r.max_risk.as_str(),
                    if r.categories.is_empty() {
                        "all categories".to_string()
                    } else {
                        r.categories
                            .iter()
                            .map(|c| c.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    },
                    env.dim(
                        &last
                            .map(|l| format!(
                                "last {} · {}",
                                l.at.format("%Y-%m-%d %H:%M"),
                                human_size(l.freed)
                            ))
                            .unwrap_or_else(|| "never run".into())
                    ),
                    if r.is_due(&env.state, now) {
                        env.orange("due")
                    } else {
                        String::new()
                    }
                );
            }
            out!("{}", env.dim("run with `cleanprojects routine run <name>` or install the scheduler with `cleanprojects routine schedule --install`"));
            Ok(())
        }
        RoutineCmd::Add {
            name,
            every,
            max_risk,
            categories,
            stale,
            min_age,
            mode,
            roots,
            templates,
        } => {
            if templates {
                let mut n = 0;
                for t in routine::templates() {
                    if !env.config.routines.iter().any(|r| r.name == t.name) {
                        env.config.routines.push(t);
                        n += 1;
                    }
                }
                env.paths.save_config(&env.config)?;
                out!("added {n} template routine(s)");
                return Ok(());
            }
            let name = name.context("--name is required (or use --templates)")?;
            let schedule = Schedule::parse(&every).with_context(|| format!("invalid schedule '{every}'; use daily@3, weekly@sun@4, monthly@1@5 or every@6h"))?;
            let mut cats = Vec::new();
            for c in &categories {
                cats.push(Category::parse(c).with_context(|| format!("unknown category '{c}'"))?);
            }
            let r = Routine {
                id: routine::new_id(),
                name,
                enabled: true,
                schedule,
                max_risk: max_risk_from(max_risk),
                categories: cats,
                stale_only: stale,
                min_age_days: min_age,
                mode: if mode.eq_ignore_ascii_case("trash") {
                    "trash".into()
                } else {
                    "recovery".into()
                },
                roots: roots.iter().map(|r| r.display().to_string()).collect(),
                notify: true,
            };
            out!("added '{}' ({})", r.name, r.schedule.describe());
            env.config.routines.push(r);
            env.paths.save_config(&env.config)
        }
        RoutineCmd::Run {
            name,
            due,
            dry_run,
            json,
        } => {
            let targets: Vec<Routine> = if due {
                routine::due(&env.config, &env.state, chrono::Local::now())
                    .into_iter()
                    .cloned()
                    .collect()
            } else {
                vec![find_routine(env, &name.context("give a routine name or --due")?)?.clone()]
            };
            if targets.is_empty() {
                if !json {
                    out!("Nothing is due.");
                }
                return Ok(());
            }
            let mut runs = Vec::new();
            for r in &targets {
                if !json {
                    out!(
                        "{} {} ({})",
                        env.orange("▶"),
                        env.bold(&r.name),
                        r.schedule.describe()
                    );
                }
                let run = routine::run(&env.paths, &env.config, &mut env.state, r, dry_run);
                if !json {
                    out!(
                        "  {} {} freed · {} items · {} failed{}",
                        env.green("✓"),
                        human_size(run.freed),
                        run.items,
                        run.failed,
                        if dry_run { "  (dry run)" } else { "" }
                    );
                }
                runs.push(run);
            }
            if json {
                out!("{}", serde_json::to_string_pretty(&runs)?);
            }
            Ok(())
        }
        RoutineCmd::Enable { name } => set_routine_enabled(env, &name, true),
        RoutineCmd::Disable { name } => set_routine_enabled(env, &name, false),
        RoutineCmd::Remove { name } => {
            let id = find_routine(env, &name)?.id.clone();
            env.config.routines.retain(|r| r.id != id);
            env.paths.save_config(&env.config)?;
            out!("removed");
            Ok(())
        }
        RoutineCmd::Schedule { install, uninstall } => {
            cmd_routine_schedule(env, install, uninstall)
        }
    }
}

fn set_routine_enabled(env: &mut Env, name: &str, enable: bool) -> Result<()> {
    let id = find_routine(env, name)?.id.clone();
    if let Some(r) = env.config.routines.iter_mut().find(|r| r.id == id) {
        r.enabled = enable;
        out!("{} {}", r.name, if enable { "enabled" } else { "disabled" });
    }
    env.paths.save_config(&env.config)
}

fn max_risk_from(r: RiskArg) -> Risk {
    match r {
        RiskArg::Low => Risk::Low,
        _ => Risk::Medium,
    }
}

/// Install an hourly OS job running `cleanprojects routine run --due`.
fn cmd_routine_schedule(env: &Env, install: bool, uninstall: bool) -> Result<()> {
    let exe = std::env::current_exe()?;
    let home = &env.paths.home;
    if cfg!(target_os = "macos") {
        let plist = home.join("Library/LaunchAgents/com.furkanmeclis.cleanprojects.routines.plist");
        if uninstall {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", &plist.display().to_string()])
                .output();
            if plist.exists() {
                std::fs::remove_file(&plist)?;
            }
            out!("removed {}", plist.display());
            return Ok(());
        }
        if !install {
            out!(
                "{}",
                if plist.exists() {
                    format!("installed: {}", plist.display())
                } else {
                    "not installed (use --install)".into()
                }
            );
            return Ok(());
        }
        std::fs::create_dir_all(plist.parent().unwrap())?;
        std::fs::write(
            &plist,
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.furkanmeclis.cleanprojects.routines</string>
  <key>ProgramArguments</key><array><string>{}</string><string>routine</string><string>run</string><string>--due</string></array>
  <key>StartInterval</key><integer>3600</integer>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict></plist>
"#,
                exe.display(),
                env.paths.log_file.display(),
                env.paths.log_file.display()
            ),
        )?;
        let _ = std::process::Command::new("launchctl")
            .args(["load", &plist.display().to_string()])
            .output();
        out!(
            "installed {} (hourly; runs whatever is due)",
            plist.display()
        );
    } else if cfg!(windows) {
        let task = "CleanProjects Routines";
        if uninstall {
            let _ = std::process::Command::new("schtasks")
                .args(["/Delete", "/TN", task, "/F"])
                .output();
            out!("removed scheduled task '{task}'");
            return Ok(());
        }
        if !install {
            let exists = std::process::Command::new("schtasks")
                .args(["/Query", "/TN", task])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            out!(
                "{}",
                if exists {
                    format!("installed: scheduled task '{task}'")
                } else {
                    "not installed (use --install)".into()
                }
            );
            return Ok(());
        }
        let cmdline = format!("\"{}\" routine run --due", exe.display());
        let o = std::process::Command::new("schtasks")
            .args([
                "/Create", "/F", "/SC", "HOURLY", "/TN", task, "/TR", &cmdline,
            ])
            .output()?;
        if !o.status.success() {
            anyhow::bail!("schtasks failed: {}", String::from_utf8_lossy(&o.stderr));
        }
        out!("installed scheduled task '{task}' (hourly)");
    } else {
        let dir = home.join(".config/systemd/user");
        let service = dir.join("cleanprojects-routines.service");
        let timer = dir.join("cleanprojects-routines.timer");
        if uninstall {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "disable", "--now", "cleanprojects-routines.timer"])
                .output();
            for f in [&service, &timer] {
                if f.exists() {
                    std::fs::remove_file(f)?;
                }
            }
            out!("removed systemd user timer");
            return Ok(());
        }
        if !install {
            out!(
                "{}",
                if timer.exists() {
                    format!("installed: {}", timer.display())
                } else {
                    "not installed (use --install)".into()
                }
            );
            return Ok(());
        }
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&service, format!("[Unit]\nDescription=CleanProjects routines\n\n[Service]\nType=oneshot\nExecStart={} routine run --due\n", exe.display()))?;
        std::fs::write(&timer, "[Unit]\nDescription=Run CleanProjects routines hourly\n\n[Timer]\nOnBootSec=5min\nOnUnitActiveSec=1h\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n")?;
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "enable", "--now", "cleanprojects-routines.timer"])
            .output();
        out!("installed {} (hourly)", timer.display());
    }
    Ok(())
}
