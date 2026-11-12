//! Analysis: impact, trends and leverage. Pure functions over a course.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::grade::{Locked, current_grade, current_grade_with, final_grade_with, is_remaining};
use crate::ids::{AssignmentId, CategoryId};
use crate::model::{Assignment, AssignmentStatus, Course, CurveMode};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImpactEntry {
    pub assignment_id: AssignmentId,
    pub name: String,
    pub due: Option<NaiveDate>,
    /// Running grade before this assignment was graded; `None` for the first graded work.
    pub grade_before: Option<f64>,
    pub grade_after: Option<f64>,
    pub delta: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TrendPoint {
    /// `None` is the single trailing point for undated work.
    pub date: Option<NaiveDate>,
    pub grade: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LeverageEntry {
    pub assignment_id: AssignmentId,
    pub name: String,
    pub category_id: CategoryId,
    /// Final-grade points gained per 10% scored on this assignment.
    pub leverage: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LeverageReport {
    pub baseline_used: f64,
    pub entries: Vec<LeverageEntry>,
}

/// Same definition of "graded" as grade.rs: not excused and (scored or missing).
fn is_graded(a: &Assignment) -> bool {
    a.status != AssignmentStatus::Excused
        && (a.score.is_some() || a.status == AssignmentStatus::Missing)
}

/// Graded assignments' (category index, assignment index) in chronological order:
/// due date ascending, then list order; undated last.
fn graded_in_order(course: &Course) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = course
        .categories
        .iter()
        .enumerate()
        .flat_map(|(ci, c)| {
            c.assignments
                .iter()
                .enumerate()
                .filter(|(_, a)| is_graded(a))
                .map(move |(ai, _)| (ci, ai))
        })
        .collect();
    // Stable sort preserves list order within equal keys.
    v.sort_by_key(|&(ci, ai)| {
        let due = course.categories[ci].assignments[ai].due;
        (due.is_none(), due)
    });
    v
}

type Step = ((usize, usize), Option<f64>, Option<f64>);

/// Walk graded work chronologically, yielding the running grade after each step. A working copy
/// has all graded work excused and re-enables one assignment at a time, so drops and
/// renormalization are re-evaluated on the subset graded so far.
fn running(course: &Course, mode: CurveMode) -> Vec<Step> {
    let order = graded_in_order(course);
    let mut work = course.clone();
    let mut saved = Vec::with_capacity(order.len());
    for &(ci, ai) in &order {
        let a = &mut work.categories[ci].assignments[ai];
        saved.push(a.status);
        a.status = AssignmentStatus::Excused;
    }
    let locked = Locked::new();
    let mut before: Option<f64> = None;
    let mut out = Vec::with_capacity(order.len());
    for (&(ci, ai), status) in order.iter().zip(saved) {
        work.categories[ci].assignments[ai].status = status;
        let after = current_grade_with(&work, mode, &locked);
        out.push(((ci, ai), before, after));
        before = after;
    }
    out
}

/// Change in running grade as each graded assignment was graded, chronologically.
pub fn impact(course: &Course, mode: CurveMode) -> Vec<ImpactEntry> {
    running(course, mode)
        .into_iter()
        .map(|((ci, ai), before, after)| {
            let a = &course.categories[ci].assignments[ai];
            ImpactEntry {
                assignment_id: a.id,
                name: a.name.clone(),
                due: a.due,
                grade_before: before,
                grade_after: after,
                delta: match (before, after) {
                    (Some(b), Some(x)) => Some(x - b),
                    _ => None,
                },
            }
        })
        .collect()
}

/// Running grade over time: one point per distinct date (last value of the day), with undated
/// work folded into one trailing undated point.
pub fn trends(course: &Course, mode: CurveMode) -> Vec<TrendPoint> {
    let mut points: Vec<TrendPoint> = Vec::new();
    for ((ci, ai), _, after) in running(course, mode) {
        let Some(grade) = after else { continue };
        let date = course.categories[ci].assignments[ai].due;
        match points.last_mut() {
            Some(p) if p.date == date => p.grade = grade,
            _ => points.push(TrendPoint { date, grade }),
        }
    }
    points
}

/// Leverage of each remaining assignment, sorted descending (ties in list order). Other remaining
/// work is filled at `baseline` percent (default: current grade clamped to 0-100, or 0).
pub fn leverage(
    course: &Course,
    baseline: Option<f64>,
    mode: CurveMode,
) -> Result<LeverageReport, CoreError> {
    let baseline = match baseline {
        Some(b) => {
            if !b.is_finite() {
                return Err(CoreError::NonFinite { field: "baseline" });
            }
            if !(0.0..=100.0).contains(&b) {
                return Err(CoreError::OutOfRange { field: "baseline" });
            }
            b
        }
        // Current grade before the course add_points curve (assignment-level percent).
        None => current_grade(course, mode).map_or(0.0, |g| {
            let add = match mode {
                CurveMode::Apply => course.curve.add_points,
                CurveMode::Ignore => 0.0,
            };
            (g - add).clamp(0.0, 100.0)
        }),
    };
    let mut entries: Vec<LeverageEntry> = Vec::new();
    for cat in &course.categories {
        for a in cat.assignments.iter().filter(|a| is_remaining(a)) {
            let at = |pct: f64| {
                let mut locked = Locked::new();
                locked.insert(a.id, a.points_possible * pct / 100.0);
                final_grade_with(course, baseline, mode, &locked)
            };
            let (Some(hi), Some(lo)) = (at(100.0), at(0.0)) else {
                // Zero total weight: no final grade exists, so no leverage.
                return Ok(LeverageReport {
                    baseline_used: baseline,
                    entries: Vec::new(),
                });
            };
            let leverage = (hi - lo) / 10.0;
            entries.push(LeverageEntry {
                assignment_id: a.id,
                name: a.name.clone(),
                category_id: cat.id,
                leverage,
            });
        }
    }
    // Stable sort with an exact total order: exact ties keep list order.
    entries.sort_by(|x, y| y.leverage.total_cmp(&x.leverage));
    Ok(LeverageReport {
        baseline_used: baseline,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, CommandOutput, execute};
    use crate::ids::{CourseId, ScenarioId, TermId};
    use crate::model::{AssignmentCurve, Gradebook, Override, Scenario, Term};
    use crate::testutil::{approx, asg, cat, course, uid};

    fn d(day: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(2026, 9, day)
    }

    fn dated(possible: f64, score: Option<f64>, due: Option<NaiveDate>) -> Assignment {
        let mut a = asg(possible, score);
        a.due = due;
        a
    }

    #[test]
    fn impact_chronological_undated_last() {
        let a_undated = dated(100.0, Some(60.0), None);
        let a_late = dated(100.0, Some(80.0), d(10));
        let a_early = dated(100.0, Some(100.0), d(2));
        let c = course(vec![cat(100.0, 0, vec![a_undated, a_late, a_early])]);
        let imp = impact(&c, CurveMode::Apply);
        assert_eq!(imp.len(), 3);
        assert_eq!(imp[0].due, d(2));
        assert_eq!(imp[1].due, d(10));
        assert_eq!(imp[2].due, None);
        assert!(imp[0].grade_before.is_none() && imp[0].delta.is_none());
        approx(imp[0].grade_after.unwrap(), 100.0);
        approx(imp[1].grade_after.unwrap(), 90.0);
        approx(imp[1].delta.unwrap(), -10.0);
        approx(imp[2].grade_after.unwrap(), 80.0);
        approx(imp[2].delta.unwrap(), -10.0);
    }

    #[test]
    fn impact_ties_use_list_order_and_semantics_match_grade() {
        let a1 = dated(100.0, Some(50.0), d(1));
        let a2 = dated(100.0, Some(100.0), d(1));
        let pending = dated(100.0, None, d(1));
        let mut excused = dated(100.0, Some(0.0), d(1));
        excused.status = AssignmentStatus::Excused;
        let mut missing = dated(100.0, None, d(1));
        missing.status = AssignmentStatus::Missing;
        let c = course(vec![cat(100.0, 0, vec![a1, pending, a2, excused, missing])]);
        let imp = impact(&c, CurveMode::Apply);
        assert_eq!(imp.len(), 3);
        approx(imp[0].grade_after.unwrap(), 50.0);
        approx(imp[1].grade_after.unwrap(), 75.0);
        // Missing counts as graded 0: (50 + 100 + 0) / 300.
        approx(imp[2].grade_after.unwrap(), 50.0);
        approx(
            imp[2].grade_after.unwrap(),
            crate::grade::current_grade(&c, CurveMode::Apply).unwrap(),
        );
    }

    #[test]
    fn impact_with_drops_reevaluated_on_subset() {
        let a1 = dated(100.0, Some(50.0), d(1));
        let a2 = dated(100.0, Some(100.0), d(2));
        let a3 = dated(100.0, Some(80.0), d(3));
        let c = course(vec![cat(100.0, 1, vec![a1, a2, a3])]);
        let imp = impact(&c, CurveMode::Apply);
        approx(imp[0].grade_after.unwrap(), 50.0); // single item is kept
        approx(imp[1].grade_after.unwrap(), 100.0); // 50 dropped
        approx(imp[2].grade_after.unwrap(), 90.0); // 50 dropped, 100 and 80 kept
        approx(imp[2].delta.unwrap(), -10.0);
    }

    #[test]
    fn trends_collapse_same_day_and_fold_undated() {
        let c = course(vec![cat(
            100.0,
            0,
            vec![
                dated(100.0, Some(100.0), d(1)),
                dated(100.0, Some(50.0), d(1)),
                dated(100.0, Some(0.0), d(5)),
                dated(100.0, Some(30.0), None),
                dated(100.0, Some(30.0), None),
            ],
        )]);
        let t = trends(&c, CurveMode::Apply);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].date, d(1));
        approx(t[0].grade, 75.0);
        assert_eq!(t[1].date, d(5));
        approx(t[1].grade, 50.0);
        assert_eq!(t[2].date, None);
        approx(t[2].grade, 42.0); // 210 / 5
    }

    #[test]
    fn curve_mode_affects_analysis() {
        let mut a = dated(100.0, Some(80.0), d(1));
        a.curves = vec![AssignmentCurve::AddPoints(5.0)];
        let c = course(vec![cat(100.0, 0, vec![a])]);
        approx(impact(&c, CurveMode::Apply)[0].grade_after.unwrap(), 85.0);
        approx(impact(&c, CurveMode::Ignore)[0].grade_after.unwrap(), 80.0);
    }

    #[test]
    fn no_graded_work() {
        let c = course(vec![cat(100.0, 0, vec![asg(100.0, None)])]);
        assert!(impact(&c, CurveMode::Apply).is_empty());
        assert!(trends(&c, CurveMode::Apply).is_empty());
        let l = leverage(&c, None, CurveMode::Apply).unwrap();
        approx(l.baseline_used, 0.0);
        assert_eq!(l.entries.len(), 1);
        approx(l.entries[0].leverage, 10.0);
    }

    #[test]
    fn leverage_two_categories_hand_computed() {
        // Homework 40%: graded 80/100 plus one pending 100. Exam 60%: one pending 100.
        let hw_pending = asg(100.0, None);
        let exam = asg(100.0, None);
        let hw_id = hw_pending.id;
        let exam_id = exam.id;
        let c = course(vec![
            cat(40.0, 0, vec![asg(100.0, Some(80.0)), hw_pending]),
            cat(60.0, 0, vec![exam]),
        ]);
        let l = leverage(&c, Some(70.0), CurveMode::Apply).unwrap();
        approx(l.baseline_used, 70.0);
        // Exam: 100 points * 0.6 / 10 = 6. HW: category avg swings 100/200 = 50 points,
        // * 0.4 / 10 = 2.
        assert_eq!(l.entries[0].assignment_id, exam_id);
        approx(l.entries[0].leverage, 6.0);
        assert_eq!(l.entries[0].category_id, c.categories[1].id);
        assert_eq!(l.entries[1].assignment_id, hw_id);
        approx(l.entries[1].leverage, 2.0);
    }

    #[test]
    fn leverage_default_baseline_is_current_grade() {
        let c = course(vec![cat(
            100.0,
            0,
            vec![asg(100.0, Some(80.0)), asg(100.0, None)],
        )]);
        approx(
            leverage(&c, None, CurveMode::Apply).unwrap().baseline_used,
            80.0,
        );
    }

    #[test]
    fn leverage_ties_in_list_order_and_validation() {
        let a = asg(100.0, None);
        let b = asg(100.0, None);
        let (ia, ib) = (a.id, b.id);
        let c = course(vec![cat(100.0, 0, vec![a, b])]);
        let l = leverage(&c, Some(50.0), CurveMode::Apply).unwrap();
        assert_eq!(l.entries[0].assignment_id, ia);
        assert_eq!(l.entries[1].assignment_id, ib);
        approx(l.entries[0].leverage, 5.0);
        assert!(leverage(&c, Some(f64::NAN), CurveMode::Apply).is_err());
        assert!(leverage(&c, Some(100.5), CurveMode::Apply).is_err());
        assert!(leverage(&c, Some(-1.0), CurveMode::Apply).is_err());
        assert!(leverage(&c, Some(100.0), CurveMode::Apply).is_ok());
    }

    #[test]
    fn leverage_with_drops_is_lower_for_likely_dropped() {
        let p = asg(100.0, None);
        let c = course(vec![cat(
            100.0,
            1,
            vec![asg(100.0, Some(100.0)), asg(100.0, Some(95.0)), p],
        )]);
        let with_drop = leverage(&c, Some(90.0), CurveMode::Apply).unwrap();
        let mut nodrop = c.clone();
        nodrop.categories[0].drop_lowest = 0;
        let without = leverage(&nodrop, Some(90.0), CurveMode::Apply).unwrap();
        // No drop: 100 / 300 * 100 / 10 = 3.333. With drop: scoring 0 drops it (avg 97.5),
        // scoring 100 drops the 95 (avg 100): swing 2.5 / 10 = 0.25.
        approx(without.entries[0].leverage, 100.0 / 3.0 / 10.0);
        approx(with_drop.entries[0].leverage, 0.25);
        assert!(with_drop.entries[0].leverage < without.entries[0].leverage);
    }

    #[test]
    fn no_remaining_work_empty_leverage() {
        let c = course(vec![cat(100.0, 0, vec![asg(100.0, Some(90.0))])]);
        let l = leverage(&c, None, CurveMode::Apply).unwrap();
        assert!(l.entries.is_empty());
        approx(l.baseline_used, 90.0);
        // Zero-possible assignments are never remaining.
        let c = course(vec![cat(100.0, 0, vec![asg(0.0, None)])]);
        assert!(
            leverage(&c, None, CurveMode::Apply)
                .unwrap()
                .entries
                .is_empty()
        );
    }

    #[test]
    fn leverage_default_baseline_ignores_course_add_points() {
        let mut c = course(vec![cat(
            100.0,
            0,
            vec![asg(100.0, Some(80.0)), asg(100.0, None)],
        )]);
        c.curve.add_points = 5.0;
        let apply = leverage(&c, None, CurveMode::Apply).unwrap();
        let ignore = leverage(&c, None, CurveMode::Ignore).unwrap();
        approx(apply.baseline_used, 80.0);
        approx(ignore.baseline_used, 80.0);
        approx(apply.entries[0].leverage, ignore.entries[0].leverage);
    }

    #[test]
    fn leverage_zero_weight_course_is_empty() {
        let c = course(vec![cat(0.0, 0, vec![asg(100.0, None)])]);
        let l = leverage(&c, Some(50.0), CurveMode::Apply).unwrap();
        assert!(l.entries.is_empty());
        approx(l.baseline_used, 50.0);
        let l = leverage(&c, None, CurveMode::Apply).unwrap();
        assert!(l.entries.is_empty());
        approx(l.baseline_used, 0.0);
    }

    #[test]
    fn impact_missing_in_middle_of_chain() {
        let mut missing = dated(100.0, None, d(2));
        missing.status = AssignmentStatus::Missing;
        let c = course(vec![cat(
            100.0,
            0,
            vec![
                dated(100.0, Some(100.0), d(1)),
                missing,
                dated(100.0, Some(80.0), d(3)),
            ],
        )]);
        let imp = impact(&c, CurveMode::Apply);
        assert_eq!(imp.len(), 3);
        approx(imp[0].grade_after.unwrap(), 100.0);
        approx(imp[1].grade_after.unwrap(), 50.0);
        approx(imp[1].delta.unwrap(), -50.0);
        approx(imp[2].grade_after.unwrap(), 60.0);
        approx(imp[2].delta.unwrap(), 10.0);
    }

    #[test]
    fn course_add_points_in_impact_and_trends() {
        let mut c = course(vec![cat(
            100.0,
            0,
            vec![
                dated(100.0, Some(80.0), d(1)),
                dated(100.0, Some(60.0), d(2)),
            ],
        )]);
        c.curve.add_points = 5.0;
        let ia = impact(&c, CurveMode::Apply);
        let ii = impact(&c, CurveMode::Ignore);
        approx(ia[0].grade_after.unwrap(), 85.0);
        approx(ia[1].grade_after.unwrap(), 75.0);
        approx(ia[1].delta.unwrap(), -10.0);
        approx(ii[0].grade_after.unwrap(), 80.0);
        approx(ii[1].grade_after.unwrap(), 70.0);
        let ta = trends(&c, CurveMode::Apply);
        let ti = trends(&c, CurveMode::Ignore);
        approx(ta[1].grade, 75.0);
        approx(ti[1].grade, 70.0);
    }

    #[test]
    fn graded_status_without_score_is_remaining_not_impact() {
        let mut g = asg(100.0, None);
        g.status = AssignmentStatus::Graded;
        let gid = g.id;
        let c = course(vec![cat(100.0, 0, vec![asg(100.0, Some(80.0)), g])]);
        let imp = impact(&c, CurveMode::Apply);
        assert_eq!(imp.len(), 1);
        assert!(imp.iter().all(|e| e.assignment_id != gid));
        let l = leverage(&c, None, CurveMode::Apply).unwrap();
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.entries[0].assignment_id, gid);
    }

    #[test]
    fn scenario_command_does_not_mutate() {
        let p = asg(100.0, None);
        let pid = p.id;
        let c = course(vec![cat(100.0, 0, vec![asg(100.0, Some(80.0)), p])]);
        let cid = c.id;
        let mut term = Term::new(TermId(uid()), "T");
        term.courses.push(c);
        let sid = ScenarioId(uid());
        let mut gb = Gradebook::default();
        gb.terms.push(term);
        gb.scenarios.push(Scenario {
            id: sid,
            name: "s".into(),
            overrides: vec![Override::SetScore {
                assignment_id: pid,
                score: Some(100.0),
            }],
        });
        let before = gb.clone();
        let out = execute(
            &mut gb,
            Command::Impact {
                course_id: cid,
                curve_mode: CurveMode::Apply,
                scenario_id: Some(sid),
            },
        )
        .unwrap();
        let CommandOutput::Impact { entries } = out else {
            panic!("wrong output")
        };
        assert_eq!(entries.len(), 2);
        approx(entries[1].grade_after.unwrap(), 90.0);
        assert_eq!(gb, before);
        // Without the scenario the pending assignment is not graded.
        let out = execute(
            &mut gb,
            Command::Impact {
                course_id: cid,
                curve_mode: CurveMode::Apply,
                scenario_id: None,
            },
        )
        .unwrap();
        let CommandOutput::Impact { entries } = out else {
            panic!("wrong output")
        };
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn command_serde_round_trip() {
        let cid = CourseId(uid());
        let cmds = vec![
            Command::Impact {
                course_id: cid,
                curve_mode: CurveMode::Ignore,
                scenario_id: Some(ScenarioId(uid())),
            },
            Command::Trends {
                course_id: cid,
                curve_mode: CurveMode::Apply,
                scenario_id: None,
            },
            Command::Leverage {
                course_id: cid,
                baseline: Some(75.0),
                curve_mode: CurveMode::Apply,
                scenario_id: None,
            },
        ];
        for c in cmds {
            let back: Command = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
            assert_eq!(c, back);
        }
        let minimal: Command = serde_json::from_str(&format!(
            r#"{{"command":"leverage","course_id":"{}"}}"#,
            cid.0
        ))
        .unwrap();
        assert!(matches!(
            minimal,
            Command::Leverage {
                baseline: None,
                curve_mode: CurveMode::Apply,
                scenario_id: None,
                ..
            }
        ));
    }
}
