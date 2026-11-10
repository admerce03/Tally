//! `grades`: thin CLI adapter over tally-core Commands. Parses args, resolves refs via core,
//! executes, saves when the command mutates, renders. No grade math lives here.

mod args;
mod plan;
mod render;
mod sync;

use std::ffi::OsString;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use clap::error::ErrorKind;
use tally_core::CommandOutput;
use tally_store::{Store, now_ms, resolve_db_path};

use crate::args::Cli;

fn run(cli: Cli) -> Result<String> {
    let path = resolve_db_path(cli.db)?;
    let mut store =
        Store::open(&path).with_context(|| format!("could not open {}", path.display()))?;
    let command = match cli.command {
        crate::args::Top::Sync(a) => return sync::run(&mut store, a, cli.json),
        other => other,
    };
    let output = store.transact(now_ms(), |tx| -> Result<CommandOutput> {
        let planned = plan::plan(tx.gradebook(), command)?;
        let mut last = CommandOutput::Done;
        for cmd in planned.commands {
            last = tx.execute(cmd)?;
        }
        Ok(last)
    })?;
    if cli.json {
        Ok(serde_json::to_string(&output)?)
    } else {
        Ok(render::render(&output))
    }
}

/// `--json` anywhere before a `--` terminator (used when clap fails before it can tell us).
fn wants_json(raw: &[OsString]) -> bool {
    raw.iter()
        .skip(1)
        .take_while(|a| *a != "--")
        .any(|a| a == "--json")
}

fn main() -> ExitCode {
    let raw: Vec<OsString> = std::env::args_os().collect();
    let cli = match Cli::try_parse_from(&raw) {
        Ok(cli) => cli,
        Err(e) => {
            let informational = matches!(
                e.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayVersion
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            );
            if informational || !wants_json(&raw) {
                e.exit();
            }
            println!(
                "{}",
                serde_json::json!({ "error": e.to_string().trim().to_string() })
            );
            return ExitCode::from(2);
        }
    };
    let json = cli.json;
    match run(cli) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({ "error": format!("{e:#}") }));
            } else {
                eprintln!("error: {e:#}");
            }
            ExitCode::FAILURE
        }
    }
}
