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

use crate::cli::{CleanCmd, Cli, ConfigCmd, FilterArgs, RiskArg, ScanArgs, ScanCmd};
use cleanprojects_core::core::cleaner::{self, CleanEvent, CleanOptions};
use cleanprojects_core::core::fsutil::{compact_path, ellipsize_middle, human_size};
use cleanprojects_core::core::state::{Config, Paths, State};
use cleanprojects_core::core::{Candidate, Category, Risk};
use cleanprojects_core::scan::{self, ScanContext, ScanEvent, ScanResult};

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
    let mut ctx = ScanContext::new(env.paths.home.clone(), config);
    if args.no_git {
        ctx.include_git = false;
    }
    ctx.include_global = !args.no_global;
    ctx.include_projects = !args.no_projects;
    ctx.include_tools = !args.no_tools;
    ctx
}

/// Run a scan with a live progress line on stderr.
pub fn run_scan(env: &Env, ctx: &ScanContext, quiet: bool) -> ScanResult {
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
    handle.join().expect("scan thread panicked")
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
    let result = run_scan(env, &ctx, cmd.json);
    let items = apply_filter(env, &result, &cmd.filter)?;
    if cmd.json {
        let out = serde_json::json!({
            "scanned_at": result.started_at,
            "duration_ms": result.duration_ms,
            "disk": result.disk,
            "roots": result.roots,
            "projects": result.projects,
            "repos": result.repos,
            "total_bytes": items.iter().map(|c| c.size).sum::<u64>(),
            "candidates": items,
        });
        out!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        print_table(env, &items);
        let total: u64 = items.iter().map(|c| c.size).sum();
        out!();
        out!(
            "{} candidates · {} recoverable · disk {} free of {} · {} projects, {} repos · {:.1}s",
            env.bold(&items.len().to_string()),
            env.orange(&human_size(total)),
            human_size(result.disk.free),
            human_size(result.disk.total),
            result.projects,
            result.repos,
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
        std::fs::write(path, markdown_report(env, &result, &items))?;
        eprintln!("report written to {}", path.display());
    }
    Ok(())
}

pub fn markdown_report(env: &Env, result: &ScanResult, items: &[&Candidate]) -> String {
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
    for c in items {
        s.push_str(&format!(
            "| {} | {} | {} | {} | `{}` |\n",
            human_size(c.size),
            c.risk.as_str(),
            c.category.as_str(),
            c.label.replace('|', "\\|"),
            compact_path(std::path::Path::new(&c.detail()), &env.paths.home)
        ));
    }
    let total: u64 = items.iter().map(|c| c.size).sum();
    s.push_str(&format!("\n**Total recoverable: {}**\n", human_size(total)));
    s
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
    let use_trash = env.config.clean.use_trash && !cmd.permanent;
    if !cmd.json {
        let refs: Vec<&Candidate> = items.iter().collect();
        print_table(env, &refs);
        out!();
        let mode = if cmd.dry_run {
            env.dim("dry run")
        } else if use_trash {
            env.green("move to trash")
        } else {
            env.red("PERMANENT delete")
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
    let opts = CleanOptions {
        dry_run: cmd.dry_run,
        use_trash,
        home: env.paths.home.clone(),
        protected: ctx.protected.clone(),
    };
    let json = cmd.json;
    let env_ref = &*env;
    let entry = cleaner::run(&items, &opts, |ev| {
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
    env.paths.append_log(&format!(
        "clean dry_run={} trash={} freed={} ok={} failed={}",
        entry.dry_run,
        entry.trashed,
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
    let disk = cleanprojects_core::core::disk::disk_for(&env.paths.home);
    out!("\n{}", env.bold("disk"));
    out!(
        "  {} · {} free of {} ({:.0}% used)",
        disk.mount,
        human_size(disk.free),
        human_size(disk.total),
        disk.used_percent()
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
