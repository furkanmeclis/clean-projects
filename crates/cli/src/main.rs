#![forbid(unsafe_code)]

mod cli;
mod tui;

use anyhow::Result;
use clap::Parser;

use cli::commands::{self, Env};
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    if let Some(Command::Completions { shell }) = &cli.command {
        commands::cmd_completions(*shell);
        return Ok(());
    }
    let mut env = Env::load(&cli)?;
    match cli.command {
        None => tui::run(env, cli::ScanArgs::default()),
        Some(Command::Tui(args)) => tui::run(env, args),
        Some(Command::Scan(cmd)) => commands::cmd_scan(&env, &cmd),
        Some(Command::Clean(cmd)) => commands::cmd_clean(&mut env, &cmd),
        Some(Command::Exclude { targets }) => commands::cmd_exclude(&mut env, &targets),
        Some(Command::Include { targets, all }) => commands::cmd_include(&mut env, &targets, all),
        Some(Command::History { json, limit }) => commands::cmd_history(&env, json, limit),
        Some(Command::Config(cmd)) => commands::cmd_config(&mut env, &cmd),
        Some(Command::Doctor) => commands::cmd_doctor(&env),
        Some(Command::Undo { run, id }) => commands::cmd_undo(&mut env, run, id),
        Some(Command::Recovery { purge, empty, json }) => {
            commands::cmd_recovery(&env, purge, empty, json)
        }
        Some(Command::Volumes { json }) => commands::cmd_volumes(&env, json),
        Some(Command::Explore { path, top, json }) => commands::cmd_explore(&env, path, top, json),
        Some(Command::Large {
            path,
            min_size,
            top,
            json,
        }) => commands::cmd_large(&env, path, min_size, top, json),
        Some(Command::Skill { install }) => commands::cmd_skill(&env, install),
        Some(Command::Completions { .. }) => unreachable!(),
    }
}
