//! What-if scenarios: named overlays on a gradebook. Real data is never mutated.

use std::borrow::Cow;
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::command::{curves, finite, points};
use crate::error::CoreError;
use crate::gpa::{cumulative_gpa, term_gpa};
use crate::grade::{course_letter, current_grade, projected_grade};
use crate::ids::{AssignmentId, CategoryId, CourseId, ScenarioId, TermId};
use crate::model::{Category, Course, CurveMode, Gradebook, Letter, Override, Scenario, Term};

/// Clone the gradebook and overlay the scenario's overrides in order.
pub fn apply_scenario(gb: &Gradebook, scenario: &Scenario) -> Result<Gradebook, CoreError> {
    let mut out = gb.clone();
    for o in &scenario.overrides {
        apply_override(&mut out, o)?;
    }
    Ok(out)
}

/// Ids of entities removed from the real gradebook.
#[derive(Debug, Clone, Default)]
pub struct Removed {
    pub courses: HashSet<CourseId>,
    pub categories: HashSet<CategoryId>,
    pub assignments: HashSet<AssignmentId>,
}

impl Removed {
    pub fn from_category(cat: &Category) -> Self {
        let mut r = Self::default();
        r.add_category(cat);
        r
    }

    pub fn from_course(course: &Course) -> Self {
        let mut r = Self::default();
        r.add_course(course);
        r
    }

    pub fn from_term(term: &Term) -> Self {
        let mut r = Self::default();
        for c in &term.courses {
            r.add_course(c);
        }
        r
    }

    pub fn from_assignment(id: AssignmentId) -> Self {
        let mut r = Self::default();
        r.assignments.insert(id);
        r
    }

    fn add_course(&mut self, course: &Course) {
        self.courses.insert(course.id);
        for cat in &course.categories {
            self.add_category(cat);
        }
    }

    fn add_category(&mut self, cat: &Category) {
        self.categories.insert(cat.id);
        self.assignments
            .extend(cat.assignments.iter().map(|a| a.id));
    }
}

/// Drop every override that targets a removed entity or anything beneath it, including
/// hypothetical assignments added into a removed category (and overrides on those).
pub fn prune_overrides(scenarios: &mut [Scenario], removed: &Removed) {
    for sc in scenarios {
        let mut gone = removed.assignments.clone();
        sc.overrides.retain(|o| match o {
            Override::SetScore { assignment_id, .. }
            | Override::SetStatus { assignment_id, .. }
            | Override::SetAssignmentCurves { assignment_id, .. } => !gone.contains(assignment_id),
            Override::AddAssignment {
                category_id,
                assignment,
            } => {
                if removed.categories.contains(category_id) {
                    gone.insert(assignment.id);
                    false
                } else {
                    true
                }
            }
            Override::SetCourseCurve { course_id, .. }
            | Override::SetGpaStatus { course_id, .. }
            | Override::SetOfficialLetter { course_id, .. } => !removed.courses.contains(course_id),
        });
    }
}

/// True if any scenario's hypothetical assignment uses this id.
pub fn hypothetical_id_in_use(gb: &Gradebook, id: AssignmentId) -> bool {
    gb.scenarios.iter().any(|s| {
        s.overrides
            .iter()
            .any(|o| matches!(o, Override::AddAssignment { assignment, .. } if assignment.id == id))
    })
}

fn apply_override(gb: &mut Gradebook, o: &Override) -> Result<(), CoreError> {
    match o {
        Override::SetScore {
            assignment_id,
            score,
        } => {
            if let Some(s) = score {
                points("score", *s)?;
            }
            gb.assignment_mut(*assignment_id)
                .ok_or(CoreError::UnknownAssignment(assignment_id.0))?
                .score = *score;
        }
        Override::SetStatus {
            assignment_id,
            status,
        } => {
            gb.assignment_mut(*assignment_id)
                .ok_or(CoreError::UnknownAssignment(assignment_id.0))?
                .status = *status;
        }
        Override::AddAssignment {
            category_id,
            assignment,
        } => {
            points("points_possible", assignment.points_possible)?;
            if let Some(s) = assignment.score {
                points("score", s)?;
            }
            curves(&assignment.curves)?;
            if gb.assignment(assignment.id).is_some() {
                return Err(CoreError::DuplicateId {
                    kind: "assignment",
                    id: assignment.id.0,
                });
            }
            gb.category_mut(*category_id)
                .ok_or(CoreError::UnknownCategory(category_id.0))?
                .assignments
                .push(assignment.clone());
        }
        Override::SetAssignmentCurves {
            assignment_id,
            curves: list,
        } => {
            curves(list)?;
            gb.assignment_mut(*assignment_id)
                .ok_or(CoreError::UnknownAssignment(assignment_id.0))?
                .curves = list.clone();
        }
        Override::SetCourseCurve { course_id, curve } => {
            finite("add_points", curve.add_points)?;
            finite("cutoff_shift", curve.cutoff_shift)?;
            course_mut(gb, *course_id)?.curve = *curve;
        }
        Override::SetGpaStatus {
            course_id,
            gpa_status,
        } => course_mut(gb, *course_id)?.gpa_status = *gpa_status,
        Override::SetOfficialLetter { course_id, letter } => {
            course_mut(gb, *course_id)?.official_letter = *letter;
        }
    }
    Ok(())
}

fn course_mut(gb: &mut Gradebook, id: CourseId) -> Result<&mut crate::model::Course, CoreError> {
    gb.course_mut(id).ok_or(CoreError::UnknownCourse(id.0))
}

pub fn find_scenario(gb: &Gradebook, id: ScenarioId) -> Result<&Scenario, CoreError> {
    gb.scenarios
        .iter()
        .find(|s| s.id == id)
        .ok_or(CoreError::UnknownScenario(id.0))
}

/// The gradebook as seen through an optional scenario (borrowed when there is none).
pub fn view(
    gb: &Gradebook,
    scenario_id: Option<ScenarioId>,
) -> Result<Cow<'_, Gradebook>, CoreError> {
    match scenario_id {
        None => Ok(Cow::Borrowed(gb)),
        Some(id) => apply_scenario(gb, find_scenario(gb, id)?).map(Cow::Owned),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompareCourse {
    pub course_id: CourseId,
    pub name: String,
    pub current: Option<f64>,
    pub current_letter: Option<Letter>,
    pub projected: Option<f64>,
    pub projected_letter: Option<Letter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompareTerm {
    pub term_id: TermId,
    pub name: String,
    pub gpa: Option<f64>,
}

/// One column of a comparison: the real data (`scenario_id: None`) or a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioColumn {
    pub scenario_id: Option<ScenarioId>,
    /// "real" for the baseline, else the scenario name.
    pub name: String,
    pub courses: Vec<CompareCourse>,
    pub terms: Vec<CompareTerm>,
    pub cumulative_gpa: Option<f64>,
    /// Set when the scenario failed to apply; the result fields are then empty/None.
    #[serde(default)]
    pub error: Option<String>,
}

fn column(gb: &Gradebook, scenario_id: Option<ScenarioId>, name: &str) -> ScenarioColumn {
    let mode = CurveMode::Apply;
    let courses = gb
        .terms
        .iter()
        .flat_map(|t| &t.courses)
        .map(|c| {
            let current = current_grade(c, mode);
            let projected = projected_grade(c, mode);
            CompareCourse {
                course_id: c.id,
                name: c.name.clone(),
                current,
                current_letter: current.map(|g| course_letter(c, g, mode)),
                projected,
                projected_letter: projected.map(|g| course_letter(c, g, mode)),
            }
        })
        .collect();
    ScenarioColumn {
        scenario_id,
        name: name.to_string(),
        courses,
        terms: gb
            .terms
            .iter()
            .map(|t| {
                let r = term_gpa(t, &gb.scale);
                CompareTerm {
                    term_id: t.id,
                    name: t.name.clone(),
                    gpa: r.gpa,
                }
            })
            .collect(),
        cumulative_gpa: cumulative_gpa(gb).gpa,
        error: None,
    }
}

fn error_column(s: &Scenario, e: &CoreError) -> ScenarioColumn {
    ScenarioColumn {
        scenario_id: Some(s.id),
        name: s.name.clone(),
        courses: Vec::new(),
        terms: Vec::new(),
        cumulative_gpa: None,
        error: Some(e.to_string()),
    }
}

/// Baseline plus one column per requested scenario (empty list = all, in stored order).
pub fn compare(gb: &Gradebook, ids: &[ScenarioId]) -> Result<Vec<ScenarioColumn>, CoreError> {
    let selected: Vec<&Scenario> = if ids.is_empty() {
        gb.scenarios.iter().collect()
    } else {
        ids.iter()
            .map(|id| find_scenario(gb, *id))
            .collect::<Result<_, _>>()?
    };
    let mut cols = vec![column(gb, None, "real")];
    for s in selected {
        cols.push(match apply_scenario(gb, s) {
            Ok(view) => column(&view, Some(s.id), &s.name),
            Err(e) => error_column(s, &e),
        });
    }
    Ok(cols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{AssignmentId, CategoryId};
    use crate::model::{
        Assignment, AssignmentCurve, AssignmentStatus, CourseCurve, GpaStatus, Term,
    };
    use crate::testutil::*;

    struct Fx {
        gb: Gradebook,
        course: CourseId,
        cat: CategoryId,
        graded: AssignmentId,
        pending: AssignmentId,
    }

    fn fx() -> Fx {
        let g = asg(100.0, Some(90.0));
        let p = asg(100.0, None);
        let (gid, pid) = (g.id, p.id);
        let c = course(vec![cat(100.0, 0, vec![g, p])]);
        let (cid, kid) = (c.id, c.categories[0].id);
        let mut t = Term::new(TermId(uid()), "Fall");
        t.courses = vec![c];
        Fx {
            gb: Gradebook {
                terms: vec![t],
                ..Default::default()
            },
            course: cid,
            cat: kid,
            graded: gid,
            pending: pid,
        }
    }

    fn scen(overrides: Vec<Override>) -> Scenario {
        Scenario {
            id: ScenarioId(uid()),
            name: "s".into(),
            overrides,
        }
    }

    #[test]
    fn apply_overlays_without_mutating() {
        let f = fx();
        let before = f.gb.clone();
        let s = scen(vec![
            Override::SetScore {
                assignment_id: f.pending,
                score: Some(50.0),
            },
            Override::SetAssignmentCurves {
                assignment_id: f.graded,
                curves: vec![AssignmentCurve::AddPoints(5.0)],
            },
            Override::SetCourseCurve {
                course_id: f.course,
                curve: CourseCurve {
                    add_points: 2.0,
                    cutoff_shift: 0.0,
                },
            },
        ]);
        let out = apply_scenario(&f.gb, &s).unwrap();
        assert_eq!(f.gb, before);
        assert_eq!(out.assignment(f.pending).unwrap().score, Some(50.0));
        assert_eq!(out.assignment(f.graded).unwrap().score, Some(90.0));
        let c = out.course(f.course).unwrap();
        approx(current_grade(c, CurveMode::Apply).unwrap(), 72.5 + 2.0);
    }

    #[test]
    fn add_assignment_status_and_gpa_overrides() {
        let f = fx();
        let extra = Assignment::new(AssignmentId(uid()), "hyp", 50.0);
        let eid = extra.id;
        let s = scen(vec![
            Override::AddAssignment {
                category_id: f.cat,
                assignment: extra,
            },
            Override::SetScore {
                assignment_id: eid,
                score: Some(50.0),
            },
            Override::SetStatus {
                assignment_id: f.pending,
                status: AssignmentStatus::Excused,
            },
            Override::SetGpaStatus {
                course_id: f.course,
                gpa_status: GpaStatus::PassFail,
            },
            Override::SetOfficialLetter {
                course_id: f.course,
                letter: Some(Letter::B),
            },
        ]);
        let out = apply_scenario(&f.gb, &s).unwrap();
        assert!(f.gb.assignment(eid).is_none());
        assert!(out.assignment(eid).is_some());
        let c = out.course(f.course).unwrap();
        approx(
            current_grade(c, CurveMode::Apply).unwrap(),
            140.0 / 150.0 * 100.0,
        );
        assert_eq!(c.gpa_status, GpaStatus::PassFail);
        assert_eq!(c.official_letter, Some(Letter::B));
    }

    #[test]
    fn missing_ids_error() {
        let f = fx();
        let bad = AssignmentId(uid());
        assert!(matches!(
            apply_scenario(
                &f.gb,
                &scen(vec![Override::SetScore {
                    assignment_id: bad,
                    score: Some(1.0)
                }])
            ),
            Err(CoreError::UnknownAssignment(_))
        ));
        assert!(matches!(
            apply_scenario(
                &f.gb,
                &scen(vec![Override::SetCourseCurve {
                    course_id: CourseId(uid()),
                    curve: CourseCurve::default()
                }])
            ),
            Err(CoreError::UnknownCourse(_))
        ));
        assert!(matches!(
            apply_scenario(
                &f.gb,
                &scen(vec![Override::AddAssignment {
                    category_id: CategoryId(uid()),
                    assignment: Assignment::new(AssignmentId(uid()), "x", 1.0)
                }])
            ),
            Err(CoreError::UnknownCategory(_))
        ));
        // Duplicate hypothetical id.
        assert!(matches!(
            apply_scenario(
                &f.gb,
                &scen(vec![Override::AddAssignment {
                    category_id: f.cat,
                    assignment: Assignment::new(f.graded, "dup", 1.0)
                }])
            ),
            Err(CoreError::DuplicateId { .. })
        ));
    }

    #[test]
    fn compare_shows_differing_gpa_and_leaves_real_untouched() {
        let mut f = fx();
        let s = scen(vec![Override::SetScore {
            assignment_id: f.pending,
            score: Some(100.0),
        }]);
        let sid = s.id;
        f.gb.scenarios.push(s);
        let before = f.gb.clone();
        let cols = compare(&f.gb, &[]).unwrap();
        assert_eq!(f.gb, before);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].name, "real");
        assert_eq!(cols[0].scenario_id, None);
        assert_eq!(cols[1].scenario_id, Some(sid));
        // Real: 90 -> A- (3.7); scenario: 95 -> A (4.0).
        approx(cols[0].cumulative_gpa.unwrap(), 3.7);
        approx(cols[1].cumulative_gpa.unwrap(), 4.0);
        approx(cols[1].courses[0].current.unwrap(), 95.0);
        approx(cols[0].terms[0].gpa.unwrap(), 3.7);
        // Explicit selection and unknown id.
        assert_eq!(compare(&f.gb, &[sid]).unwrap().len(), 2);
        assert!(matches!(
            compare(&f.gb, &[ScenarioId(uid())]),
            Err(CoreError::UnknownScenario(_))
        ));
    }

    #[test]
    fn view_borrows_without_scenario() {
        let f = fx();
        assert!(matches!(view(&f.gb, None).unwrap(), Cow::Borrowed(_)));
        assert!(view(&f.gb, Some(ScenarioId(uid()))).is_err());
    }

    #[test]
    fn compare_survives_broken_scenario() {
        let mut f = fx();
        let good = scen(vec![Override::SetScore {
            assignment_id: f.pending,
            score: Some(100.0),
        }]);
        let bad = scen(vec![Override::SetScore {
            assignment_id: AssignmentId(uid()),
            score: Some(1.0),
        }]);
        let bad_id = bad.id;
        f.gb.scenarios.push(bad);
        f.gb.scenarios.push(good);
        let cols = compare(&f.gb, &[]).unwrap();
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0].error, None);
        approx(cols[0].cumulative_gpa.unwrap(), 3.7);
        assert_eq!(cols[1].scenario_id, Some(bad_id));
        assert!(cols[1].error.is_some());
        assert!(cols[1].courses.is_empty() && cols[1].cumulative_gpa.is_none());
        assert_eq!(cols[2].error, None);
        approx(cols[2].cumulative_gpa.unwrap(), 4.0);
    }

    #[test]
    fn prune_removes_nested_and_hypothetical_overrides() {
        let f = fx();
        let extra = Assignment::new(AssignmentId(uid()), "hyp", 10.0);
        let eid = extra.id;
        let mut s = scen(vec![
            Override::AddAssignment {
                category_id: f.cat,
                assignment: extra,
            },
            Override::SetScore {
                assignment_id: eid,
                score: Some(5.0),
            },
            Override::SetScore {
                assignment_id: f.pending,
                score: Some(1.0),
            },
            Override::SetCourseCurve {
                course_id: f.course,
                curve: CourseCurve::default(),
            },
        ]);
        s.overrides.push(Override::SetGpaStatus {
            course_id: CourseId(uid()),
            gpa_status: GpaStatus::PassFail,
        });
        let mut list = vec![s];
        let removed = Removed::from_course(f.gb.course(f.course).unwrap());
        prune_overrides(&mut list, &removed);
        assert_eq!(list[0].overrides.len(), 1);
        assert!(matches!(
            list[0].overrides[0],
            Override::SetGpaStatus { .. }
        ));
    }
}
