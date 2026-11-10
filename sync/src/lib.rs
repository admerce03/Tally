//! `tally-sync`: wire protocol, client configuration and the sync algorithm shared by the
//! `grades` CLI and the desktop app. No grade math lives here.

pub mod config;
mod engine;
pub mod protocol;

#[cfg(feature = "client")]
mod client;

pub use config::{SyncConfig, config_path};
pub use engine::{SyncReport, Transport, sync_once, sync_shared};
pub use protocol::*;

#[cfg(feature = "client")]
pub use client::SyncClient;

/// Everything that can go wrong while syncing.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Store(#[from] tally_store::StoreError),
    #[error("server returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("unauthorized: the server rejected the token")]
    Unauthorized,
    #[error("could not reach the server: {0}")]
    Transport(String),
    #[error("sync is not configured correctly: {0}")]
    Config(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
