//! SQLite persistence for the whole `Gradebook` (milestone 1: one JSON row, no sync yet).

use std::cmp::Ordering;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use tally_core::Gradebook;
use thiserror::Error;

/// Bumped when the stored JSON layout changes incompatibly.
pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("stored gradebook is not valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database schema version {found} is newer than supported ({SCHEMA_VERSION})")]
    UnsupportedSchema { found: i64 },
}

pub struct Store {
    conn: Connection,
}

/// How long a writer waits for another process's lock before giving up.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Bring data stored at schema version `from` (older than `SCHEMA_VERSION`) up to date.
/// Version 1 is the first layout, so there is nothing to convert yet; future layouts add
/// one step per version here.
fn migrate(from: i64, data: &str) -> Result<Gradebook, StoreError> {
    debug_assert!(from < SCHEMA_VERSION);
    Ok(serde_json::from_str(data)?)
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
}
