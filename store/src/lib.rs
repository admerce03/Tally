//! SQLite persistence: the materialized `Gradebook` (one JSON row) plus the sync op log.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use tally_core::sync::{OpKey, apply_sorted, recv, replay, tick};
use tally_core::{Command, CommandOutput, CoreError, DeviceId, Gradebook, Hlc, Op, OpId, execute};
use thiserror::Error;
use uuid::Uuid;

/// Bumped when the stored JSON layout changes incompatibly.
pub const SCHEMA_VERSION: i64 = 3;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored gradebook is not valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database schema version {found} is newer than supported ({SCHEMA_VERSION})")]
    UnsupportedSchema { found: i64 },
    #[error("invalid stored op or clock value: {0}")]
    InvalidOp(String),
    #[error("could not determine the data directory; pass --db <PATH> or set TALLY_DB")]
    NoDataDir,
    #[error("could not create {}: {source}", path.display())]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Environment variable that overrides the default database location.
pub const DB_ENV: &str = "TALLY_DB";

/// `<data dir>/Tally/tally.db` (for example under `%APPDATA%` on Windows). Pure: creates nothing.
pub fn default_db_path() -> Result<PathBuf, StoreError> {
    Ok(dirs::data_dir()
        .ok_or(StoreError::NoDataDir)?
        .join("Tally")
        .join("tally.db"))
}

/// Pick the database path: `explicit` (e.g. `--db`), else `TALLY_DB` (when non-empty), else the
/// default. An explicit or env path is used as given (the user owns that location); only the
/// parent directory of the default path is created on demand.
pub fn resolve_db_path(explicit: Option<PathBuf>) -> Result<PathBuf, StoreError> {
    resolve_db_path_with(explicit, std::env::var_os(DB_ENV).map(PathBuf::from))
}

fn resolve_db_path_with(
    explicit: Option<PathBuf>,
    env: Option<PathBuf>,
) -> Result<PathBuf, StoreError> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    if let Some(p) = env.filter(|p| !p.as_os_str().is_empty()) {
        return Ok(p);
    }
    let path = default_db_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| StoreError::CreateDir {
            path: dir.to_path_buf(),
            source,
        })?;
    }
    Ok(path)
}

pub struct Store {
    conn: Connection,
}

/// How long a writer waits for another process's lock before giving up.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Current wall-clock time in milliseconds since the Unix epoch. Adapters pass this to
/// [`Store::transact`] and the sync API; core never reads the clock itself.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Bring data stored at schema version `from` (older than `SCHEMA_VERSION`) up to date.
///
/// v1 -> v2: letter cutoffs and the GPA scale changed from `[letter, value]` pairs to
/// `{letter, min}` / `{letter, points}` objects.
/// v2 -> v3: the gradebook JSON is unchanged; the op log and `meta` tables were added (see
/// `Store::init`, which also flags the existing data for bootstrap).
fn migrate(from: i64, data: &str) -> Result<Gradebook, StoreError> {
    debug_assert!(from < SCHEMA_VERSION);
    let mut value: serde_json::Value = serde_json::from_str(data)?;
    if from < 2 {
        migrate_v1_to_v2(&mut value);
    }
    Ok(serde_json::from_value(value)?)
}

/// Rewrite `[letter, n]` pair arrays in `list` into `{letter, <key>: n}` objects.
fn pairs_to_objects(list: Option<&mut serde_json::Value>, key: &str) {
    let Some(items) = list.and_then(|v| v.as_array_mut()) else {
        return;
    };
    for item in items {
        let converted = match item.as_array().map(Vec::as_slice) {
            Some([letter, n]) => Some(serde_json::json!({ "letter": letter, key: n })),
            _ => None,
        };
        if let Some(obj) = converted {
            *item = obj;
        }
    }
}

fn migrate_v1_to_v2(root: &mut serde_json::Value) {
    pairs_to_objects(root.pointer_mut("/scale/points"), "points");
    let Some(terms) = root.get_mut("terms").and_then(|t| t.as_array_mut()) else {
        return;
    };
    for course in terms
        .iter_mut()
        .filter_map(|t| t.get_mut("courses").and_then(|c| c.as_array_mut()))
        .flatten()
    {
        pairs_to_objects(course.pointer_mut("/cutoffs/entries"), "min");
    }
}

fn load_from(conn: &Connection) -> Result<Gradebook, StoreError> {
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT schema_version, data FROM gradebook WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        None => Ok(Gradebook::default()),
        Some((found, data)) => match found.cmp(&SCHEMA_VERSION) {
            Ordering::Equal => Ok(serde_json::from_str(&data)?),
            Ordering::Less => migrate(found, &data),
            Ordering::Greater => Err(StoreError::UnsupportedSchema { found }),
        },
    }
}

fn save_to(conn: &Connection, gb: &Gradebook) -> Result<(), StoreError> {
    let data = serde_json::to_string(gb)?;
    conn.execute(
        "INSERT INTO gradebook (id, schema_version, data) VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET schema_version = excluded.schema_version,
                                       data = excluded.data",
        params![SCHEMA_VERSION, data],
    )?;
    Ok(())
}

// ---- meta + op log ----

struct Meta {
    device_id: DeviceId,
    cursor: i64,
    last_hlc: Hlc,
    needs_bootstrap: bool,
}

fn invalid(what: &str, e: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidOp(format!("{what}: {e}"))
}

fn hlc_from_columns(wall: i64, counter: i64) -> Result<Hlc, StoreError> {
    Ok(Hlc {
        wall_ms: u64::try_from(wall).map_err(|e| invalid("hlc", e))?,
        counter: u32::try_from(counter).map_err(|e| invalid("hlc", e))?,
    })
}

fn read_meta(conn: &Connection) -> Result<Meta, StoreError> {
    let (device, cursor, wall, counter, boot): (String, i64, i64, i64, i64) = conn.query_row(
        "SELECT device_id, sync_cursor, last_hlc_wall, last_hlc_counter, needs_bootstrap
         FROM meta WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    Ok(Meta {
        device_id: DeviceId(Uuid::parse_str(&device).map_err(|e| invalid("device id", e))?),
        cursor,
        last_hlc: hlc_from_columns(wall, counter)?,
        needs_bootstrap: boot != 0,
    })
}

fn write_hlc(conn: &Connection, hlc: Hlc) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE meta SET last_hlc_wall = ?1, last_hlc_counter = ?2 WHERE id = 1",
        params![
            i64::try_from(hlc.wall_ms).map_err(|e| invalid("hlc", e))?,
            i64::from(hlc.counter)
        ],
    )?;
    Ok(())
}

fn insert_op(conn: &Connection, op: &Op, pushed: bool) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO ops (op_id, device_id, wall_ms, counter, command_json, pushed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            op.op_id.0.to_string(),
            op.device_id.0.to_string(),
            i64::try_from(op.hlc.wall_ms).map_err(|e| invalid("hlc", e))?,
            i64::from(op.hlc.counter),
            serde_json::to_string(&op.command)?,
            i64::from(pushed),
        ],
    )?;
    Ok(())
}

fn op_from_row(r: &rusqlite::Row<'_>) -> Result<Op, StoreError> {
    let id: String = r.get(0)?;
    let dev: String = r.get(1)?;
    let wall: i64 = r.get(2)?;
    let counter: i64 = r.get(3)?;
    let json: String = r.get(4)?;
    Ok(Op {
        op_id: OpId(Uuid::parse_str(&id).map_err(|e| invalid("op id", e))?),
        device_id: DeviceId(Uuid::parse_str(&dev).map_err(|e| invalid("device id", e))?),
        hlc: hlc_from_columns(wall, counter)?,
        command: serde_json::from_str(&json)?,
    })
}

const OP_COLUMNS: &str = "op_id, device_id, wall_ms, counter, command_json";

fn query_ops(conn: &Connection, where_sql: &str) -> Result<Vec<Op>, StoreError> {
    let sql = format!(
        "SELECT {OP_COLUMNS} FROM ops {where_sql} ORDER BY wall_ms, counter, device_id, op_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(op_from_row(row)?);
    }
    Ok(out)
}

fn newest_op_key(conn: &Connection) -> Result<Option<OpKey>, StoreError> {
    let sql = format!(
        "SELECT {OP_COLUMNS} FROM ops
         ORDER BY wall_ms DESC, counter DESC, device_id DESC, op_id DESC LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    match rows.next()? {
        Some(row) => Ok(Some(op_from_row(row)?.key())),
        None => Ok(None),
    }
}

fn op_exists(conn: &Connection, id: OpId) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ops WHERE op_id = ?1)",
        params![id.0.to_string()],
        |r| r.get::<_, i64>(0),
    )? != 0)
}

fn log_is_empty(conn: &Connection) -> Result<bool, StoreError> {
    Ok(
        conn.query_row("SELECT NOT EXISTS(SELECT 1 FROM ops)", [], |r| {
            r.get::<_, i64>(0)
        })? != 0,
    )
}

fn clear_bootstrap_flag(conn: &Connection) -> Result<(), StoreError> {
    conn.execute("UPDATE meta SET needs_bootstrap = 0 WHERE id = 1", [])?;
    Ok(())
}

/// Append a local op for `command`, advancing `hlc`.
fn append_local(
    conn: &Connection,
    device_id: DeviceId,
    hlc: &mut Hlc,
    now_ms: u64,
    command: Command,
) -> Result<(), StoreError> {
    *hlc = tick(*hlc, now_ms);
    let op = Op {
        op_id: OpId(Uuid::new_v4()),
        device_id,
        hlc: *hlc,
        command,
    };
    insert_op(conn, &op, false)
}

/// If the op log is empty and `gb` is not, append one `ImportGradebook` op of `gb`.
fn bootstrap_in(
    conn: &Connection,
    gb: &Gradebook,
    device_id: DeviceId,
    hlc: &mut Hlc,
    now_ms: u64,
) -> Result<bool, StoreError> {
    if !log_is_empty(conn)? || *gb == Gradebook::default() {
        return Ok(false);
    }
    append_local(
        conn,
        device_id,
        hlc,
        now_ms,
        Command::ImportGradebook {
            gradebook: gb.clone(),
        },
    )?;
    Ok(true)
}

/// Handle given to a [`Store::transact`] closure: read the gradebook and run Commands.
/// Mutations that succeed are recorded and become ops when the transaction commits.
pub struct Tx<'a> {
    gb: &'a mut Gradebook,
    executed: Vec<Command>,
}

impl Tx<'_> {
    /// The current state (including mutations already executed in this transaction).
    pub fn gradebook(&self) -> &Gradebook {
        self.gb
    }

    /// Execute a Command. A failed mutation leaves the gradebook untouched.
    pub fn execute(&mut self, command: Command) -> Result<CommandOutput, CoreError> {
        if !command.is_mutation() {
            return execute(self.gb, command);
        }
        let mut next = self.gb.clone();
        let out = execute(&mut next, command.clone())?;
        *self.gb = next;
        self.executed.push(command);
        Ok(out)
    }
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self, StoreError> {
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS gradebook (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                schema_version INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                device_id TEXT NOT NULL,
                sync_cursor INTEGER NOT NULL DEFAULT 0,
                last_hlc_wall INTEGER NOT NULL DEFAULT 0,
                last_hlc_counter INTEGER NOT NULL DEFAULT 0,
                needs_bootstrap INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS ops (
                op_id TEXT PRIMARY KEY,
                device_id TEXT NOT NULL,
                wall_ms INTEGER NOT NULL,
                counter INTEGER NOT NULL,
                command_json TEXT NOT NULL,
                pushed INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS ops_order
                ON ops (wall_ms, counter, device_id, op_id);
            CREATE TABLE IF NOT EXISTS server_ops (
                server_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                op_id TEXT NOT NULL UNIQUE,
                op_json TEXT NOT NULL
            );",
        )?;
        let version: Option<i64> = tx
            .query_row(
                "SELECT schema_version FROM gradebook WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let has_meta = tx.query_row("SELECT EXISTS(SELECT 1 FROM meta)", [], |r| {
            r.get::<_, i64>(0)
        })? != 0;
        if !has_meta {
            // Data that predates the op log must be bootstrapped into it.
            tx.execute(
                "INSERT INTO meta (id, device_id, needs_bootstrap) VALUES (1, ?1, ?2)",
                params![Uuid::new_v4().to_string(), i64::from(version.is_some())],
            )?;
        }
        if let Some(v) = version
            && v < SCHEMA_VERSION
        {
            let gb = load_from(&tx)?;
            save_to(&tx, &gb)?;
        }
        tx.commit()?;
        Ok(Self { conn })
    }

    /// The stored gradebook, or an empty default if nothing was saved yet.
    pub fn load(&self) -> Result<Gradebook, StoreError> {
        load_from(&self.conn)
    }

    /// Replace the stored gradebook, atomically. Does not touch the op log; prefer
    /// [`Store::transact`] for anything that should sync.
    pub fn save(&mut self, gb: &Gradebook) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        save_to(&tx, gb)?;
        tx.commit()?;
        Ok(())
    }

    /// Load, run `f`, and persist, all inside one `BEGIN IMMEDIATE` transaction so concurrent
    /// processes cannot lose each other's updates. `f` runs Commands through [`Tx::execute`];
    /// if any mutation succeeded, the HLC is ticked at `now_ms`, one op per mutation is
    /// appended to the op log and the materialized gradebook is saved. If `f` or the save
    /// fails, nothing is written. A gradebook that predates the op log is first bootstrapped
    /// into it.
    pub fn transact<T, E>(
        &mut self,
        now_ms: u64,
        f: impl FnOnce(&mut Tx<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::from)?;
        let mut gb = load_from(&tx)?;
        let before = (gb != Gradebook::default()).then(|| gb.clone());
        let mut handle = Tx {
            gb: &mut gb,
            executed: Vec::new(),
        };
        let value = f(&mut handle)?;
        let executed = handle.executed;
        if !executed.is_empty() {
            let meta = read_meta(&tx)?;
            let mut hlc = meta.last_hlc;
            if let Some(before) = before {
                bootstrap_in(&tx, &before, meta.device_id, &mut hlc, now_ms)?;
            }
            for command in executed {
                append_local(&tx, meta.device_id, &mut hlc, now_ms, command)?;
            }
            write_hlc(&tx, hlc)?;
            clear_bootstrap_flag(&tx)?;
            save_to(&tx, &gb)?;
        }
        tx.commit().map_err(StoreError::from)?;
        Ok(value)
    }

    /// This device's id (generated on first open).
    pub fn device_id(&self) -> Result<DeviceId, StoreError> {
        Ok(read_meta(&self.conn)?.device_id)
    }

    /// Last server sequence number pulled.
    pub fn cursor(&self) -> Result<i64, StoreError> {
        Ok(read_meta(&self.conn)?.cursor)
    }

    pub fn set_cursor(&mut self, cursor: i64) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE meta SET sync_cursor = ?1 WHERE id = 1",
            params![cursor],
        )?;
        Ok(())
    }

    /// True when the store was migrated from before the op log and not yet bootstrapped.
    pub fn needs_bootstrap(&self) -> Result<bool, StoreError> {
        Ok(read_meta(&self.conn)?.needs_bootstrap)
    }

    /// All ops in log order `(hlc, device_id, op_id)`.
    pub fn ops(&self) -> Result<Vec<Op>, StoreError> {
        query_ops(&self.conn, "")
    }

    /// Local ops not yet acknowledged by the server, in log order.
    pub fn unpushed_ops(&self) -> Result<Vec<Op>, StoreError> {
        query_ops(&self.conn, "WHERE pushed = 0")
    }

    pub fn mark_pushed(&mut self, ids: &[OpId]) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE ops SET pushed = 1 WHERE op_id = ?1",
                params![id.0.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// If the op log is empty and the gradebook is not, append one `ImportGradebook` op of the
    /// current state. Returns whether an op was appended.
    pub fn ensure_bootstrap(&mut self, now_ms: u64) -> Result<bool, StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let gb = load_from(&tx)?;
        let meta = read_meta(&tx)?;
        let mut hlc = meta.last_hlc;
        let added = bootstrap_in(&tx, &gb, meta.device_id, &mut hlc, now_ms)?;
        if added {
            write_hlc(&tx, hlc)?;
        }
        clear_bootstrap_flag(&tx)?;
        tx.commit()?;
        Ok(added)
    }

    /// Merge ops received from the server. Ops already in the log are ignored (and marked as
    /// pushed); new ones are stored as pushed, the HLC advances via `recv`, and the gradebook
    /// is updated: incrementally if every new op sorts after the newest local op, otherwise by
    /// full replay of the log. Returns the new ops that could not be applied, with reasons.
    pub fn apply_remote_ops(
        &mut self,
        ops: Vec<Op>,
        now_ms: u64,
    ) -> Result<Vec<(OpId, String)>, StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let meta = read_meta(&tx)?;
        let mut hlc = meta.last_hlc;
        let mut gb = load_from(&tx)?;
        // Local state that predates the log must be in it before remote ops are merged.
        bootstrap_in(&tx, &gb, meta.device_id, &mut hlc, now_ms)?;
        let newest = newest_op_key(&tx)?;

        let mut fresh: Vec<Op> = Vec::new();
        for op in ops {
            if op_exists(&tx, op.op_id)? {
                tx.execute(
                    "UPDATE ops SET pushed = 1 WHERE op_id = ?1",
                    params![op.op_id.0.to_string()],
                )?;
                continue;
            }
            insert_op(&tx, &op, true)?;
            hlc = recv(hlc, op.hlc, now_ms);
            fresh.push(op);
        }
        fresh.sort_by_key(Op::key);

        let mut skipped = Vec::new();
        if let Some(first) = fresh.first() {
            if newest.is_none_or(|n| first.key() > n) {
                skipped = apply_sorted(&mut gb, &fresh);
            } else {
                let result = replay(&query_ops(&tx, "")?);
                gb = result.gradebook;
                skipped = result
                    .skipped
                    .into_iter()
                    .filter(|(id, _)| fresh.iter().any(|o| o.op_id == *id))
                    .collect();
            }
            save_to(&tx, &gb)?;
        }
        write_hlc(&tx, hlc)?;
        clear_bootstrap_flag(&tx)?;
        tx.commit()?;
        Ok(skipped)
    }
}

/// One page of the server op registry, see [`Store::server_log_pull`].
#[derive(Debug, Clone, PartialEq)]
pub struct ServerLogPage {
    /// `(server_seq, op)` with `server_seq > since`, ascending.
    pub ops: Vec<(i64, Op)>,
    /// Cursor to pass as `since` next time (the last returned seq, or `since` if empty).
    pub next: i64,
    /// True when more ops exist after `next`.
    pub more: bool,
}

/// Server-side op registry (the `server_ops` table): every op the server knows, with a
/// monotonic `server_seq` that devices use as a pull cursor. Used only by the sync server.
impl Store {
    /// Register ops idempotently by `op_id`. Returns, per input op, whether it was new.
    pub fn server_log_append(&mut self, ops: &[Op]) -> Result<Vec<bool>, StoreError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = server_log_append_in(&tx, ops)?;
        tx.commit()?;
        Ok(out)
    }

    /// Highest assigned `server_seq` (0 when empty).
    pub fn server_log_head(&self) -> Result<i64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(server_seq), 0) FROM server_ops",
            [],
            |r| r.get(0),
        )?)
    }

    /// Ops after cursor `since`, at most `limit` (at least 1).
    pub fn server_log_pull(&self, since: i64, limit: usize) -> Result<ServerLogPage, StoreError> {
        let limit = limit.max(1);
        let fetch = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
        let mut stmt = self.conn.prepare(
            "SELECT server_seq, op_json FROM server_ops WHERE server_seq > ?1
             ORDER BY server_seq LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![since, fetch])?;
        let mut ops = Vec::new();
        while let Some(row) = rows.next()? {
            let seq: i64 = row.get(0)?;
            let json: String = row.get(1)?;
            ops.push((seq, serde_json::from_str::<Op>(&json)?));
        }
        let more = ops.len() > limit;
        ops.truncate(limit);
        let next = ops.last().map_or(since, |(seq, _)| *seq);
        Ok(ServerLogPage { ops, next, more })
    }

    /// Register this store's own locally created ops (from `transact`) in the registry so
    /// devices can pull them, and mark them as pushed. Returns how many were registered.
    pub fn server_log_register_local(&mut self) -> Result<usize, StoreError> {
        let local = self.unpushed_ops()?;
        if local.is_empty() {
            return Ok(0);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        server_log_append_in(&tx, &local)?;
        for op in &local {
            tx.execute(
                "UPDATE ops SET pushed = 1 WHERE op_id = ?1",
                params![op.op_id.0.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(local.len())
    }
}

fn server_log_append_in(conn: &Connection, ops: &[Op]) -> Result<Vec<bool>, StoreError> {
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let n = conn.execute(
            "INSERT OR IGNORE INTO server_ops (op_id, op_json) VALUES (?1, ?2)",
            params![op.op_id.0.to_string(), serde_json::to_string(op)?],
        )?;
        out.push(n > 0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tally_core::{Command, CommandOutput, CoreError, TermId, execute};

    #[test]
    fn db_path_precedence_explicit_then_env() {
        let explicit = PathBuf::from("explicit.db");
        let env = PathBuf::from("env.db");
        assert_eq!(
            resolve_db_path_with(Some(explicit.clone()), Some(env.clone())).unwrap(),
            explicit
        );
        assert_eq!(resolve_db_path_with(None, Some(env.clone())).unwrap(), env);
    }

    #[test]
    fn default_db_path_ends_with_tally_dir_and_file() {
        let p = default_db_path().unwrap();
        assert!(p.ends_with(Path::new("Tally").join("tally.db")));
    }

    fn sample(name: &str) -> Gradebook {
        let mut gb = Gradebook::default();
        let id = uuid_from(name);
        let _ = execute(
            &mut gb,
            Command::AddTerm {
                id,
                name: name.into(),
            },
        );
        gb
    }

    fn uuid_from(name: &str) -> TermId {
        let n = name
            .bytes()
            .fold(1u128, |a, b| a.wrapping_mul(31).wrapping_add(u128::from(b)));
        serde_json::from_str(&format!("\"{:032x}\"", n)).unwrap()
    }

    #[test]
    fn empty_db_loads_default() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.load().unwrap(), Gradebook::default());
    }

    #[test]
    fn in_memory_round_trip() {
        let mut store = Store::open_in_memory().unwrap();
        let gb = sample("Fall");
        store.save(&gb).unwrap();
        assert_eq!(store.load().unwrap(), gb);
    }

    #[test]
    fn save_overwrites() {
        let mut store = Store::open_in_memory().unwrap();
        store.save(&sample("Fall")).unwrap();
        let second = sample("Spring");
        store.save(&second).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded, second);
        assert_eq!(loaded.terms.len(), 1);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn
            .execute(
                "INSERT INTO gradebook (id, schema_version, data) VALUES (1, ?1, '{}')",
                params![SCHEMA_VERSION + 1],
            )
            .unwrap();
        assert!(matches!(
            store.load(),
            Err(StoreError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn file_round_trip() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "tally-store-test-{}-{nanos}.db",
            std::process::id()
        ));
        let gb = sample("Fall");
        {
            let mut store = Store::open(&path).unwrap();
            store.save(&gb).unwrap();
        }
        let loaded = Store::open(&path).unwrap().load().unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(loaded, gb);
    }

    #[derive(Debug, PartialEq)]
    enum TestErr {
        Store,
        Core,
        Boom,
    }

    impl From<StoreError> for TestErr {
        fn from(_: StoreError) -> Self {
            TestErr::Store
        }
    }

    impl From<CoreError> for TestErr {
        fn from(_: CoreError) -> Self {
            TestErr::Core
        }
    }

    fn add_term_cmd(name: &str) -> Command {
        Command::AddTerm {
            id: uuid_from(name),
            name: name.into(),
        }
    }

    fn run(store: &mut Store, now: u64, cmd: Command) -> Result<CommandOutput, TestErr> {
        store.transact(now, |tx| Ok(tx.execute(cmd)?))
    }

    fn term_names(store: &Store) -> Vec<String> {
        let mut v: Vec<String> = store
            .load()
            .unwrap()
            .terms
            .iter()
            .map(|t| t.name.clone())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn transact_saves_mutations_only() {
        let mut store = Store::open_in_memory().unwrap();
        run(&mut store, 1000, add_term_cmd("Fall")).unwrap();
        assert_eq!(store.load().unwrap().terms.len(), 1);
        assert_eq!(store.ops().unwrap().len(), 1);
        // A query appends no op and saves nothing.
        run(&mut store, 1001, Command::ListTerms).unwrap();
        assert_eq!(store.ops().unwrap().len(), 1);
    }

    #[test]
    fn transact_failure_rolls_back_and_appends_nothing() {
        let mut store = Store::open_in_memory().unwrap();
        run(&mut store, 1000, add_term_cmd("Fall")).unwrap();
        let before = store.load().unwrap();
        let res: Result<(), TestErr> = store.transact(1001, |tx| {
            tx.execute(add_term_cmd("Spring"))?;
            Err(TestErr::Boom)
        });
        assert_eq!(res, Err(TestErr::Boom));
        assert_eq!(store.load().unwrap(), before);
        assert_eq!(store.ops().unwrap().len(), 1);
        // A failing command alone appends nothing either (duplicate id).
        assert_eq!(
            run(&mut store, 1002, add_term_cmd("Fall")),
            Err(TestErr::Core)
        );
        assert_eq!(store.ops().unwrap().len(), 1);
        // The failed transactions released their lock: the next one works.
        run(&mut store, 1003, add_term_cmd("Winter")).unwrap();
        assert_eq!(store.load().unwrap().terms.len(), 2);
        assert_eq!(store.ops().unwrap().len(), 2);
    }

    #[test]
    fn swallowed_failed_command_leaves_no_partial_state() {
        let mut store = Store::open_in_memory().unwrap();
        run(&mut store, 1000, add_term_cmd("Fall")).unwrap();
        let _: Result<(), TestErr> = store.transact(1001, |tx| {
            let _ = tx.execute(add_term_cmd("Fall"));
            tx.execute(add_term_cmd("Spring"))?;
            Ok(())
        });
        assert_eq!(term_names(&store), vec!["Fall", "Spring"]);
        assert_eq!(store.ops().unwrap().len(), 2);
    }

    #[test]
    fn multiple_commands_in_one_transaction_get_increasing_hlcs() {
        let mut store = Store::open_in_memory().unwrap();
        let _: Result<(), TestErr> = store.transact(1000, |tx| {
            tx.execute(add_term_cmd("A"))?;
            tx.execute(add_term_cmd("B"))?;
            Ok(())
        });
        let ops = store.ops().unwrap();
        assert_eq!(ops.len(), 2);
        assert!(ops[0].hlc < ops[1].hlc);
        assert_eq!(ops[0].device_id, store.device_id().unwrap());
        // Wall clock going backwards never makes the HLC go backwards.
        run(&mut store, 5, add_term_cmd("C")).unwrap();
        let ops = store.ops().unwrap();
        assert_eq!(ops.len(), 3);
        assert!(ops[1].hlc < ops[2].hlc);
    }

    #[test]
    fn replaying_the_log_matches_the_stored_gradebook() {
        let mut store = Store::open_in_memory().unwrap();
        for (i, n) in ["A", "B", "C"].into_iter().enumerate() {
            run(&mut store, 1000 + i as u64, add_term_cmd(n)).unwrap();
        }
        let r = tally_core::replay(&store.ops().unwrap());
        assert!(r.skipped.is_empty());
        assert_eq!(r.gradebook, store.load().unwrap());
    }

    #[test]
    fn device_id_is_stable_across_reopen_and_cursor_persists() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("tally-store-dev-{}-{nanos}.db", std::process::id()));
        let first = {
            let mut s = Store::open(&path).unwrap();
            assert_eq!(s.cursor().unwrap(), 0);
            s.set_cursor(42).unwrap();
            (s.device_id().unwrap(), s.cursor().unwrap())
        };
        let s = Store::open(&path).unwrap();
        let again = (s.device_id().unwrap(), s.cursor().unwrap());
        drop(s);
        let _ = std::fs::remove_file(&path);
        assert_eq!(first, again);
        assert_eq!(again.1, 42);
    }

    #[test]
    fn transact_rejects_newer_schema_without_running_closure() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .conn
            .execute(
                "INSERT INTO gradebook (id, schema_version, data) VALUES (1, ?1, '{}')",
                params![SCHEMA_VERSION + 1],
            )
            .unwrap();
        let mut ran = false;
        let res: Result<(), TestErr> = store.transact(1000, |_| {
            ran = true;
            Ok(())
        });
        assert_eq!(res, Err(TestErr::Store));
        assert!(!ran);
    }

    // ---- sync ----

    fn rename_cmd(name: &str, new: &str) -> Command {
        Command::RenameTerm {
            id: uuid_from(name),
            name: new.into(),
        }
    }

    fn ids_of(ops: &[Op]) -> Vec<OpId> {
        ops.iter().map(|o| o.op_id).collect()
    }

    /// Exchange all unpushed ops both ways.
    fn sync_pair(a: &mut Store, b: &mut Store, now: u64) {
        let from_a = a.unpushed_ops().unwrap();
        let from_b = b.unpushed_ops().unwrap();
        b.apply_remote_ops(from_a.clone(), now).unwrap();
        a.apply_remote_ops(from_b.clone(), now).unwrap();
        a.mark_pushed(&ids_of(&from_a)).unwrap();
        b.mark_pushed(&ids_of(&from_b)).unwrap();
    }

    #[test]
    fn two_stores_converge_in_both_orders() {
        for a_first in [true, false] {
            let mut a = Store::open_in_memory().unwrap();
            let mut b = Store::open_in_memory().unwrap();
            run(&mut a, 1000, add_term_cmd("Shared")).unwrap();
            sync_pair(&mut a, &mut b, 1001);
            run(&mut a, 2000, rename_cmd("Shared", "From A")).unwrap();
            run(&mut a, 2001, add_term_cmd("OnlyA")).unwrap();
            run(&mut b, 2000, rename_cmd("Shared", "From B")).unwrap();
            run(&mut b, 2002, add_term_cmd("OnlyB")).unwrap();
            let from_a = a.unpushed_ops().unwrap();
            let from_b = b.unpushed_ops().unwrap();
            if a_first {
                b.apply_remote_ops(from_a, 3000).unwrap();
                a.apply_remote_ops(from_b, 3000).unwrap();
            } else {
                a.apply_remote_ops(from_b, 3000).unwrap();
                b.apply_remote_ops(from_a, 3000).unwrap();
            }
            assert_eq!(a.load().unwrap(), b.load().unwrap());
            assert_eq!(a.load().unwrap().terms.len(), 3);
            assert_eq!(a.ops().unwrap(), b.ops().unwrap());
        }
    }

    #[test]
    fn duplicate_remote_ops_apply_once() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        run(&mut a, 1000, add_term_cmd("Fall")).unwrap();
        let ops = a.unpushed_ops().unwrap();
        assert!(b.apply_remote_ops(ops.clone(), 1001).unwrap().is_empty());
        let once = b.load().unwrap();
        assert!(b.apply_remote_ops(ops.clone(), 1002).unwrap().is_empty());
        assert_eq!(b.load().unwrap(), once);
        assert_eq!(b.ops().unwrap().len(), 1);
        // Remote ops are not re-pushed.
        assert!(b.unpushed_ops().unwrap().is_empty());
        // An own op echoed back by the server counts as pushed.
        a.apply_remote_ops(ops, 1003).unwrap();
        assert!(a.unpushed_ops().unwrap().is_empty());
        assert_eq!(a.ops().unwrap().len(), 1);
    }

    #[test]
    fn out_of_order_arrival_triggers_full_replay_and_matches() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        run(&mut a, 1000, add_term_cmd("T")).unwrap();
        sync_pair(&mut a, &mut b, 1001);
        // A renames late, B renames earlier; B's op reaches A after A's own newer op.
        run(&mut a, 2000, rename_cmd("T", "late")).unwrap();
        run(&mut b, 1500, rename_cmd("T", "early")).unwrap();
        let from_a = a.unpushed_ops().unwrap();
        let from_b = b.unpushed_ops().unwrap();
        a.apply_remote_ops(from_b, 2100).unwrap();
        b.apply_remote_ops(from_a, 2100).unwrap();
        // Incremental application would have left A with "early".
        assert_eq!(term_names(&a), vec!["late"]);
        assert_eq!(a.load().unwrap(), b.load().unwrap());
        let r = tally_core::replay(&a.ops().unwrap());
        assert_eq!(r.gradebook, a.load().unwrap());
    }

    #[test]
    fn remote_update_after_delete_is_reported_skipped() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        run(&mut a, 1000, add_term_cmd("T")).unwrap();
        sync_pair(&mut a, &mut b, 1001);
        let remove = Command::RemoveTerm { id: uuid_from("T") };
        run(&mut a, 2000, remove).unwrap();
        run(&mut b, 2500, rename_cmd("T", "renamed")).unwrap();
        let skipped = a.apply_remote_ops(b.unpushed_ops().unwrap(), 3000).unwrap();
        assert_eq!(skipped.len(), 1);
        assert!(a.load().unwrap().terms.is_empty());
    }

    #[test]
    fn apply_remote_ops_advances_the_local_clock() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        run(&mut a, 9000, add_term_cmd("Future")).unwrap();
        b.apply_remote_ops(a.unpushed_ops().unwrap(), 100).unwrap();
        run(&mut b, 100, add_term_cmd("After")).unwrap();
        let ops = b.ops().unwrap();
        assert_eq!(ops.len(), 2);
        assert!(ops[1].hlc > ops[0].hlc);
        assert_eq!(term_names(&b), vec!["After", "Future"]);
    }

    #[test]
    fn v2_database_migrates_to_v3_and_bootstraps() {
        let gb = sample("Fall");
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE gradebook (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                schema_version INTEGER NOT NULL,
                data TEXT NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO gradebook (id, schema_version, data) VALUES (1, 2, ?1)",
            params![serde_json::to_string(&gb).unwrap()],
        )
        .unwrap();
        let mut store = Store::init(conn).unwrap();
        assert_eq!(store.load().unwrap(), gb);
        let version: i64 = store
            .conn
            .query_row("SELECT schema_version FROM gradebook", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert!(store.needs_bootstrap().unwrap());
        assert!(store.ops().unwrap().is_empty());

        assert!(store.ensure_bootstrap(5000).unwrap());
        assert!(!store.needs_bootstrap().unwrap());
        let ops = store.ops().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0].command,
            Command::ImportGradebook {
                gradebook: gb.clone()
            }
        );
        assert_eq!(tally_core::replay(&ops).gradebook, gb);
        // Idempotent.
        assert!(!store.ensure_bootstrap(5001).unwrap());
        assert_eq!(store.ops().unwrap().len(), 1);
    }

    #[test]
    fn fresh_empty_store_does_not_bootstrap() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(!store.needs_bootstrap().unwrap());
        assert!(!store.ensure_bootstrap(1).unwrap());
        assert!(store.ops().unwrap().is_empty());
    }

    #[test]
    fn mutation_on_pre_log_gradebook_bootstraps_first() {
        let mut store = Store::open_in_memory().unwrap();
        store.save(&sample("Old")).unwrap();
        run(&mut store, 1000, add_term_cmd("New")).unwrap();
        let ops = store.ops().unwrap();
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0].command, Command::ImportGradebook { .. }));
        let r = tally_core::replay(&ops);
        assert!(r.skipped.is_empty());
        assert_eq!(r.gradebook, store.load().unwrap());
    }

    #[test]
    fn older_schema_goes_through_migrate_and_equal_loads() {
        let store = Store::open_in_memory().unwrap();
        let gb = sample("Fall");
        let data = serde_json::to_string(&gb).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO gradebook (id, schema_version, data) VALUES (1, ?1, ?2)",
                params![SCHEMA_VERSION - 1, data],
            )
            .unwrap();
        assert_eq!(store.load().unwrap(), gb);
        store
            .conn
            .execute(
                "UPDATE gradebook SET schema_version = ?1",
                params![SCHEMA_VERSION],
            )
            .unwrap();
        assert_eq!(store.load().unwrap(), gb);
    }

    #[test]
    fn busy_timeout_is_set() {
        let store = Store::open_in_memory().unwrap();
        let ms: i64 = store
            .conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ms, 5000);
    }

    #[test]
    fn migrates_v1_pair_arrays() {
        let mut store = Store::open_in_memory().unwrap();
        let v1 = r#"{"terms":[{"id":"00000000-0000-0000-0000-000000000001","name":"T","courses":[
            {"id":"00000000-0000-0000-0000-000000000002","name":"C","credits":3.0,
             "cutoffs":{"entries":[["A",93.0],["F",0.0]]}}]}],
            "scale":{"points":[["A",4.0],["F",0.0]]}}"#;
        store
            .conn
            .execute(
                "INSERT INTO gradebook (id, schema_version, data) VALUES (1, 1, ?1)",
                params![v1],
            )
            .unwrap();
        let gb = store.load().unwrap();
        let course = &gb.terms[0].courses[0];
        assert_eq!(course.cutoffs.get(tally_core::Letter::A), Some(93.0));
        assert_eq!(gb.scale.get(tally_core::Letter::A), Some(4.0));
        // Saving writes the current version and the new shape.
        store.save(&gb).unwrap();
        let reloaded = store.load().unwrap();
        assert_eq!(reloaded, gb);
    }
}
