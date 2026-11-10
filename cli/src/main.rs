//! `grades`: thin CLI adapter over tally-core Commands. Parses args, resolves refs via core,
//! executes, saves when the command mutates, renders. No grade math lives here.

mod args;
mod plan;
mod render;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use clap::error::ErrorKind;
use tally_core::CommandOutput;
use tally_store::Store;

use crate::args::Cli;

/// The explicit `--db`/`TALLY_DB` path is used as given (the user owns that location); only
/// the default data directory is created on demand.
fn db_path(arg: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(p);
    }
    let dir = dirs::data_dir()
        .context("could not determine the data directory; pass --db <PATH>")?
        .join("Tally");
    std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    Ok(dir.join("tally.db"))
}

fn run(cli: Cli) -> Result<String> {
    let path = db_path(cli.db)?;
    let mut store =
        Store::open(&path).with_context(|| format!("could not open {}", path.display()))?;
    let command = cli.command;
    let output = store.transact(|gb| -> Result<(CommandOutput, bool)> {
        let planned = plan::plan(gb, command)?;
        let mutation = planned.commands.iter().any(|c| c.is_mutation());
        let mut last = CommandOutput::Done;
        for cmd in planned.commands {
            last = tally_core::execute(gb, cmd)?;
        }
        Ok((last, mutation))
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
