# Tally

Tally tracks grades per class, per term, and cumulative GPA (standard 4.0 scale), with a target-grade solver, what-if scenarios and grade analysis (impact, leverage, trends). Every capability is a typed `Command` in `core`, exposed through the `grades` CLI and an MCP server so people and LLM agents can do the same things.

Workspace: `core` (pure domain + grade math), `store` (SQLite), `cli` (`grades`), `mcp` (`tally-mcp`), `sync` (client), `server` (`tally-server`), `app` (desktop).

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

## Sync & home server

Tally syncs across devices through `tally-server`, a small Axum server you run on a home machine. What syncs is the **operation log**, not the database: every mutation (CLI, app, MCP) appends an op `{op_id, device_id, hlc, command}`. Each device and the server rebuild the same state by replaying all known ops sorted by `(hlc, device_id, op_id)`. Updates only touch the fields they set, so concurrent edits resolve last-write-wins per field; a later delete beats an earlier update. Pushing is idempotent, so retrying is always safe. The first sync of an existing gradebook uploads it as one `ImportGradebook` op.

The server is also a device: `/api/execute` and `/mcp` run Commands through the same op-appending path, so agents using the remote MCP endpoint create data that your devices receive on their next sync. The local `tally-mcp` (stdio) is unchanged: its edits go through the same op log and sync with the next `grades sync` or app sync.

### Run the server

```
cargo build --release -p tally-server      # target/release/tally-server
```

Create a token (any 32+ random bytes) and keep it private:

```
openssl rand -hex 32 > ~/.config/tally-token && chmod 600 ~/.config/tally-token
```

```
tally-server --token-file ~/.config/tally-token --bind 100.x.y.z:7878 --allowed-host my-server.tailnet-name.ts.net
```

- Token: `TALLY_TOKEN` (the value) or `--token-file` / `TALLY_TOKEN_FILE`. The server refuses to start without one. Every route except `/health` needs `Authorization: Bearer <token>`.
- `--bind` (`TALLY_BIND`) defaults to `127.0.0.1:7878`. Bind to the machine's Tailscale address (`tailscale ip -4`). Never bind `0.0.0.0` or expose the port to the public internet: the token is the only protection and traffic is plain HTTP (Tailscale encrypts it end to end).
- `--allowed-host` (repeatable) adds a Host header value accepted by `/mcp`, for example the MagicDNS name clients connect with. `--cors-origin` (repeatable) adds a CORS origin; the app's origins are built in.
- `--db` / `TALLY_DB` selects the server's SQLite file (default: the user data dir).

#### systemd (Linux)

`/etc/systemd/system/tally-server.service`:

```ini
[Unit]
Description=Tally sync server
After=network-online.target tailscaled.service
Wants=network-online.target

[Service]
User=tally
ExecStart=/usr/local/bin/tally-server --bind 100.x.y.z:7878 --allowed-host my-server.tailnet-name.ts.net
Environment=TALLY_DB=/var/lib/tally/tally.db
Environment=TALLY_TOKEN_FILE=/etc/tally/token
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Then `sudo systemctl enable --now tally-server`.

#### launchd (macOS)

`~/Library/LaunchAgents/com.tally.server.plist`, loaded with `launchctl load ~/Library/LaunchAgents/com.tally.server.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.tally.server</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/tally-server</string>
    <string>--bind</string><string>100.x.y.z:7878</string>
    <string>--token-file</string><string>/Users/you/.config/tally-token</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict>
</plist>
```

### Configure clients

CLI (the same database as every other `grades` command: `--db` / `TALLY_DB`):

```
grades sync --server http://100.x.y.z:7878 --token-file ~/.config/tally-token --save
grades sync            # later runs use the saved config
grades --json sync     # prints the sync report
```

Settings resolve per field as flag, then environment (`TALLY_SERVER`, `TALLY_TOKEN`, `TALLY_TOKEN_FILE`), then the config file. `--save` writes `<user config dir>/Tally/sync.json` (override the path with `TALLY_SYNC_CONFIG`); **the token is stored there in plain text**, so keep that folder private or use `--token-file`/env instead.

Desktop app: the sidebar footer has **Sync now**, the last sync result, and **Settings** for the server URL and token (the same config file as the CLI). When configured, the app syncs at startup and every 60 seconds in the background and refreshes the views when data arrives.

### Hermes Agent over HTTP

The server speaks MCP over streamable HTTP at `/mcp`, with the same tools as `tally-mcp`. In `~/.hermes/config.yaml` use `url` and `headers` instead of `command`:

```yaml
mcp_servers:
  tally:
    url: "http://my-server.tailnet-name.ts.net:7878/mcp"
    headers:
      Authorization: "Bearer <your token>"
```

The hostname must be listed with `--allowed-host` (or use the bound IP). Run `/reload-mcp` or restart Hermes after editing.

### HTTP API

| Route | Purpose |
| --- | --- |
| `GET /health` | Liveness and version; no auth |
| `POST /api/execute` | Run one core `Command` (JSON body), returns the `CommandOutput` |
| `POST /api/sync/push` | Upload ops `{ops: [...]}`; idempotent by `op_id` |
| `GET /api/sync/pull?since=<seq>&limit=<n>` | Ops after the cursor, with `next` and `more` |
| `/mcp` | MCP over streamable HTTP |

## Development

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Desktop app (Tauri 2)

`app/src-tauri` (crate `tally-app`) is the Rust backend: one Tauri command, `execute`, that runs a core `Command` (JSON) through `Store::transact`, the same path as the MCP server, and returns the `CommandOutput`. `app/ui` is the Svelte 5 + Vite + TypeScript frontend. The database path is resolved like the CLI (`TALLY_DB`, else the user data dir).

Prerequisites: Rust stable, Node 20+ with npm. Windows: WebView2 (preinstalled on Windows 11) and the MSVC build tools. macOS: Xcode Command Line Tools (`xcode-select --install`).

```
cd app/ui
npm install
npm run tauri dev       # runs Vite + the app with hot reload
npm run tauri build     # release bundles (MSI/NSIS on Windows, .app/.dmg on macOS)
```

Other scripts in `app/ui`: `npm run check` (svelte-check), `npm run build` (frontend only), `npm run gen:types`.

### TypeScript types

`Command` and `CommandOutput` types are generated from the core JSON schema; never edit `app/ui/src/lib/generated/`. After changing core commands or outputs run `npm run gen:types` in `app/ui` and commit the result. A Rust test (`cargo test -p tally-app`) fails if the committed schemas are stale. The schema export is also available directly: `cargo run -p tally-core --features schema --example export_schema -- <out-dir>`.

### Fresh clone

`cargo fmt/clippy/test --workspace` work without running npm: `app/src-tauri/build.rs` creates a placeholder `app/ui/dist/index.html` if the directory is missing (Tauri needs `frontendDist` to exist at compile time). `npm run build` replaces it with the real UI; `dist/` is git-ignored.
