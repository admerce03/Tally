//! The sync algorithm: push local ops, then pull and merge remote ones.

use std::sync::Mutex;

use serde::Serialize;
use tally_core::{Op, OpId};
use tally_store::Store;

use crate::SyncError;
use crate::protocol::{DEFAULT_PULL_LIMIT, MAX_PUSH_BATCH, PullResponse, PushResponse};

/// How ops get to and from the server. `SyncClient` is the HTTP implementation; tests use an
/// in-process fake.
pub trait Transport {
    fn push(&self, ops: &[Op]) -> Result<PushResponse, SyncError>;
    fn pull(&self, since: i64, limit: usize) -> Result<PullResponse, SyncError>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SyncReport {
    /// Local ops sent to the server.
    pub pushed: usize,
    /// Ops received from the server (including echoes of our own).
    pub pulled: usize,
    /// Received ops that could not be applied, with reasons.
    pub skipped: Vec<(OpId, String)>,
    /// Pull cursor after syncing.
    pub cursor: i64,
}

/// Gives a closure temporary exclusive access to the store (a lock, or a plain borrow).
type Access<'a> = &'a dyn Fn(&mut dyn FnMut(&mut Store));

fn step<T>(
    access: Access<'_>,
    f: impl FnOnce(&mut Store) -> Result<T, SyncError>,
) -> Result<T, SyncError> {
    let mut f = Some(f);
    let mut out = None;
    access(&mut |store| {
        if let Some(f) = f.take() {
            out = Some(f(store));
        }
    });
    out.unwrap_or_else(|| Err(SyncError::Transport("store access failed".into())))
}

fn run(
    access: Access<'_>,
    transport: &impl Transport,
    now: &impl Fn() -> u64,
) -> Result<SyncReport, SyncError> {
    let unpushed = step(access, |s| {
        s.ensure_bootstrap(now())?;
        Ok(s.unpushed_ops()?)
    })?;

    let mut pushed = 0;
    for chunk in unpushed.chunks(MAX_PUSH_BATCH) {
        transport.push(chunk)?;
        let ids: Vec<OpId> = chunk.iter().map(|o| o.op_id).collect();
        step(access, |s| Ok(s.mark_pushed(&ids)?))?;
        pushed += chunk.len();
    }

    let mut pulled = 0;
    let mut skipped = Vec::new();
    let mut cursor = step(access, |s| Ok(s.cursor()?))?;
    loop {
        let page = transport.pull(cursor, DEFAULT_PULL_LIMIT)?;
        pulled += page.ops.len();
        let ops: Vec<Op> = page.ops.into_iter().map(|p| p.op).collect();
        // Never move the cursor backwards or loop forever on a misbehaving server.
        let advanced = page.next > cursor;
        let next = page.next;
        let mut s = step(access, |s| {
            let skipped = s.apply_remote_ops(ops, now())?;
            if advanced {
                s.set_cursor(next)?;
            }
            Ok(skipped)
        })?;
        skipped.append(&mut s);
        if advanced {
            cursor = next;
        }
        if !page.more || !advanced {
            break;
        }
    }
    Ok(SyncReport {
        pushed,
        pulled,
        skipped,
        cursor,
    })
}

/// One full sync round on a store the caller owns. `now` supplies wall-clock milliseconds
/// (core never reads the clock).
pub fn sync_once(
    store: &mut Store,
    transport: &impl Transport,
    now: impl Fn() -> u64,
) -> Result<SyncReport, SyncError> {
    let cell = std::cell::RefCell::new(store);
    run(&|f| f(&mut cell.borrow_mut()), transport, &now)
}

/// Like [`sync_once`] for a shared store: the lock is held only around each store step, never
/// during network calls. Poisoned locks are recovered.
pub fn sync_shared(
    store: &Mutex<Store>,
    transport: &impl Transport,
    now: impl Fn() -> u64,
) -> Result<SyncReport, SyncError> {
    run(
        &|f| f(&mut store.lock().unwrap_or_else(|p| p.into_inner())),
        transport,
        &now,
    )
}
