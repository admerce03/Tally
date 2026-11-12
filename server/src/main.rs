//! `tally-server`: sync + remote MCP hub for the home server.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use tally_server::{AppState, router};
use tally_store::{Store, resolve_db_path};

#[derive(Parser)]
#[command(version, about = "Tally sync server and remote MCP endpoint")]
struct Args {
    /// SQLite database (default: <data dir>/Tally/tally.db)
    #[arg(long, env = "TALLY_DB")]
    db: Option<PathBuf>,
    /// Address to listen on; bind to the Tailscale address explicitly to serve other devices
    #[arg(long, env = "TALLY_BIND", default_value = "127.0.0.1:7878")]
    bind: SocketAddr,
    /// File holding the bearer token (else the TALLY_TOKEN environment variable)
    #[arg(long, env = "TALLY_TOKEN_FILE")]
    token_file: Option<PathBuf>,
    /// Extra allowed CORS origin (repeatable)
    #[arg(long = "cors-origin")]
    cors_origin: Vec<String>,
    /// Extra allowed Host header value for /mcp (repeatable), e.g. a Tailscale MagicDNS name
    #[arg(long = "allowed-host")]
    allowed_host: Vec<String>,
}

fn read_token(args: &Args) -> Result<String> {
    let raw = match &args.token_file {
        Some(p) => std::fs::read_to_string(p)
            .with_context(|| format!("could not read token file {}", p.display()))?,
        None => std::env::var("TALLY_TOKEN").unwrap_or_default(),
    };
    let token = raw.trim().to_string();
    if token.is_empty() {
        bail!("no token: set TALLY_TOKEN or pass --token-file (refusing to start without auth)");
    }
    Ok(token)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let token = read_token(&args)?;
    let path = resolve_db_path(args.db.clone())?;
    let mut store =
        Store::open(&path).with_context(|| format!("could not open {}", path.display()))?;
    let registered = store.server_log_register_local()?;

    let mut hosts = args.allowed_host.clone();
    hosts.push(args.bind.ip().to_string());
    hosts.push(args.bind.to_string());
    let state = AppState::new(store, token, args.cors_origin.clone(), hosts);

    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("could not bind {}", args.bind))?;
    eprintln!(
        "tally-server {}: serving {} on http://{} ({registered} local ops registered)",
        env!("CARGO_PKG_VERSION"),
        path.display(),
        args.bind
    );
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("server error")?;
    Ok(())
}
