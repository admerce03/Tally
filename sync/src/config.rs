//! Sync client configuration: server URL and bearer token.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::SyncError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Base URL, for example `http://100.64.0.1:7878`.
    pub server: String,
    pub token: String,
}

/// `TALLY_SYNC_CONFIG` if set (non-empty), else `<config dir>/Tally/sync.json`.
pub fn config_path() -> Result<PathBuf, SyncError> {
    if let Some(p) = std::env::var_os("TALLY_SYNC_CONFIG").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let dir = dirs::config_dir()
        .ok_or_else(|| SyncError::Config("could not determine the config directory".into()))?;
    Ok(dir.join("Tally").join("sync.json"))
}

impl SyncConfig {
    /// Read the config file; `None` if it does not exist.
    pub fn load_file(path: &Path) -> Result<Option<SyncConfig>, SyncError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| SyncError::Config(format!("{}: {e}", path.display())))
    }

    /// Write the config file, creating parent directories.
    pub fn save_file(&self, path: &Path) -> Result<(), SyncError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text =
            serde_json::to_string_pretty(self).map_err(|e| SyncError::Config(e.to_string()))?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Resolve the effective config. Each field: flag, then environment (`TALLY_SERVER`,
    /// `TALLY_TOKEN`, `TALLY_TOKEN_FILE`), then the config file. `None` if no server is set;
    /// an error if a server is set without a token.
    pub fn resolve(
        server_flag: Option<String>,
        token_flag: Option<String>,
        token_file_flag: Option<PathBuf>,
        file: Option<SyncConfig>,
    ) -> Result<Option<SyncConfig>, SyncError> {
        Self::resolve_with(server_flag, token_flag, token_file_flag, file, |k| {
            std::env::var(k).ok()
        })
    }

    /// [`SyncConfig::resolve`] with an injectable environment lookup.
    pub fn resolve_with(
        server_flag: Option<String>,
        token_flag: Option<String>,
        token_file_flag: Option<PathBuf>,
        file: Option<SyncConfig>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Option<SyncConfig>, SyncError> {
        let non_empty =
            |s: Option<String>| s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let server = non_empty(server_flag)
            .or_else(|| non_empty(env("TALLY_SERVER")))
            .or_else(|| {
                file.as_ref()
                    .and_then(|f| non_empty(Some(f.server.clone())))
            });
        let Some(server) = server else {
            return Ok(None);
        };
        let read_token_file = |p: &Path| -> Result<Option<String>, SyncError> {
            let text = std::fs::read_to_string(p)
                .map_err(|e| SyncError::Config(format!("token file {}: {e}", p.display())))?;
            Ok(non_empty(Some(text)))
        };
        let token = match non_empty(token_flag) {
            Some(t) => Some(t),
            None => match token_file_flag {
                Some(p) => read_token_file(&p)?,
                None => None,
            },
        };
        let token = match token {
            Some(t) => Some(t),
            None => match non_empty(env("TALLY_TOKEN")) {
                Some(t) => Some(t),
                None => match env("TALLY_TOKEN_FILE").filter(|p| !p.trim().is_empty()) {
                    Some(p) => read_token_file(Path::new(p.trim()))?,
                    None => None,
                },
            },
        };
        let token = token.or_else(|| file.as_ref().and_then(|f| non_empty(Some(f.token.clone()))));
        let Some(token) = token else {
            return Err(SyncError::Config(
                "a server is configured but no token; set --token, TALLY_TOKEN or TALLY_TOKEN_FILE"
                    .into(),
            ));
        };
        Ok(Some(SyncConfig {
            server: server.trim_end_matches('/').to_string(),
            token,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| m.get(k).cloned()
    }

    fn file_cfg() -> Option<SyncConfig> {
        Some(SyncConfig {
            server: "http://file:1".into(),
            token: "filetok".into(),
        })
    }

    fn tmp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tally-sync-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn flag_beats_env_beats_file() {
        let e = env(&[("TALLY_SERVER", "http://env:2"), ("TALLY_TOKEN", "envtok")]);
        let c = SyncConfig::resolve_with(
            Some("http://flag:3/".into()),
            Some("flagtok".into()),
            None,
            file_cfg(),
            &e,
        )
        .unwrap()
        .unwrap();
        assert_eq!(c.server, "http://flag:3");
        assert_eq!(c.token, "flagtok");
        let c = SyncConfig::resolve_with(None, None, None, file_cfg(), &e)
            .unwrap()
            .unwrap();
        assert_eq!(
            (c.server.as_str(), c.token.as_str()),
            ("http://env:2", "envtok")
        );
        let c = SyncConfig::resolve_with(None, None, None, file_cfg(), env(&[]))
            .unwrap()
            .unwrap();
        assert_eq!(
            (c.server.as_str(), c.token.as_str()),
            ("http://file:1", "filetok")
        );
    }

    #[test]
    fn no_server_is_none_and_missing_token_is_error() {
        assert!(
            SyncConfig::resolve_with(None, Some("t".into()), None, None, env(&[]))
                .unwrap()
                .is_none()
        );
        let err = SyncConfig::resolve_with(Some("http://x".into()), None, None, None, env(&[]));
        assert!(matches!(err, Err(SyncError::Config(_))));
    }

    #[test]
    fn token_file_is_read_and_trimmed() {
        let dir = tmp("tokfile");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("token");
        std::fs::write(&p, "  secret\n").unwrap();
        let c = SyncConfig::resolve_with(
            Some("http://x".into()),
            None,
            Some(p.clone()),
            None,
            env(&[("TALLY_TOKEN", "envtok")]),
        )
        .unwrap()
        .unwrap();
        assert_eq!(c.token, "secret");
        let e = env(&[("TALLY_TOKEN_FILE", p.to_str().unwrap())]);
        let c = SyncConfig::resolve_with(Some("http://x".into()), None, None, None, e)
            .unwrap()
            .unwrap();
        assert_eq!(c.token, "secret");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_round_trip_creates_parent_dirs() {
        let dir = tmp("cfg");
        let p = dir.join("nested").join("sync.json");
        assert_eq!(SyncConfig::load_file(&p).unwrap(), None);
        let cfg = SyncConfig {
            server: "http://a".into(),
            token: "t".into(),
        };
        cfg.save_file(&p).unwrap();
        assert_eq!(SyncConfig::load_file(&p).unwrap(), Some(cfg));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
