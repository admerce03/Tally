//! Writes `command.schema.json` and `output.schema.json` into the given directory.
//!
//! `cargo run -p tally-core --features schema --example export_schema -- <out-dir>`

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let dir = PathBuf::from(std::env::args_os().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("command.schema.json"),
        tally_core::json::command_schema_json(),
    )?;
    std::fs::write(
        dir.join("output.schema.json"),
        tally_core::json::output_schema_json(),
    )?;
    Ok(())
}
