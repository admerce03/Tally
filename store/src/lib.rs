//! SQLite persistence for the whole `Gradebook` (milestone 1: one JSON row, no sync yet).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use tally_core::Gradebook;
use thiserror::Error;

/// Bumped when the stored JSON layout changes incompatibly.
pub const SCHEMA_VERSION: i64 = 2;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored gradebook is not valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database schema version {found} is newer than supported ({SCHEMA_VERSION})")]
    UnsupportedSchema { found: i64 },
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

/// Bring data stored at schema version `from` (older than `SCHEMA_VERSION`) up to date.
///
/// v1 -> v2: letter cutoffs and the GPA scale changed from `[letter, value]` pairs to
/// `{letter, min}` / `{letter, points}` objects.
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

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS gradebook (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                schema_version INTEGER NOT NULL,
                data TEXT NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    /// The stored gradebook, or an empty default if nothing was saved yet.
    pub fn load(&self) -> Result<Gradebook, StoreError> {
        load_from(&self.conn)
    }

    /// Replace the stored gradebook, atomically.
    pub fn save(&mut self, gb: &Gradebook) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        save_to(&tx, gb)?;
        tx.commit()?;
        Ok(())
    }

    /// Load, run `f`, and save if `f` asks for it, all inside one `BEGIN IMMEDIATE`
    /// transaction so concurrent processes cannot lose each other's updates. `f` returns its
    /// result plus whether to save. If `f` or the save fails, nothing is written.
    pub fn transact<T, E>(
        &mut self,
        f: impl FnOnce(&mut Gradebook) -> Result<(T, bool), E>,
    ) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::from)?;
        let mut gb = load_from(&tx)?;
        let (value, save) = f(&mut gb)?;
        if save {
            save_to(&tx, &gb)?;
        }
        tx.commit().map_err(StoreError::from)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tally_core::{Command, TermId, execute};

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
        Boom,
    }

    impl From<StoreError> for TestErr {
        fn from(_: StoreError) -> Self {
            TestErr::Store
        }
    }

    fn add_term(gb: &mut Gradebook, name: &str) {
        let id = uuid_from(name);
        let _ = execute(
            gb,
            Command::AddTerm {
                id,
                name: name.into(),
            },
        );
    }

    #[test]
    fn transact_saves_when_asked() {
        let mut store = Store::open_in_memory().unwrap();
        let n: Result<usize, TestErr> = store.transact(|gb| {
            add_term(gb, "Fall");
            Ok((gb.terms.len(), true))
        });
        assert_eq!(n, Ok(1));
        assert_eq!(store.load().unwrap().terms.len(), 1);
        // Read-only: changes made by the closure are not persisted.
        let n: Result<usize, TestErr> = store.transact(|gb| {
            add_term(gb, "Spring");
            Ok((gb.terms.len(), false))
        });
        assert_eq!(n, Ok(2));
        assert_eq!(store.load().unwrap().terms.len(), 1);
    }

    #[test]
    fn transact_failure_rolls_back() {
        let mut store = Store::open_in_memory().unwrap();
        store.save(&sample("Fall")).unwrap();
        let before = store.load().unwrap();
        let res: Result<(), TestErr> = store.transact(|gb| {
            add_term(gb, "Spring");
            Err(TestErr::Boom)
        });
        assert_eq!(res, Err(TestErr::Boom));
        assert_eq!(store.load().unwrap(), before);
        // The failed transaction released its lock: the next one works.
        let ok: Result<(), TestErr> = store.transact(|gb| {
            add_term(gb, "Winter");
            Ok(((), true))
        });
        assert_eq!(ok, Ok(()));
        assert_eq!(store.load().unwrap().terms.len(), 2);
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
        let res: Result<(), TestErr> = store.transact(|_| {
            ran = true;
            Ok(((), false))
        });
        assert_eq!(res, Err(TestErr::Store));
        assert!(!ran);
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
