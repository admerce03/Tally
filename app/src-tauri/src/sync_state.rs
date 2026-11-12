//! Sync glue for the app that does not depend on Tauri: status, config editing and the
//! "never sync concurrently" guard. Adapter code only; the algorithm lives in `tally-sync`.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tally_store::Store;
use tally_sync::{SyncClient, SyncConfig, SyncReport, Transport, sync_shared};

/// Error text returned when a sync is requested while one is running.
pub const ALREADY_RUNNING: &str = "sync already running";

/// Outcome of the most recent sync attempt (also the `sync-finished` event payload).
#[derive(Debug, Clone, Serialize)]
pub struct LastSync {
    pub at_ms: u64,
    pub report: Option<SyncReport>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub configured: bool,
    pub server: Option<String>,
    pub running: bool,
    pub last: Option<LastSync>,
}

#[derive(Default)]
pub struct SyncState {
    running: AtomicBool,
    last: Mutex<Option<LastSync>>,
}

/// Clears the running flag when dropped.
struct RunningGuard<'a>(&'a AtomicBool);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The effective config (config file, then environment overrides), or `None`.
fn load_config(path: &Path) -> Result<Option<SyncConfig>, String> {
    let file = SyncConfig::load_file(path).map_err(|e| e.to_string())?;
    SyncConfig::resolve(None, None, None, file).map_err(|e| e.to_string())
}

impl SyncState {
    pub fn last(&self) -> Option<LastSync> {
        self.last.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn status(&self, config_path: &Path) -> Result<SyncStatus, String> {
        let cfg = load_config(config_path)?;
        Ok(SyncStatus {
            configured: cfg.is_some(),
            server: cfg.map(|c| c.server),
            running: self.is_running(),
            last: self.last(),
        })
    }

    /// Run one sync round over `transport`, recording the outcome. Errs with
    /// [`ALREADY_RUNNING`] (without recording) if another round is in progress.
    pub fn run(
        &self,
        store: &Mutex<Store>,
        transport: &impl Transport,
        now: impl Fn() -> u64,
    ) -> Result<LastSync, String> {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(ALREADY_RUNNING.into());
        }
        let _guard = RunningGuard(&self.running);
        let at_ms = now();
        let result = sync_shared(store, transport, &now);
        let last = match result {
            Ok(report) => LastSync {
                at_ms,
                report: Some(report),
                error: None,
            },
            Err(e) => LastSync {
                at_ms,
                report: None,
                error: Some(e.to_string()),
            },
        };
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = Some(last.clone());
        Ok(last)
    }
}

/// Sync with the configured server (config file, then environment). `Ok(None)` if sync is not
/// configured; errors with [`ALREADY_RUNNING`] if a round is in progress.
pub fn sync_configured(
    sync: &SyncState,
    store: &Mutex<Store>,
    config_path: &Path,
    now: impl Fn() -> u64,
) -> Result<Option<LastSync>, String> {
    let Some(cfg) = load_config(config_path)? else {
        return Ok(None);
    };
    sync.run(store, &SyncClient::new(&cfg), now).map(Some)
}

/// Validate and save the sync settings. An empty server removes the config file.
pub fn config_set(path: &Path, server: &str, token: &str) -> Result<(), String> {
    let server = server.trim().trim_end_matches('/');
    if server.is_empty() {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    if !(server.starts_with("http://") || server.starts_with("https://")) {
        return Err("server must start with http:// or https://".into());
    }
    let token = token.trim();
    if token.is_empty() {
        return Err("a token is required".into());
    }
    SyncConfig {
        server: server.to_string(),
        token: token.to_string(),
    }
    .save_file(path)
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tally_core::Op;
    use tally_sync::{PullResponse, PushResponse, SyncError};

    struct Empty;
    impl Transport for Empty {
        fn push(&self, _: &[Op]) -> Result<PushResponse, SyncError> {
            Ok(PushResponse {
                accepted: 0,
                duplicates: 0,
                head: 0,
            })
        }
        fn pull(&self, since: i64, _: usize) -> Result<PullResponse, SyncError> {
            Ok(PullResponse {
                ops: vec![],
                next: since,
                more: false,
            })
        }
    }

    struct Failing;
    impl Transport for Failing {
        fn push(&self, _: &[Op]) -> Result<PushResponse, SyncError> {
            Err(SyncError::Unauthorized)
        }
        fn pull(&self, _: i64, _: usize) -> Result<PullResponse, SyncError> {
            Err(SyncError::Unauthorized)
        }
    }

    fn store() -> Mutex<Store> {
        Mutex::new(Store::open_in_memory().unwrap())
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("tally-app-test-{}-{name}", std::process::id()))
            .join("sync.json")
    }

    #[test]
    fn run_records_success_and_failure() {
        let st = SyncState::default();
        let ok = st.run(&store(), &Empty, || 5).unwrap();
        assert_eq!(ok.at_ms, 5);
        assert!(ok.report.is_some() && ok.error.is_none());
        let bad = st.run(&store(), &Failing, || 9).unwrap();
        assert!(bad.report.is_none());
        assert!(bad.error.unwrap().contains("unauthorized"));
        assert_eq!(st.last().unwrap().at_ms, 9);
        assert!(!st.is_running());
    }

    #[test]
    fn concurrent_run_is_rejected() {
        let st = SyncState::default();
        st.running.store(true, Ordering::SeqCst);
        let err = st.run(&store(), &Empty, || 1).unwrap_err();
        assert_eq!(err, ALREADY_RUNNING);
        assert!(st.last().is_none());
        // The rejected call must not clear the other run's flag.
        assert!(st.is_running());
    }

    #[test]
    fn guard_clears_after_run() {
        let st = SyncState::default();
        st.run(&store(), &Empty, || 1).unwrap();
        assert!(st.run(&store(), &Empty, || 2).is_ok());
    }

    #[test]
    fn config_set_validates_saves_and_clears() {
        let path = tmp("cfg");
        assert!(config_set(&path, "ftp://x", "t").is_err());
        assert!(config_set(&path, "http://h:1", " ").is_err());
        config_set(&path, " http://h:1/ ", " tok ").unwrap();
        let c = SyncConfig::load_file(&path).unwrap().unwrap();
        assert_eq!((c.server.as_str(), c.token.as_str()), ("http://h:1", "tok"));
        config_set(&path, "", "").unwrap();
        assert!(SyncConfig::load_file(&path).unwrap().is_none());
        config_set(&path, "", "").unwrap();
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
