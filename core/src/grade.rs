//! Pure grade math. Percentages are f64 in 0.0-100.0.

use std::collections::HashMap;

use crate::ids::AssignmentId;
use crate::model::{
    Assignment, AssignmentCurve, AssignmentStatus, Category, Course, CurveMode, Letter,
    LetterCutoffs,
};

/// Epsilon for letter-cutoff and weight comparisons.
pub const EPS: f64 = 1e-9;

/// Locked expected scores (raw points, before curves) overriding assignments' stored scores.
pub type Locked = HashMap<AssignmentId, f64>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Points {
    pub earned: f64,
    pub possible: f64,
}

#[derive(Clone, Copy)]
struct Ctx<'a> {
    mode: CurveMode,
    locked: &'a Locked,
    /// `Some(pct)`: remaining work is filled with pct percent of points possible.
    fill: Option<f64>,
}

/// Apply assignment curves in list order.
pub fn apply_curves(score: f64, possible: f64, curves: &[AssignmentCurve]) -> Points {
    let (mut earned, mut possible) = (score, possible);
    for curve in curves {
        match *curve {
            AssignmentCurve::AddPoints(n) => earned += n,
            AssignmentCurve::ScaleTopTo { top_score } => {
                if top_score > 0.0 && possible > 0.0 {
                    earned = earned * possible / top_score;
                }
            }
            AssignmentCurve::SetMax(new_max) => {
                if new_max >= 0.0 {
                    possible = new_max;
                }
            }
        }
    }
    Points { earned, possible }
}

/// True if the assignment is unscored work that still counts as "remaining":
/// not excused, not missing, no score, and points possible > 0.
pub fn is_remaining(a: &Assignment) -> bool {
    a.status != AssignmentStatus::Excused
        && a.status != AssignmentStatus::Missing
        && a.score.is_none()
        && a.points_possible > 0.0
}

/// Remaining assignments of a course (in list order), excluding any that are locked.
pub fn remaining_assignments<'a>(course: &'a Course, locked: &Locked) -> Vec<&'a Assignment> {
    course
        .categories
        .iter()
        .flat_map(|c| &c.assignments)
        .filter(|a| is_remaining(a) && !locked.contains_key(&a.id))
        .collect()
}

/// Points for an assignment under the context, or `None` if it is ignored.
fn assignment_points(a: &Assignment, ctx: Ctx) -> Option<Points> {
    if a.status == AssignmentStatus::Excused {
        return None;
    }
    let raw = if let Some(s) = ctx.locked.get(&a.id) {
        Some(*s)
    } else if let Some(s) = a.score {
        Some(s)
    } else if a.status == AssignmentStatus::Missing {
        Some(0.0)
    } else if let (Some(f), true) = (ctx.fill, a.points_possible > 0.0) {
        Some(f / 100.0 * a.points_possible)
    } else {
        None
    }?;
    Some(match ctx.mode {
        CurveMode::Apply => apply_curves(raw, a.points_possible, &a.curves),
        CurveMode::Ignore => Points {
            earned: raw,
            possible: a.points_possible,
        },
    })
}

/// Flags which items are kept after dropping the lowest `drop_n`.
///
/// Drops the set of `drop_n` items that maximizes the kept average (Dinkelbach iteration).
/// Items with `possible <= 0` are never dropped; at least one droppable item is kept.
/// Ties keep earlier items.
pub fn kept_after_drops(items: &[Points], drop_n: u32) -> Vec<bool> {
    let mut kept = vec![true; items.len()];
    let droppable: Vec<usize> = (0..items.len())
        .filter(|&i| items[i].possible > 0.0)
        .collect();
    let drops = (drop_n as usize).min(droppable.len().saturating_sub(1));
    if drops == 0 {
        return kept;
    }
    let keep_m = droppable.len() - drops;
    let (fixed_e, fixed_p) = items
        .iter()
        .filter(|p| p.possible <= 0.0)
        .fold((0.0, 0.0), |(e, p), x| (e + x.earned, p + x.possible));
    let ratio = |set: &[usize]| {
        let (e, p) = set.iter().fold((fixed_e, fixed_p), |(e, p), &i| {
            (e + items[i].earned, p + items[i].possible)
        });
        e / p
    };

    // Start from the first `keep_m` droppable items (favours earlier items on ties).
    let mut set: Vec<usize> = droppable[..keep_m].to_vec();
    let mut lambda = ratio(&set);
    for _ in 0..100 {
        let mut order = droppable.clone();
        // Stable sort: equal values keep list order.
        order.sort_by(|&a, &b| {
            let va = items[a].earned - lambda * items[a].possible;
            let vb = items[b].earned - lambda * items[b].possible;
            vb.total_cmp(&va)
        });
        let mut next: Vec<usize> = order[..keep_m].to_vec();
        next.sort_unstable();
        let next_lambda = ratio(&next);
        if next_lambda > lambda + 1e-12 {
            set = next;
            lambda = next_lambda;
        } else {
            break;
        }
    }

    for &i in &droppable {
        kept[i] = set.contains(&i);
    }
    kept
}

fn category_average_ctx(cat: &Category, ctx: Ctx) -> Option<f64> {
    let items: Vec<Points> = cat
        .assignments
        .iter()
        .filter_map(|a| assignment_points(a, ctx))
        .collect();
    let kept = kept_after_drops(&items, cat.drop_lowest);
    let (earned, possible) = items
        .iter()
        .zip(&kept)
        .filter(|(_, k)| **k)
        .fold((0.0, 0.0), |(e, p), (x, _)| (e + x.earned, p + x.possible));
    (possible > EPS).then(|| earned / possible * 100.0)
}

/// Points-based category average (percent) over graded work, after curves and drops.
/// `None` if there is no graded work with points possible.
pub fn category_average(cat: &Category, mode: CurveMode) -> Option<f64> {
    category_average_ctx(
        cat,
        Ctx {
            mode,
            locked: &Locked::new(),
            fill: None,
        },
    )
}

fn course_add_points(course: &Course, mode: CurveMode) -> f64 {
    match mode {
        CurveMode::Apply => course.curve.add_points,
        CurveMode::Ignore => 0.0,
    }
}

fn current_ctx(course: &Course, ctx: Ctx) -> Option<f64> {
    let (mut sum, mut weight) = (0.0, 0.0);
    for cat in course.categories.iter().filter(|c| c.weight > 0.0) {
        if let Some(avg) = category_average_ctx(cat, ctx) {
            sum += avg * cat.weight;
            weight += cat.weight;
        }
    }
    (weight > 0.0).then(|| sum / weight + course_add_points(course, ctx.mode))
}

/// Current grade: weighted average of category averages, renormalized over categories with
/// graded work. `None` if there is no graded work.
pub fn current_grade(course: &Course, mode: CurveMode) -> Option<f64> {
    current_grade_with(course, mode, &Locked::new())
}

/// Like [`current_grade`] with some assignment scores overridden (locked).
pub fn current_grade_with(course: &Course, mode: CurveMode, locked: &Locked) -> Option<f64> {
    current_ctx(
        course,
        Ctx {
            mode,
            locked,
            fill: None,
        },
    )
}

/// Final grade at full weights, with remaining work filled at `fill` percent of points possible.
/// Categories without any work count as 0. `None` if total weight is zero.
pub fn final_grade(course: &Course, fill: f64, mode: CurveMode) -> Option<f64> {
    final_grade_with(course, fill, mode, &Locked::new())
}

/// Like [`final_grade`] with some assignment scores overridden (locked).
pub fn final_grade_with(
    course: &Course,
    fill: f64,
    mode: CurveMode,
    locked: &Locked,
) -> Option<f64> {
    let ctx = Ctx {
        mode,
        locked,
        fill: Some(fill),
    };
    let total: f64 = course
        .categories
        .iter()
        .map(|c| c.weight)
        .filter(|w| *w > 0.0)
        .sum();
    if total <= 0.0 {
        return None;
    }
    let sum: f64 = course
        .categories
        .iter()
        .filter(|c| c.weight > 0.0)
        .map(|c| category_average_ctx(c, ctx).unwrap_or(0.0) * c.weight)
        .sum();
    Some(sum / total + course_add_points(course, mode))
}

/// Projected grade: full weights, remaining work counted as 0 (the floor "if you stop now").
pub fn projected_grade(course: &Course, mode: CurveMode) -> Option<f64> {
    final_grade(course, 0.0, mode)
}

/// Letter for `grade`: highest letter whose cutoff (minus `shift`) is <= grade (epsilon 1e-9).
pub fn letter_for(grade: f64, cutoffs: &LetterCutoffs, shift: f64) -> Letter {
    Letter::ALL
        .into_iter()
        .find(|l| cutoffs.get(*l).is_some_and(|c| grade + EPS >= c - shift))
        .unwrap_or(Letter::F)
}

/// Letter for an already-computed course grade, honoring the course curve's cutoff shift.
pub fn course_letter(course: &Course, grade: f64, mode: CurveMode) -> Letter {
    let shift = match mode {
        CurveMode::Apply => course.curve.cutoff_shift,
        CurveMode::Ignore => 0.0,
    };
    letter_for(grade, &course.cutoffs, shift)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CourseCurve;
    use crate::testutil::*;

    const A: AssignmentStatus = AssignmentStatus::Pending;

    fn graded(possible: f64, score: f64) -> Assignment {
        asg(possible, Some(score))
    }

    #[test]
    fn no_graded_work() {
        let c = course(vec![
            cat(50.0, 0, vec![asg(100.0, None)]),
            cat(50.0, 0, vec![]),
        ]);
        assert_eq!(current_grade(&c, CurveMode::Apply), None);
        approx(projected_grade(&c, CurveMode::Apply).unwrap(), 0.0);
    }

    #[test]
    fn all_categories_empty() {
        let c = course(vec![cat(60.0, 0, vec![]), cat(40.0, 0, vec![])]);
        assert_eq!(current_grade(&c, CurveMode::Apply), None);
        approx(projected_grade(&c, CurveMode::Apply).unwrap(), 0.0);
        let none = course(vec![]);
        assert_eq!(current_grade(&none, CurveMode::Apply), None);
        assert_eq!(projected_grade(&none, CurveMode::Apply), None);
    }

    #[test]
    fn some_categories_empty_renormalize() {
        let c = course(vec![
            cat(50.0, 0, vec![graded(100.0, 80.0)]),
            cat(30.0, 0, vec![]),
            cat(20.0, 0, vec![graded(100.0, 60.0)]),
        ]);
        approx(
            current_grade(&c, CurveMode::Apply).unwrap(),
            (80.0 * 50.0 + 60.0 * 20.0) / 70.0,
        );
        approx(projected_grade(&c, CurveMode::Apply).unwrap(), 52.0);
    }

    #[test]
    fn drop_fewer_than_n_keeps_one() {
        let c = course(vec![cat(100.0, 3, vec![graded(100.0, 70.0)])]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 70.0);
    }

    #[test]
    fn drop_exactly_n_keeps_best_one() {
        let c = course(vec![cat(
            100.0,
            2,
            vec![graded(100.0, 50.0), graded(100.0, 90.0)],
        )]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 90.0);
    }

    #[test]
    fn drop_more_assignments_than_n() {
        let c = course(vec![cat(
            100.0,
            1,
            vec![
                graded(100.0, 100.0),
                graded(100.0, 50.0),
                graded(100.0, 60.0),
            ],
        )]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 80.0);
    }

    #[test]
    fn drop_maximizes_average_not_lowest_percent() {
        // 0/1 (0%), 40/100 (40%), 100/100: dropping the 40% item beats dropping the 0% item.
        let items = [
            Points {
                earned: 0.0,
                possible: 1.0,
            },
            Points {
                earned: 40.0,
                possible: 100.0,
            },
            Points {
                earned: 100.0,
                possible: 100.0,
            },
        ];
        assert_eq!(kept_after_drops(&items, 1), vec![true, false, true]);
        let c = course(vec![cat(
            100.0,
            1,
            vec![graded(1.0, 0.0), graded(100.0, 40.0), graded(100.0, 100.0)],
        )]);
        approx(
            current_grade(&c, CurveMode::Apply).unwrap(),
            100.0 / 101.0 * 100.0,
        );
    }

    #[test]
    fn drop_ties_keep_earlier() {
        let items = [
            Points {
                earned: 50.0,
                possible: 100.0,
            },
            Points {
                earned: 50.0,
                possible: 100.0,
            },
        ];
        assert_eq!(kept_after_drops(&items, 1), vec![true, false]);
    }

    #[test]
    fn zero_possible_never_dropped() {
        let items = [
            Points {
                earned: 5.0,
                possible: 0.0,
            },
            Points {
                earned: 50.0,
                possible: 100.0,
            },
            Points {
                earned: 100.0,
                possible: 100.0,
            },
        ];
        assert_eq!(kept_after_drops(&items, 1), vec![true, false, true]);
        let c = course(vec![cat(
            100.0,
            1,
            vec![
                asg(0.0, Some(5.0)),
                graded(50.0 * 2.0, 50.0),
                graded(100.0, 100.0),
            ],
        )]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 105.0);
    }

    #[test]
    fn ungraded_work_is_not_dropped_or_counted_in_current() {
        let c = course(vec![cat(
            100.0,
            1,
            vec![graded(100.0, 50.0), asg(100.0, None)],
        )]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 50.0);
    }

    #[test]
    fn assignment_curve_add_points() {
        let mut a = graded(100.0, 80.0);
        a.curves = vec![AssignmentCurve::AddPoints(5.0)];
        let c = course(vec![cat(100.0, 0, vec![a])]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 85.0);
        approx(current_grade(&c, CurveMode::Ignore).unwrap(), 80.0);
    }

    #[test]
    fn assignment_curve_scale_top() {
        let mut a = graded(100.0, 72.0);
        a.curves = vec![AssignmentCurve::ScaleTopTo { top_score: 90.0 }];
        let c = course(vec![cat(100.0, 0, vec![a])]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 80.0);
    }

    #[test]
    fn assignment_curve_set_max() {
        let mut a = graded(100.0, 72.0);
        a.curves = vec![AssignmentCurve::SetMax(80.0)];
        let c = course(vec![cat(100.0, 0, vec![a])]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 90.0);
    }

    #[test]
    fn assignment_curves_combined_in_order() {
        let p = apply_curves(
            72.0,
            100.0,
            &[
                AssignmentCurve::AddPoints(8.0),
                AssignmentCurve::SetMax(90.0),
            ],
        );
        approx(p.earned / p.possible * 100.0, 80.0 / 90.0 * 100.0);
        let p = apply_curves(
            64.0,
            100.0,
            &[
                AssignmentCurve::ScaleTopTo { top_score: 80.0 },
                AssignmentCurve::AddPoints(5.0),
            ],
        );
        approx(p.earned, 85.0);
        approx(p.possible, 100.0);
    }

    #[test]
    fn curves_do_not_mutate_stored_scores() {
        let mut a = graded(100.0, 72.0);
        a.curves = vec![
            AssignmentCurve::AddPoints(5.0),
            AssignmentCurve::SetMax(90.0),
        ];
        let mut c = course(vec![cat(100.0, 0, vec![a])]);
        c.curve = CourseCurve {
            add_points: 2.0,
            cutoff_shift: 3.0,
        };
        let before = c.clone();
        let _ = current_grade(&c, CurveMode::Apply);
        let _ = final_grade(&c, 50.0, CurveMode::Apply);
        assert_eq!(c, before);
    }

    #[test]
    fn course_curve_add_points_and_cutoff_shift() {
        let mut c = course(vec![cat(100.0, 0, vec![graded(100.0, 88.0)])]);
        c.curve = CourseCurve {
            add_points: 2.0,
            cutoff_shift: 0.0,
        };
        let g = current_grade(&c, CurveMode::Apply).unwrap();
        approx(g, 90.0);
        assert_eq!(course_letter(&c, g, CurveMode::Apply), Letter::AMinus);
        let g = current_grade(&c, CurveMode::Ignore).unwrap();
        approx(g, 88.0);
        assert_eq!(course_letter(&c, g, CurveMode::Ignore), Letter::BPlus);

        c.curve = CourseCurve {
            add_points: 0.0,
            cutoff_shift: 5.0,
        };
        assert_eq!(course_letter(&c, 88.0, CurveMode::Apply), Letter::A);
        assert_eq!(course_letter(&c, 88.0, CurveMode::Ignore), Letter::BPlus);
    }

    #[test]
    fn letter_epsilon_and_fallback() {
        let cut = LetterCutoffs::default();
        assert_eq!(letter_for(90.0 - 1e-12, &cut, 0.0), Letter::AMinus);
        assert_eq!(letter_for(89.9, &cut, 0.0), Letter::BPlus);
        assert_eq!(letter_for(120.0, &cut, 0.0), Letter::A);
        assert_eq!(letter_for(-5.0, &cut, 0.0), Letter::F);
    }

    #[test]
    fn missing_counts_zero_excused_ignored() {
        let mut missing = asg(100.0, None);
        missing.status = AssignmentStatus::Missing;
        let c = course(vec![cat(100.0, 0, vec![graded(100.0, 100.0), missing])]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 50.0);

        let mut ex = asg(100.0, None);
        ex.status = AssignmentStatus::Excused;
        let mut ex_scored = asg(100.0, Some(0.0));
        ex_scored.status = AssignmentStatus::Excused;
        let c = course(vec![cat(
            100.0,
            0,
            vec![graded(100.0, 100.0), ex.clone(), ex_scored],
        )]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 100.0);
        assert!(!is_remaining(&ex));
    }

    #[test]
    fn extra_credit() {
        let c = course(vec![cat(100.0, 0, vec![graded(100.0, 110.0)])]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 110.0);
        let c = course(vec![cat(
            100.0,
            0,
            vec![graded(100.0, 100.0), asg(0.0, Some(5.0))],
        )]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 105.0);
    }

    #[test]
    fn final_grade_fill_and_locked() {
        let c = course(vec![cat(
            100.0,
            0,
            vec![graded(100.0, 50.0), asg(100.0, None)],
        )]);
        approx(final_grade(&c, 0.0, CurveMode::Apply).unwrap(), 25.0);
        approx(final_grade(&c, 50.0, CurveMode::Apply).unwrap(), 50.0);
        approx(final_grade(&c, 100.0, CurveMode::Apply).unwrap(), 75.0);

        let rem = remaining_assignments(&c, &Locked::new());
        assert_eq!(rem.len(), 1);
        let mut locked = Locked::new();
        locked.insert(rem[0].id, 100.0);
        assert!(remaining_assignments(&c, &locked).is_empty());
        approx(
            current_grade_with(&c, CurveMode::Apply, &locked).unwrap(),
            75.0,
        );
        approx(
            final_grade_with(&c, 0.0, CurveMode::Apply, &locked).unwrap(),
            75.0,
        );
        // Stored data untouched.
        assert!(c.categories[0].assignments[1].score.is_none());
    }

    #[test]
    fn zero_remaining_possible_is_not_remaining() {
        let a = asg(0.0, None);
        assert!(!is_remaining(&a));
        assert_eq!(a.status, A);
    }

    #[test]
    fn zero_or_empty_weights_yield_none() {
        let c = course(vec![cat(0.0, 0, vec![graded(100.0, 80.0)])]);
        assert_eq!(current_grade(&c, CurveMode::Apply), None);
        assert_eq!(projected_grade(&c, CurveMode::Apply), None);
    }

    #[test]
    fn pending_with_score_counts_as_graded() {
        let c = course(vec![cat(100.0, 0, vec![graded(100.0, 70.0)])]);
        approx(current_grade(&c, CurveMode::Apply).unwrap(), 70.0);
    }

    #[test]
    fn drop_equal_ratio_items_of_different_possible() {
        // 50/100, 25/50, 100/100 drop 1: dropping the 50/100 leaves 125/150 (83.3%),
        // better than dropping 25/50 (150/200 = 75%) or 100/100 (75/150 = 50%).
        let items = [
            Points {
                earned: 50.0,
                possible: 100.0,
            },
            Points {
                earned: 25.0,
                possible: 50.0,
            },
            Points {
                earned: 100.0,
                possible: 100.0,
            },
        ];
        assert_eq!(kept_after_drops(&items, 1), vec![false, true, true]);
    }

    #[test]
    fn fill_is_percent_of_original_possible_under_set_max() {
        let mut a = asg(100.0, None);
        a.curves = vec![AssignmentCurve::SetMax(80.0)];
        let c = course(vec![cat(100.0, 0, vec![a])]);
        approx(final_grade(&c, 100.0, CurveMode::Apply).unwrap(), 125.0);
        approx(final_grade(&c, 100.0, CurveMode::Ignore).unwrap(), 100.0);
    }

    #[test]
    fn course_add_points_with_no_graded_work() {
        let mut c = course(vec![cat(100.0, 0, vec![asg(100.0, None)])]);
        c.curve = CourseCurve {
            add_points: 5.0,
            cutoff_shift: 0.0,
        };
        approx(projected_grade(&c, CurveMode::Apply).unwrap(), 5.0);
        approx(projected_grade(&c, CurveMode::Ignore).unwrap(), 0.0);
    }
}
