//! `grades sync`: adapter action (not a core Command). Resolves the sync config, runs one
//! sync round on the same database the CLI uses and renders the report.

use anyhow::{Result, bail};
use tally_store::{Store, now_ms};
use tally_sync::{SyncClient, SyncConfig, SyncReport, config_path, sync_once};

use crate::args::SyncArgs;

pub fn run(store: &mut Store, args: SyncArgs, json: bool) -> Result<String> {
    let path = config_path()?;
    let file = SyncConfig::load_file(&path)?;
    let Some(cfg) = SyncConfig::resolve(args.server, args.token, args.token_file, file)? else {
        bail!(
            "no sync server configured: pass --server URL (and --token or --token-file), set \
             TALLY_SERVER and TALLY_TOKEN/TALLY_TOKEN_FILE, or save a config with `grades sync \
             --server URL --token-file PATH --save` ({})",
            path.display()
        );
    };
    if args.save {
        cfg.save_file(&path)?;
    }
    let report = sync_once(store, &SyncClient::new(&cfg), now_ms)?;
    if json {
        Ok(serde_json::to_string(&report)?)
    } else {
        Ok(render(&report, args.save.then_some(&path)))
    }
}

fn render(r: &SyncReport, saved: Option<&std::path::PathBuf>) -> String {
    let mut out = format!(
        "Synced: pushed {}, pulled {}, skipped {}",
        r.pushed,
        r.pulled,
        r.skipped.len()
    );
    for (id, why) in &r.skipped {
        out.push_str(&format!("\n  skipped {id}: {why}"));
    }
    if let Some(p) = saved {
        out.push_str(&format!("\nSaved sync config to {}", p.display()));
    }
    out
}
