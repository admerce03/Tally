//! Sync model: hybrid logical clock, ops and deterministic replay. Pure: the wall clock is
//! always passed in by the adapter.
//!
//! State is the replay of all known ops sorted by `(hlc, device_id, op_id)`, starting from an
//! empty gradebook. A command that fails during replay is skipped (and reported), identically
//! on every device. Updates only touch the fields they set, so replay order gives
//! last-write-wins per field.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::command::{Command, execute};
use crate::error::CoreError;
use crate::ids::{DeviceId, OpId};
use crate::model::Gradebook;

/// Hybrid logical clock timestamp. Orders by wall time, then counter.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Hlc {
    pub wall_ms: u64,
    pub counter: u32,
}

impl Hlc {
    pub const ZERO: Hlc = Hlc {
        wall_ms: 0,
        counter: 0,
    };

    /// The timestamp right after `(wall_ms, counter)`: counter increments within the same wall
    /// time. On counter overflow the wall time advances by one ms with counter 0 (saturating
    /// at `u64::MAX`).
    fn after(wall_ms: u64, counter: u32) -> Hlc {
        match counter.checked_add(1) {
            Some(counter) => Hlc { wall_ms, counter },
            None => Hlc {
                wall_ms: wall_ms.saturating_add(1),
                counter: 0,
            },
        }
    }
}

/// Timestamp for a local event: never goes backwards even if the wall clock does.
pub fn tick(local: Hlc, now_ms: u64) -> Hlc {
    if now_ms > local.wall_ms {
        Hlc {
            wall_ms: now_ms,
            counter: 0,
        }
    } else {
        Hlc::after(local.wall_ms, local.counter)
    }
}

/// New local clock after receiving `remote`: strictly greater than both inputs.
pub fn recv(local: Hlc, remote: Hlc, now_ms: u64) -> Hlc {
    let max = local.max(remote);
    if now_ms > max.wall_ms {
        Hlc {
            wall_ms: now_ms,
            counter: 0,
        }
    } else {
        Hlc::after(max.wall_ms, max.counter)
    }
}

/// One recorded mutation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Op {
    pub op_id: OpId,
    pub device_id: DeviceId,
    pub hlc: Hlc,
    pub command: Command,
}

/// Total order of ops: `(hlc, device_id, op_id)`.
pub type OpKey = (Hlc, DeviceId, OpId);

impl Op {
    pub fn key(&self) -> OpKey {
        (self.hlc, self.device_id, self.op_id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayResult {
    pub gradebook: Gradebook,
    /// Ops that could not be applied, with the reason.
    pub skipped: Vec<(OpId, String)>,
}

/// Apply one op to the gradebook. Only mutations are accepted. On error the gradebook is
/// left unchanged.
pub fn apply_op(gb: &mut Gradebook, op: &Op) -> Result<(), CoreError> {
    if !op.command.is_mutation() {
        return Err(CoreError::NotAMutation {
            tag: op.command.tag(),
        });
    }
    let mut next = gb.clone();
    execute(&mut next, op.command.clone())?;
    *gb = next;
    Ok(())
}

/// Apply ops in the given order onto `gb` (the caller sorts), collecting skipped ones.
pub fn apply_sorted<'a>(
    gb: &mut Gradebook,
    ops: impl IntoIterator<Item = &'a Op>,
) -> Vec<(OpId, String)> {
    let mut skipped = Vec::new();
    for op in ops {
        if let Err(e) = apply_op(gb, op) {
            skipped.push((op.op_id, e.to_string()));
        }
    }
    skipped
}

/// Rebuild the gradebook from ops, independent of input order. Duplicate op ids are applied
/// once.
pub fn replay<'a>(ops: impl IntoIterator<Item = &'a Op>) -> ReplayResult {
    let mut sorted: Vec<&Op> = ops.into_iter().collect();
    sorted.sort_by_key(|o| o.key());
    let mut seen = HashSet::new();
    sorted.retain(|o| seen.insert(o.op_id));
    let mut gradebook = Gradebook::default();
    let skipped = apply_sorted(&mut gradebook, sorted);
    ReplayResult { gradebook, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{AssignmentId, CategoryId, CourseId, TermId};
    use crate::model::Assignment;
    use crate::testutil::uid;

    fn h(wall_ms: u64, counter: u32) -> Hlc {
        Hlc { wall_ms, counter }
    }

    #[test]
    fn tick_advances_with_wall_clock() {
        assert_eq!(tick(h(10, 5), 20), h(20, 0));
    }

    #[test]
    fn tick_same_ms_increments_counter() {
        assert_eq!(tick(h(10, 5), 10), h(10, 6));
    }

    #[test]
    fn tick_clock_going_backwards_stays_monotonic() {
        let a = tick(h(100, 0), 50);
        assert_eq!(a, h(100, 1));
        assert_eq!(tick(a, 10), h(100, 2));
    }

    #[test]
    fn tick_counter_overflow_advances_wall() {
        let t = tick(h(10, u32::MAX), 5);
        assert_eq!(t, h(11, 0));
        assert!(t > h(10, u32::MAX));
        assert_eq!(tick(h(u64::MAX, u32::MAX), 0), h(u64::MAX, 0));
    }

    #[test]
    fn recv_takes_max_of_all_three() {
        assert_eq!(recv(h(10, 1), h(10, 3), 5), h(10, 4));
        assert_eq!(recv(h(10, 7), h(10, 3), 5), h(10, 8));
        assert_eq!(recv(h(10, 7), h(12, 3), 5), h(12, 4));
        assert_eq!(recv(h(15, 7), h(12, 3), 5), h(15, 8));
        assert_eq!(recv(h(10, 7), h(12, 3), 30), h(30, 0));
    }

    #[test]
    fn recv_is_greater_than_both_even_when_clock_goes_backwards() {
        let r = recv(h(100, 2), h(90, 9), 1);
        assert!(r > h(100, 2) && r > h(90, 9));
    }

    #[test]
    fn recv_counter_overflow_advances_wall() {
        assert_eq!(recv(h(10, u32::MAX), h(3, 0), 1), h(11, 0));
        assert_eq!(recv(h(3, 0), h(10, u32::MAX), 1), h(11, 0));
    }

    #[test]
    fn hlc_orders_by_wall_then_counter() {
        assert!(h(1, 9) < h(2, 0));
        assert!(h(2, 0) < h(2, 1));
    }

    // ---- replay ----

    fn dev(n: u128) -> DeviceId {
        DeviceId(uuid::Uuid::from_u128(n))
    }

    fn op(device: u128, hlc: Hlc, command: Command) -> Op {
        Op {
            op_id: OpId(uid()),
            device_id: dev(device),
            hlc,
            command,
        }
    }

    struct Fx {
        base: Vec<Op>,
        asg: AssignmentId,
    }

    /// Term, course, category and assignment created by device 1 at wall 1..4.
    fn fixture() -> Fx {
        let (t, c, k, a) = (
            TermId(uid()),
            CourseId(uid()),
            CategoryId(uid()),
            AssignmentId(uid()),
        );
        let base = vec![
            op(
                1,
                h(1, 0),
                Command::AddTerm {
                    id: t,
                    name: "Fall".into(),
                },
            ),
            op(
                1,
                h(2, 0),
                Command::AddCourse {
                    term_id: t,
                    id: c,
                    name: "Math".into(),
                    code: None,
                    credits: 3.0,
                    target: None,
                },
            ),
            op(
                1,
                h(3, 0),
                Command::AddCategory {
                    course_id: c,
                    id: k,
                    name: "HW".into(),
                    weight: 100.0,
                    drop_lowest: 0,
                },
            ),
            op(
                1,
                h(4, 0),
                Command::AddAssignment {
                    category_id: k,
                    id: a,
                    name: "hw1".into(),
                    points_possible: 10.0,
                    score: None,
                    due: None,
                    status: None,
                    curves: vec![],
                },
            ),
        ];
        Fx { base, asg: a }
    }

    fn upd(id: AssignmentId, name: Option<&str>, score: Option<f64>) -> Command {
        Command::UpdateAssignment {
            id,
            name: name.map(str::to_string),
            points_possible: None,
            score: score.map(Some),
            due: None,
            status: None,
            curves: None,
        }
    }

    fn the_assignment(gb: &Gradebook, id: AssignmentId) -> Option<&Assignment> {
        gb.terms
            .iter()
            .flat_map(|t| &t.courses)
            .flat_map(|c| &c.categories)
            .flat_map(|k| &k.assignments)
            .find(|a| a.id == id)
    }

    fn score_of(gb: &Gradebook, id: AssignmentId) -> Option<f64> {
        the_assignment(gb, id).and_then(|a| a.score)
    }

    #[test]
    fn concurrent_updates_to_different_fields_both_apply() {
        let fx = fixture();
        let mut ops = fx.base.clone();
        ops.push(op(2, h(10, 0), upd(fx.asg, Some("Quiz"), None)));
        ops.push(op(3, h(10, 0), upd(fx.asg, None, Some(7.0))));
        let r = replay(&ops);
        assert!(r.skipped.is_empty());
        let a = the_assignment(&r.gradebook, fx.asg).unwrap();
        assert_eq!(a.name, "Quiz");
        assert_eq!(a.score, Some(7.0));
    }

    #[test]
    fn same_field_higher_hlc_wins_regardless_of_arrival_order() {
        let fx = fixture();
        let early = op(2, h(10, 0), upd(fx.asg, None, Some(3.0)));
        let late = op(3, h(11, 0), upd(fx.asg, None, Some(9.0)));
        for order in [[&early, &late], [&late, &early]] {
            let mut ops = fx.base.clone();
            ops.extend(order.iter().map(|o| (*o).clone()));
            let r = replay(&ops);
            assert_eq!(score_of(&r.gradebook, fx.asg), Some(9.0));
        }
    }

    #[test]
    fn same_hlc_ties_break_by_device_id() {
        let fx = fixture();
        let a = op(2, h(10, 0), upd(fx.asg, None, Some(3.0)));
        let b = op(3, h(10, 0), upd(fx.asg, None, Some(9.0)));
        let mut ops = fx.base.clone();
        ops.extend([b, a]);
        assert_eq!(score_of(&replay(&ops).gradebook, fx.asg), Some(9.0));
    }

    #[test]
    fn delete_after_update_is_gone() {
        let fx = fixture();
        let mut ops = fx.base.clone();
        ops.push(op(2, h(10, 0), upd(fx.asg, None, Some(5.0))));
        ops.push(op(3, h(11, 0), Command::RemoveAssignment { id: fx.asg }));
        let r = replay(&ops);
        assert!(r.skipped.is_empty());
        assert!(the_assignment(&r.gradebook, fx.asg).is_none());
    }

    #[test]
    fn update_after_delete_is_skipped_and_recorded() {
        let fx = fixture();
        let mut ops = fx.base.clone();
        ops.push(op(3, h(10, 0), Command::RemoveAssignment { id: fx.asg }));
        let late = op(2, h(11, 0), upd(fx.asg, None, Some(5.0)));
        let late_id = late.op_id;
        ops.push(late);
        let r = replay(&ops);
        assert!(the_assignment(&r.gradebook, fx.asg).is_none());
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].0, late_id);
    }

    #[test]
    fn replay_is_independent_of_input_order() {
        let fx = fixture();
        let mut ops = fx.base.clone();
        ops.push(op(2, h(10, 0), upd(fx.asg, Some("Quiz"), None)));
        ops.push(op(3, h(10, 1), upd(fx.asg, None, Some(7.0))));
        ops.push(op(2, h(12, 0), upd(fx.asg, None, Some(8.0))));
        let forward = replay(&ops);
        let mut reversed = ops.clone();
        reversed.reverse();
        assert_eq!(forward, replay(&reversed));
        let mut rotated = ops.clone();
        rotated.rotate_left(3);
        assert_eq!(forward, replay(&rotated));
    }

    #[test]
    fn replay_applies_duplicate_op_ids_once() {
        let fx = fixture();
        let mut ops = fx.base.clone();
        ops.extend(fx.base.clone());
        let r = replay(&ops);
        assert!(r.skipped.is_empty());
        assert_eq!(r.gradebook, replay(&fx.base).gradebook);
    }

    #[test]
    fn non_mutations_are_skipped() {
        let q = op(1, h(1, 0), Command::ListTerms);
        let r = replay([&q]);
        assert_eq!(r.gradebook, Gradebook::default());
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].0, q.op_id);
    }

    #[test]
    fn failed_apply_leaves_gradebook_untouched() {
        let fx = fixture();
        let mut gb = replay(&fx.base).gradebook;
        let before = gb.clone();
        let bad = op(1, h(9, 0), Command::RemoveTerm { id: TermId(uid()) });
        assert!(apply_op(&mut gb, &bad).is_err());
        assert_eq!(gb, before);
    }

    #[test]
    fn import_gradebook_replaces_state() {
        let fx = fixture();
        let imported = replay(&fx.base).gradebook;
        let mut ops = vec![op(
            2,
            h(1, 0),
            Command::AddTerm {
                id: TermId(uid()),
                name: "Old".into(),
            },
        )];
        ops.push(op(
            1,
            h(5, 0),
            Command::ImportGradebook {
                gradebook: imported.clone(),
            },
        ));
        assert_eq!(replay(&ops).gradebook, imported);
        // Ops after the import build on the imported state.
        ops.push(op(2, h(6, 0), upd(fx.asg, None, Some(4.0))));
        assert_eq!(score_of(&replay(&ops).gradebook, fx.asg), Some(4.0));
    }

    #[test]
    fn export_returns_state_and_is_not_a_mutation() {
        let fx = fixture();
        let mut gb = replay(&fx.base).gradebook;
        assert!(!Command::ExportGradebook.is_mutation());
        match execute(&mut gb.clone(), Command::ExportGradebook).unwrap() {
            crate::CommandOutput::Gradebook { gradebook } => assert_eq!(gradebook, gb),
            other => panic!("unexpected {other:?}"),
        }
        // Export does not change anything.
        let before = gb.clone();
        execute(&mut gb, Command::ExportGradebook).unwrap();
        assert_eq!(gb, before);
    }

    #[test]
    fn op_serde_round_trip() {
        let o = op(
            1,
            h(5, 2),
            Command::AddTerm {
                id: TermId(uid()),
                name: "x".into(),
            },
        );
        let json = serde_json::to_string(&o).unwrap();
        assert_eq!(serde_json::from_str::<Op>(&json).unwrap(), o);
    }
}
