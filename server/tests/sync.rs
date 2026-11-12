//! Device <-> server sync scenarios: an in-process fake transport over a server `Store`, plus
//! one end-to-end run over real HTTP.

use std::sync::Mutex;

use serde_json::{Value, json};
use tally_core::{Command, Gradebook, Op, execute};
use tally_server::{AppState, router};
use tally_store::Store;
use tally_sync::protocol::{PullResponse, PulledOp, PushResponse};
use tally_sync::{SyncClient, SyncConfig, SyncError, Transport, sync_once, sync_shared};

const TERM: &str = "10000000-0000-4000-8000-000000000001";
const COURSE: &str = "20000000-0000-4000-8000-000000000001";
const CAT: &str = "30000000-0000-4000-8000-000000000001";
const ASG: &str = "40000000-0000-4000-8000-000000000001";

fn cmd(v: Value) -> Command {
    serde_json::from_value(v).expect("valid command")
}

fn run(store: &mut Store, now: u64, v: Value) {
    store
        .transact(now, |tx| Ok::<_, anyhow::Error>(tx.execute(cmd(v))?))
        .expect("command succeeds");
}

fn try_run(store: &mut Store, now: u64, v: Value) -> Result<(), anyhow::Error> {
    store.transact(now, |tx| {
        Ok::<_, anyhow::Error>(tx.execute(cmd(v)).map(|_| ())?)
    })
}

fn structure(store: &mut Store, now: u64) {
    run(
        store,
        now,
        json!({"command":"add_term","id":TERM,"name":"Fall"}),
    );
    run(
        store,
        now + 1,
        json!({"command":"add_course","term_id":TERM,"id":COURSE,"name":"Math","credits":3.0}),
    );
    run(
        store,
        now + 2,
        json!({"command":"add_category","course_id":COURSE,"id":CAT,"name":"HW","weight":100.0}),
    );
    run(
        store,
        now + 3,
        json!({"command":"add_assignment","category_id":CAT,"id":ASG,"name":"HW1","points_possible":10.0}),
    );
}

fn assignment(gb: &Gradebook) -> Option<(String, Option<f64>)> {
    gb.terms
        .first()?
        .courses
        .first()?
        .categories
        .first()?
        .assignments
        .first()
        .map(|a| (a.name.clone(), a.score))
}

/// In-process stand-in for the HTTP server.
struct Fake {
    server: Mutex<Store>,
    pushes: Mutex<Vec<usize>>,
    pulls: Mutex<usize>,
}

impl Fake {
    fn new() -> Self {
        Fake {
            server: Mutex::new(Store::open_in_memory().unwrap()),
            pushes: Mutex::new(Vec::new()),
            pulls: Mutex::new(0),
        }
    }
}

impl Transport for Fake {
    fn push(&self, ops: &[Op]) -> Result<PushResponse, SyncError> {
        self.pushes.lock().unwrap().push(ops.len());
        let mut s = self.server.lock().unwrap();
        let r = s.server_push(ops.to_vec(), 1)?;
        let accepted = r.accepted.iter().filter(|a| **a).count();
        Ok(PushResponse {
            accepted,
            duplicates: r.accepted.len() - accepted,
            head: s.server_log_head()?,
        })
    }

    fn pull(&self, since: i64, limit: usize) -> Result<PullResponse, SyncError> {
        *self.pulls.lock().unwrap() += 1;
        let page = self.server.lock().unwrap().server_log_pull(since, limit)?;
        Ok(PullResponse {
            ops: page
                .ops
                .into_iter()
                .map(|(seq, op)| PulledOp { seq, op })
                .collect(),
            next: page.next,
            more: page.more,
        })
    }
}

fn device() -> Store {
    Store::open_in_memory().unwrap()
}

fn sync(s: &mut Store, f: &Fake) -> tally_sync::SyncReport {
    sync_once(s, f, || 50_000).expect("sync")
}

fn assert_converged(a: &Store, b: &Store) {
    assert_eq!(a.load().unwrap(), b.load().unwrap());
    let key = |s: &Store| s.ops().unwrap().iter().map(|o| o.op_id).collect::<Vec<_>>();
    assert_eq!(key(a), key(b));
}

#[test]
fn two_devices_converge() {
    let f = Fake::new();
    let (mut a, mut b) = (device(), device());
    run(
        &mut a,
        1_000,
        json!({"command":"add_term","id":TERM,"name":"A term"}),
    );
    run(
        &mut b,
        2_000,
        json!({"command":"add_term","id":"10000000-0000-4000-8000-000000000002","name":"B term"}),
    );
    sync(&mut a, &f);
    sync(&mut b, &f);
    sync(&mut a, &f);
    assert_converged(&a, &b);
    assert_eq!(a.load().unwrap().terms.len(), 2);
}

#[test]
fn concurrent_field_edits_both_apply_and_same_field_is_lww() {
    let f = Fake::new();
    let (mut a, mut b) = (device(), device());
    structure(&mut a, 1_000);
    sync(&mut a, &f);
    sync(&mut b, &f);

    // Different fields: both survive.
    run(
        &mut a,
        100_000,
        json!({"command":"update_assignment","id":ASG,"name":"Renamed"}),
    );
    run(
        &mut b,
        100_001,
        json!({"command":"update_assignment","id":ASG,"score":8.0}),
    );
    sync(&mut a, &f);
    sync(&mut b, &f);
    sync(&mut a, &f);
    assert_converged(&a, &b);
    assert_eq!(
        assignment(&a.load().unwrap()),
        Some(("Renamed".into(), Some(8.0)))
    );

    // Same field: the higher HLC wins on both, whatever the sync order.
    run(
        &mut b,
        120_000,
        json!({"command":"update_assignment","id":ASG,"name":"from B"}),
    );
    run(
        &mut a,
        110_000,
        json!({"command":"update_assignment","id":ASG,"name":"from A"}),
    );
    sync(&mut b, &f);
    sync(&mut a, &f);
    sync(&mut b, &f);
    assert_converged(&a, &b);
    assert_eq!(assignment(&a.load().unwrap()).unwrap().0, "from B");
}

#[test]
fn delete_beats_earlier_update_and_later_update_is_skipped() {
    let f = Fake::new();
    let (mut a, mut b) = (device(), device());
    structure(&mut a, 1_000);
    sync(&mut a, &f);
    sync(&mut b, &f);

    // Update older than the delete: delete wins.
    run(
        &mut b,
        130_000,
        json!({"command":"update_assignment","id":ASG,"score":5.0}),
    );
    run(
        &mut a,
        140_000,
        json!({"command":"remove_assignment","id":ASG}),
    );
    sync(&mut b, &f);
    sync(&mut a, &f);
    sync(&mut b, &f);
    assert_converged(&a, &b);
    assert_eq!(assignment(&a.load().unwrap()), None);

    // Update newer than the delete (made offline on B before B saw the delete).
    let f = Fake::new();
    let (mut a, mut b) = (device(), device());
    structure(&mut a, 1_000);
    sync(&mut a, &f);
    sync(&mut b, &f);
    run(
        &mut a,
        140_000,
        json!({"command":"remove_assignment","id":ASG}),
    );
    run(
        &mut b,
        150_000,
        json!({"command":"update_assignment","id":ASG,"score":5.0}),
    );
    sync(&mut a, &f);
    let rb = sync(&mut b, &f);
    let ra = sync(&mut a, &f);
    assert_converged(&a, &b);
    assert_eq!(assignment(&a.load().unwrap()), None);
    // The no-op update is reported by the device that applied it after the delete.
    assert!(
        !rb.skipped.is_empty() || !ra.skipped.is_empty(),
        "update after delete should be reported as skipped"
    );
}

#[test]
fn offline_then_one_sync_and_chunking() {
    let f = Fake::new();
    let (mut a, mut b) = (device(), device());
    for i in 0..1_200u32 {
        let id = format!("50000000-0000-4000-8000-{i:012}");
        run(
            &mut a,
            1_000 + u64::from(i),
            json!({"command":"add_term","id":id,"name":format!("T{i}")}),
        );
    }
    let r = sync(&mut a, &f);
    assert_eq!(r.pushed, 1_200);
    assert_eq!(*f.pushes.lock().unwrap(), vec![500, 500, 200]);
    assert_eq!(r.cursor, 1_200);
    assert!(r.skipped.is_empty());

    let before = *f.pulls.lock().unwrap();
    let r = sync(&mut b, &f);
    assert_eq!(r.pulled, 1_200);
    assert!(*f.pulls.lock().unwrap() - before >= 3, "pulled in pages");
    assert_eq!(b.load().unwrap().terms.len(), 1_200);
    assert_converged(&a, &b);
}

#[test]
fn push_is_idempotent_after_lost_ack() {
    let f = Fake::new();
    let mut a = device();
    structure(&mut a, 1_000);
    let ops = a.unpushed_ops().unwrap();
    // Push but never mark_pushed (lost acknowledgement), twice.
    let first = f.push(&ops).unwrap();
    assert_eq!((first.accepted, first.duplicates), (ops.len(), 0));
    let second = f.push(&ops).unwrap();
    assert_eq!((second.accepted, second.duplicates), (0, ops.len()));
    assert_eq!(second.head, first.head);
    // A real sync re-sends the unacknowledged ops: still no duplicates, same state.
    let r = sync(&mut a, &f);
    assert_eq!(r.pushed, ops.len());
    assert_eq!(
        f.server.lock().unwrap().server_log_head().unwrap(),
        first.head
    );
    assert_eq!(f.server.lock().unwrap().load().unwrap(), a.load().unwrap());
    let again = sync(&mut a, &f);
    assert_eq!(again.pushed, 0);
}

#[test]
fn existing_gradebook_bootstraps_via_import() {
    let f = Fake::new();
    let (mut a, mut b) = (device(), device());
    let mut gb = Gradebook::default();
    execute(
        &mut gb,
        cmd(json!({"command":"add_term","id":TERM,"name":"Old"})),
    )
    .unwrap();
    a.save(&gb).unwrap();
    assert!(a.ops().unwrap().is_empty());
    let r = sync(&mut a, &f);
    assert_eq!(r.pushed, 1);
    assert!(matches!(
        a.ops().unwrap()[0].command,
        Command::ImportGradebook { .. }
    ));
    sync(&mut b, &f);
    assert_eq!(b.load().unwrap(), gb);
}

#[test]
fn server_side_edits_reach_devices() {
    let f = Fake::new();
    let mut a = device();
    structure(&mut a, 1_000);
    sync(&mut a, &f);
    {
        let mut s = f.server.lock().unwrap();
        run(
            &mut s,
            20_000,
            json!({"command":"update_assignment","id":ASG,"name":"Server edit"}),
        );
        assert_eq!(s.server_log_register_local().unwrap(), 1);
    }
    sync(&mut a, &f);
    assert_eq!(assignment(&a.load().unwrap()).unwrap().0, "Server edit");
}

#[test]
fn shared_variant_syncs_without_holding_lock_across_network() {
    // A transport that fails the test if the store lock is held during a network call.
    struct Probe<'a> {
        inner: &'a Fake,
        store: &'a Mutex<Store>,
    }
    impl Transport for Probe<'_> {
        fn push(&self, ops: &[Op]) -> Result<PushResponse, SyncError> {
            assert!(self.store.try_lock().is_ok(), "lock held during push");
            self.inner.push(ops)
        }
        fn pull(&self, since: i64, limit: usize) -> Result<PullResponse, SyncError> {
            assert!(self.store.try_lock().is_ok(), "lock held during pull");
            self.inner.pull(since, limit)
        }
    }
    let f = Fake::new();
    let mut a = device();
    structure(&mut a, 1_000);
    sync(&mut a, &f);

    let mut b = device();
    run(
        &mut b,
        2_000,
        json!({"command":"add_term","id":"10000000-0000-4000-8000-000000000009","name":"B"}),
    );
    let shared = Mutex::new(b);
    let probe = Probe {
        inner: &f,
        store: &shared,
    };
    let r = sync_shared(&shared, &probe, || 60_000).unwrap();
    assert_eq!(r.pushed, 1);
    assert_eq!(r.pulled, 5);
    sync(&mut a, &f);
    assert_converged(&a, &shared.into_inner().unwrap());
}

#[test]
fn end_to_end_over_http() {
    let server_store = Store::open_in_memory().unwrap();
    let state = AppState::new(server_store, "tok".into(), vec![], vec![]);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(listener.local_addr().unwrap()).unwrap();
            axum::serve(listener, router(state)).await.unwrap();
        });
    });
    let addr = rx.recv().unwrap();
    let cfg = SyncConfig {
        server: format!("http://{addr}"),
        token: "tok".into(),
    };
    let client = SyncClient::new(&cfg);
    assert_eq!(client.health().unwrap().status, "ok");

    let (mut a, mut b) = (device(), device());
    structure(&mut a, 1_000);
    let r = sync_once(&mut a, &client, || 50_000).unwrap();
    assert_eq!(r.pushed, 4);
    sync_once(&mut b, &client, || 50_000).unwrap();
    assert_converged(&a, &b);
    assert!(
        try_run(
            &mut b,
            60_000,
            json!({"command":"update_assignment","id":ASG,"score":9.0})
        )
        .is_ok()
    );
    sync_once(&mut b, &client, || 70_000).unwrap();
    sync_once(&mut a, &client, || 70_000).unwrap();
    assert_eq!(assignment(&a.load().unwrap()).unwrap().1, Some(9.0));

    let bad = SyncClient::new(&SyncConfig {
        server: cfg.server.clone(),
        token: "nope".into(),
    });
    assert!(matches!(bad.pull(0, 10), Err(SyncError::Unauthorized)));
}
