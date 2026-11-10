//! `tally-mcp`: Tally MCP server over stdio. stdout carries the protocol; logs go to stderr.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use rmcp::ServiceExt;
use tally_mcp::TallyServer;
use tally_store::{Store, resolve_db_path};

const USAGE: &str = "\
tally-mcp: Tally MCP server (stdio)

USAGE:
    tally-mcp [--db <PATH>]

OPTIONS:
    --db <PATH>    SQLite database (env TALLY_DB; default: <data dir>/Tally/tally.db)
    -h, --help     Print this help
    -V, --version  Print the version";

/// `Ok(None)` means an informational flag was handled and the process should exit.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Option<Option<PathBuf>>> {
    let mut db = None;
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                eprintln!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                eprintln!("tally-mcp {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--db" => {
                db = Some(PathBuf::from(
                    args.next().context("--db requires a path argument")?,
                ));
            }
            other => match other.strip_prefix("--db=") {
                Some(p) => db = Some(PathBuf::from(p)),
                None => bail!("unexpected argument '{other}'\n\n{USAGE}"),
            },
        }
    }
    Ok(Some(db))
}

async fn run() -> Result<()> {
    let Some(db) = parse_args(std::env::args().skip(1))? else {
        return Ok(());
    };
    let path = resolve_db_path(db)?;
    let store = Store::open(&path).with_context(|| format!("could not open {}", path.display()))?;
    eprintln!("tally-mcp: serving {} on stdio", path.display());
    let server = TallyServer::new(store)?;
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .context("MCP initialization failed")?;
    running.waiting().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tally-mcp: error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
