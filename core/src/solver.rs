//! Target solver: the uniform score on remaining work needed to reach a target grade.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::grade::{EPS, Locked, final_grade_with, remaining_assignments};
use crate::ids::AssignmentId;
use crate::model::{AssignmentStatus, Course, CurveMode, Letter};

/// Upper bound of the search (percent of points possible; allows hopeless-but-finite answers).
const SEARCH_MAX: f64 = 1000.0;
const SEARCH_TOL: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SolveStatus {
    /// Target is met even if all remaining work scores 0.
    Secured,
    /// Reachable: every remaining assignment needs `needed` percent (0-100).
    OnTrack { needed: f64 },
    /// Needs more than 100% (`Some`, up to the search cap).
    ///
    /// `needed: None` covers two cases: there is no remaining work to solve for, or the target
    /// is unreachable even at the search cap (1000%). Callers distinguish them via
    /// `SolveResult::remaining_count` (0 means no remaining work).
    Unreachable { needed: Option<f64> },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolveResult {
    pub status: SolveStatus,
    pub target_percent: f64,
    /// Ungraded, non-excused, non-locked assignments with points possible > 0.
    pub remaining_count: usize,
    /// Final grade when remaining work scores `needed` (Secured: grade at 0%).
    pub grade_at_needed: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSpec {
    Percent(f64),
    Letter(Letter),
}

impl std::str::FromStr for TargetSpec {
    type Err = CoreError;

    /// A percent ("90", "90.5", "90%") or a letter ("A-", "b+", case-insensitive).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        let bad = || CoreError::InvalidTarget(s.to_string());
        let number = t.strip_suffix('%').unwrap_or(t).trim();
        if let Ok(p) = number.parse::<f64>() {
            return if p.is_finite() {
                Ok(TargetSpec::Percent(p))
            } else {
                Err(bad())
            };
        }
        if t.ends_with('%') {
            return Err(bad());
        }
        t.parse::<Letter>()
            .map(TargetSpec::Letter)
            .map_err(|_| bad())
    }
}

/// A locked expected score (raw points, before curves).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LockedScore {
    pub assignment_id: AssignmentId,
    pub points: f64,
}

/// Resolve a target spec to a percent. A letter uses the course's cutoff, shifted by the course
/// curve only when `mode` is `Apply`.
pub fn resolve_target(
    course: &Course,
    spec: TargetSpec,
    mode: CurveMode,
) -> Result<f64, CoreError> {
    match spec {
        TargetSpec::Percent(p) => {
            crate::command::finite("target", p)?;
            Ok(p)
        }
        TargetSpec::Letter(l) => {
            let cutoff = course
                .cutoffs
                .get(l)
                .ok_or(CoreError::NoCutoff(l.as_str()))?;
            let shift = match mode {
                CurveMode::Apply => course.curve.cutoff_shift,
                CurveMode::Ignore => 0.0,
            };
            Ok(cutoff - shift)
        }
    }
}

/// Validate locked scores against a course and build the lookup map.
pub fn build_locked(course: &Course, list: &[LockedScore]) -> Result<Locked, CoreError> {
    let mut locked = Locked::new();
    for l in list {
        crate::command::points("locked points", l.points)?;
        let a = course
            .categories
            .iter()
            .flat_map(|c| &c.assignments)
            .find(|a| a.id == l.assignment_id)
            .ok_or(CoreError::UnknownAssignment(l.assignment_id.0))?;
        if a.status == AssignmentStatus::Excused {
            return Err(CoreError::LockedExcused(a.id.0));
        }
        locked.insert(l.assignment_id, l.points);
    }
    Ok(locked)
}

/// Solve for the uniform percent on remaining work that hits `target_pct`.
///
/// Grade is monotonic in the fill, so a binary search on [0, 1000] to 1e-6 is exact enough.
pub fn solve(course: &Course, target_pct: f64, locked: &Locked, mode: CurveMode) -> SolveResult {
    let remaining_count = remaining_assignments(course, locked).len();
    let grade = |fill: f64| final_grade_with(course, fill, mode, locked);
    let result = |status, grade_at_needed| SolveResult {
        status,
        target_percent: target_pct,
        remaining_count,
        grade_at_needed,
    };
    let meets = |g: Option<f64>| g.is_some_and(|g| g >= target_pct - EPS);

    let g0 = grade(0.0);
    if meets(g0) {
        return result(SolveStatus::Secured, g0);
    }
    if remaining_count == 0 || !meets(grade(SEARCH_MAX)) {
        return result(SolveStatus::Unreachable { needed: None }, None);
    }
    let (mut lo, mut hi) = (0.0_f64, SEARCH_MAX);
    for _ in 0..200 {
        if hi - lo <= SEARCH_TOL {
            break;
        }
        let mid = (lo + hi) / 2.0;
        if meets(grade(mid)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    if meets(grade(100.0)) {
        // `hi` may overshoot a hair past an exactly-achievable 100.
        let needed = hi.min(100.0);
        result(SolveStatus::OnTrack { needed }, grade(needed))
    } else {
        result(SolveStatus::Unreachable { needed: Some(hi) }, grade(hi))
    }
}

/// Resolve target and locks, then solve. `with_curve` switches the curve mode to `Apply`.
pub fn solve_for_course(
    course: &Course,
    target: Option<TargetSpec>,
    locked: &[LockedScore],
    with_curve: bool,
) -> Result<SolveResult, CoreError> {
    let mode = if with_curve {
        CurveMode::Apply
    } else {
        CurveMode::Ignore
    };
    let spec = match target {
        Some(t) => t,
        None => TargetSpec::Percent(course.target.ok_or(CoreError::NoTarget)?),
    };
    let pct = resolve_target(course, spec, mode)?;
    let locked = build_locked(course, locked)?;
    Ok(solve(course, pct, &locked, mode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AssignmentCurve, CourseCurve};
    use crate::testutil::*;

    fn simple() -> Course {
        // 50 of 50 possible graded at 80%, 50 remaining.
        course(vec![cat(
            100.0,
            0,
            vec![asg(50.0, Some(40.0)), asg(50.0, None)],
        )])
    }

    fn needed(r: &SolveResult) -> f64 {
        match r.status {
            SolveStatus::OnTrack { needed } => needed,
            s => panic!("not on track: {s:?}"),
        }
    }

    #[test]
    fn target_spec_from_str() {
        let p = |s: &str| s.parse::<TargetSpec>();
        assert_eq!(p("90"), Ok(TargetSpec::Percent(90.0)));
        assert_eq!(p(" 90.5 "), Ok(TargetSpec::Percent(90.5)));
        assert_eq!(p("90%"), Ok(TargetSpec::Percent(90.0)));
        assert_eq!(p("A-"), Ok(TargetSpec::Letter(Letter::AMinus)));
        assert_eq!(p("b+"), Ok(TargetSpec::Letter(Letter::BPlus)));
        assert_eq!(p("f"), Ok(TargetSpec::Letter(Letter::F)));
        for bad in ["", "x", "%", "A%", "nan", "inf", "E"] {
            assert!(
                matches!(p(bad), Err(CoreError::InvalidTarget(_))),
                "{bad:?}"
            );
        }
        assert!(matches!(
            "G".parse::<Letter>(),
            Err(CoreError::InvalidLetter(_))
        ));
    }

    #[test]
    fn secured() {
        let r = solve(&simple(), 40.0, &Locked::new(), CurveMode::Ignore);
        assert_eq!(r.status, SolveStatus::Secured);
        assert_eq!(r.remaining_count, 1);
        approx(r.grade_at_needed.unwrap(), 40.0);
    }

    #[test]
    fn on_track() {
        let r = solve(&simple(), 70.0, &Locked::new(), CurveMode::Ignore);
        assert!((needed(&r) - 60.0).abs() < 1e-5);
        assert!((r.grade_at_needed.unwrap() - 70.0).abs() < 1e-5);
    }

    #[test]
    fn exactly_achievable() {
        // 40 earned + 50 possible remaining: exactly 90 needs 100%.
        let r = solve(&simple(), 90.0, &Locked::new(), CurveMode::Ignore);
        approx(needed(&r), 100.0);
    }

    #[test]
    fn unreachable_needs_more_than_100() {
        let r = solve(&simple(), 95.0, &Locked::new(), CurveMode::Ignore);
        let SolveStatus::Unreachable { needed: Some(n) } = r.status else {
            panic!("{r:?}")
        };
        assert!(n > 100.0 && (n - 110.0).abs() < 1e-4);
    }

    #[test]
    fn no_remaining_work() {
        let c = course(vec![cat(100.0, 0, vec![asg(100.0, Some(80.0))])]);
        let r = solve(&c, 90.0, &Locked::new(), CurveMode::Ignore);
        assert_eq!(r.status, SolveStatus::Unreachable { needed: None });
        assert_eq!(r.remaining_count, 0);
        assert_eq!(
            solve(&c, 80.0, &Locked::new(), CurveMode::Ignore).status,
            SolveStatus::Secured
        );
    }

    #[test]
    fn unreachable_even_at_cap() {
        let c = course(vec![
            cat(1.0, 0, vec![asg(10.0, None)]),
            cat(99.0, 0, vec![asg(10.0, Some(0.0))]),
        ]);
        let r = solve(&c, 100.0, &Locked::new(), CurveMode::Ignore);
        assert_eq!(r.status, SolveStatus::Unreachable { needed: None });
    }

    #[test]
    fn locked_scores_change_result() {
        let a = asg(50.0, None);
        let aid = a.id;
        let c = course(vec![cat(100.0, 0, vec![a, asg(50.0, None)])]);
        let free = solve(&c, 80.0, &Locked::new(), CurveMode::Ignore);
        assert!((needed(&free) - 80.0).abs() < 1e-5);
        let mut locked = Locked::new();
        locked.insert(aid, 50.0);
        let r = solve(&c, 80.0, &locked, CurveMode::Ignore);
        assert!((needed(&r) - 60.0).abs() < 1e-5);
        assert_eq!(r.remaining_count, 1);
    }

    #[test]
    fn curve_toggle() {
        let mut c = simple();
        c.curve = CourseCurve {
            add_points: 10.0,
            cutoff_shift: 0.0,
        };
        let without = solve(&c, 90.0, &Locked::new(), CurveMode::Ignore);
        let with = solve(&c, 90.0, &Locked::new(), CurveMode::Apply);
        assert!((needed(&without) - 100.0).abs() < 1e-5);
        assert!((needed(&with) - 80.0).abs() < 1e-5);
    }

    #[test]
    fn assignment_curve_toggle() {
        let mut a = asg(100.0, None);
        a.curves = vec![AssignmentCurve::AddPoints(10.0)];
        let c = course(vec![cat(100.0, 0, vec![a])]);
        let without = solve(&c, 90.0, &Locked::new(), CurveMode::Ignore);
        let with = solve(&c, 90.0, &Locked::new(), CurveMode::Apply);
        assert!((needed(&without) - 90.0).abs() < 1e-5);
        assert!((needed(&with) - 80.0).abs() < 1e-5);
    }

    #[test]
    fn with_drops() {
        // Drop 1 of {50 graded, remaining}: one item is kept, so the better of the two counts.
        let c = course(vec![cat(
            100.0,
            1,
            vec![asg(100.0, Some(50.0)), asg(100.0, None)],
        )]);
        let r = solve(&c, 80.0, &Locked::new(), CurveMode::Ignore);
        assert!((needed(&r) - 80.0).abs() < 1e-5);
        let r = solve(&c, 40.0, &Locked::new(), CurveMode::Ignore);
        assert_eq!(r.status, SolveStatus::Secured);
    }

    #[test]
    fn letter_target_uses_cutoffs() {
        let mut c = simple();
        c.curve.cutoff_shift = 3.0;
        approx(
            resolve_target(&c, TargetSpec::Letter(Letter::AMinus), CurveMode::Ignore).unwrap(),
            90.0,
        );
        approx(
            resolve_target(&c, TargetSpec::Letter(Letter::AMinus), CurveMode::Apply).unwrap(),
            87.0,
        );
        assert_eq!(
            resolve_target(&c, TargetSpec::Letter(Letter::APlus), CurveMode::Apply),
            Err(CoreError::NoCutoff("A+"))
        );
    }

    #[test]
    fn locking_unknown_or_excused_errors() {
        let mut ex = asg(10.0, None);
        ex.status = AssignmentStatus::Excused;
        let exid = ex.id;
        let c = course(vec![cat(100.0, 0, vec![ex])]);
        let bad = LockedScore {
            assignment_id: AssignmentId(uid()),
            points: 1.0,
        };
        assert!(matches!(
            build_locked(&c, &[bad]),
            Err(CoreError::UnknownAssignment(_))
        ));
        let l = LockedScore {
            assignment_id: exid,
            points: 1.0,
        };
        assert_eq!(
            build_locked(&c, &[l]),
            Err(CoreError::LockedExcused(exid.0))
        );
    }

    #[test]
    fn no_target_errors() {
        assert_eq!(
            solve_for_course(&simple(), None, &[], false),
            Err(CoreError::NoTarget)
        );
    }

    #[test]
    fn status_serializes_tagged() {
        let j = serde_json::to_value(SolveStatus::OnTrack { needed: 50.0 }).unwrap();
        assert_eq!(j["state"], "on_track");
    }
}
