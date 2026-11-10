# Tally

Tally tracks grades per class, per term, and cumulative GPA (standard 4.0 scale), with a target-grade solver, what-if scenarios and grade analysis (impact, leverage, trends). Every capability is a typed `Command` in `core`, exposed through the `grades` CLI and an MCP server so people and LLM agents can do the same things.

Workspace: `core` (pure domain + grade math), `store` (SQLite), `cli` (`grades`), `mcp` (`tally-mcp`).

## Build

```
cargo build --release
```

Binaries land in `target/release/` as `grades` and `tally-mcp` (`.exe` on Windows).

## Database

Both binaries share one SQLite file. Path precedence: `--db <PATH>`, then the `TALLY_DB` environment variable, then `<user data dir>/Tally/tally.db` (for example `%APPDATA%\Tally\tally.db` on Windows, `~/Library/Application Support/Tally/tally.db` on macOS). The default directory is created automatically; for an explicit `--db`/`TALLY_DB` path the directory must already exist. They can run against the same file at the same time.

## CLI quickstart

Every subcommand supports `--json`.

```
grades --help
grades term add "Fall 2026"
grades term list
grades --json term list
```

Run `grades --help` and `grades <subcommand> --help` for the full list (courses, categories, assignments, curves, target solver, scenarios, analysis, GPA).

## MCP server

`tally-mcp` speaks MCP over stdio. There is one tool per core `Command`, generated from the Command schema, plus `resolve` (name, code or UUID prefix to id). Tools take UUIDs; new entities need a caller-generated UUID v4, and every add returns the created id. Percentages are 0-100. Errors come back as tool errors. Start with `list_terms`, then `show_course`.

```
tally-mcp [--db <PATH>]     # or set TALLY_DB
```

### Generic MCP client

```json
{
  "mcpServers": {
    "tally": {
      "command": "C:\\path\\to\\tally-mcp.exe",
      "args": [],
      "env": { "TALLY_DB": "C:\\path\\to\\tally.db" }
    }
  }
}
```

Use your platform's path to the binary; `args` may instead be `["--db", "/path/to/tally.db"]`.

### Hermes Agent

Hermes Agent (NousResearch) reads MCP servers from `~/.hermes/config.yaml` under `mcp_servers`, with `command`, `args` and `env` for stdio servers. Source: <https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp>. Hermes passes only the variables listed in `env` (plus a safe baseline) to the server.

```yaml
mcp_servers:
  tally:
    command: "/path/to/tally-mcp"
    args: []
    env:
      TALLY_DB: "/path/to/tally.db"
```

After editing, run `/reload-mcp` in a session or restart Hermes.

## Development

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
