//! Typed Commands: the single entry point for every capability (CLI/MCP/UI are thin adapters).

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::CoreError;
use crate::gpa::{CumulativeGpaReport, TermGpaReport};
use crate::grade;
use crate::ids::{AssignmentId, CategoryId, CourseId, ScenarioId, TermId};
use crate::model::{
    Assignment, AssignmentCurve, AssignmentStatus, Category, Course, CourseCurve, CurveMode,
    GpaScale, GpaStatus, Gradebook, Letter, LetterCutoffs, Override, Scenario, Term,
};
use crate::scenario::{self, ScenarioColumn};
use crate::solver::{self, LockedScore, SolveResult, TargetSpec};

/// Distinguishes "field absent" (None) from "set to null" (Some(None)).
fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    AddTerm {
        id: TermId,
        name: String,
    },
    RenameTerm {
        id: TermId,
        name: String,
    },
    RemoveTerm {
        id: TermId,
    },
    AddCourse {
        term_id: TermId,
        id: CourseId,
        name: String,
        #[serde(default)]
        code: Option<String>,
        credits: f64,
        #[serde(default)]
        target: Option<f64>,
    },
    /// Fields left out are unchanged; clearable fields accept `null` to clear.
    UpdateCourse {
        id: CourseId,
        #[serde(default)]
        name: Option<String>,
        #[serde(
            default,
            deserialize_with = "double_option",
            skip_serializing_if = "Option::is_none"
        )]
        code: Option<Option<String>>,
        #[serde(default)]
        credits: Option<f64>,
        #[serde(
            default,
            deserialize_with = "double_option",
            skip_serializing_if = "Option::is_none"
        )]
        target: Option<Option<f64>>,
        #[serde(default)]
        gpa_status: Option<GpaStatus>,
        #[serde(
            default,
            deserialize_with = "double_option",
            skip_serializing_if = "Option::is_none"
        )]
        official_letter: Option<Option<Letter>>,
    },
    RemoveCourse {
        id: CourseId,
    },
    AddCategory {
        course_id: CourseId,
        id: CategoryId,
        name: String,
        weight: f64,
        #[serde(default)]
        drop_lowest: u32,
    },
    UpdateCategory {
        id: CategoryId,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        weight: Option<f64>,
        #[serde(default)]
        drop_lowest: Option<u32>,
    },
    RemoveCategory {
        id: CategoryId,
    },
    AddAssignment {
        category_id: CategoryId,
        id: AssignmentId,
        name: String,
        points_possible: f64,
        #[serde(default)]
        score: Option<f64>,
        #[serde(default)]
        due: Option<NaiveDate>,
        #[serde(default)]
        status: Option<AssignmentStatus>,
        #[serde(default)]
        curves: Vec<AssignmentCurve>,
    },
    UpdateAssignment {
        id: AssignmentId,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        points_possible: Option<f64>,
        #[serde(
            default,
            deserialize_with = "double_option",
            skip_serializing_if = "Option::is_none"
        )]
        score: Option<Option<f64>>,
        #[serde(
            default,
            deserialize_with = "double_option",
            skip_serializing_if = "Option::is_none"
        )]
        due: Option<Option<NaiveDate>>,
        #[serde(default)]
        status: Option<AssignmentStatus>,
        #[serde(default)]
        curves: Option<Vec<AssignmentCurve>>,
    },
    RemoveAssignment {
        id: AssignmentId,
    },
    SetCourseCurve {
        course_id: CourseId,
        curve: CourseCurve,
    },
    SetCutoffs {
        course_id: CourseId,
        cutoffs: LetterCutoffs,
    },
    ListTerms,
    ShowCourse {
        course_id: CourseId,
    },
    CourseGrade {
        course_id: CourseId,
        #[serde(default)]
        curve_mode: CurveMode,
        /// Evaluate against this what-if scenario instead of the real data.
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    SetTarget {
        course_id: CourseId,
        target: Option<f64>,
    },
    /// `target: None` uses the course's stored target. Curves are ignored unless `with_curve`.
    SolveTarget {
        course_id: CourseId,
        #[serde(default)]
        target: Option<TargetSpec>,
        #[serde(default)]
        locked: Vec<LockedScore>,
        #[serde(default)]
        with_curve: bool,
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    CreateScenario {
        id: ScenarioId,
        name: String,
    },
    RenameScenario {
        id: ScenarioId,
        name: String,
    },
    DeleteScenario {
        id: ScenarioId,
    },
    AddOverride {
        scenario_id: ScenarioId,
        #[serde(rename = "override")]
        overlay: Override,
    },
    RemoveOverride {
        scenario_id: ScenarioId,
        index: usize,
    },
    ListScenarios,
    /// Real data plus the given scenarios (empty = all) side by side.
    CompareScenarios {
        #[serde(default)]
        scenario_ids: Vec<ScenarioId>,
    },
    TermGpa {
        term_id: TermId,
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    CumulativeGpa {
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    SetGpaScale {
        scale: GpaScale,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CourseSummary {
    pub id: CourseId,
    pub name: String,
    pub code: Option<String>,
    pub credits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TermSummary {
    pub id: TermId,
    pub name: String,
    pub courses: Vec<CourseSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CourseGradeReport {
    pub course_id: CourseId,
    pub curve_mode: CurveMode,
    /// Renormalized over categories with graded work; `None` if no graded work.
    pub current: Option<f64>,
    pub current_letter: Option<Letter>,
    /// Full weights, remaining work as 0.
    pub projected: Option<f64>,
    pub projected_letter: Option<Letter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum CommandOutput {
    Done,
    Terms { terms: Vec<TermSummary> },
    Course { course: Box<Course> },
    CourseGrade { report: CourseGradeReport },
    Solve { solve: SolveResult },
    Scenarios { scenarios: Vec<Scenario> },
    Comparison { columns: Vec<ScenarioColumn> },
    TermGpa { report: TermGpaReport },
    CumulativeGpa { report: CumulativeGpaReport },
}

pub(crate) fn finite(field: &'static str, v: f64) -> Result<(), CoreError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(CoreError::NonFinite { field })
    }
}

pub(crate) fn points(field: &'static str, v: f64) -> Result<(), CoreError> {
    finite(field, v)?;
    if v < 0.0 {
        return Err(CoreError::NegativePoints { field });
    }
    Ok(())
}

fn weight(v: f64) -> Result<(), CoreError> {
    finite("weight", v)?;
    if v < 0.0 {
        return Err(CoreError::NegativeWeight);
    }
    Ok(())
}

fn credits(v: f64) -> Result<(), CoreError> {
    finite("credits", v)?;
    if v < 0.0 {
        return Err(CoreError::NegativeCredits);
    }
    Ok(())
}

pub(crate) fn curves(list: &[AssignmentCurve]) -> Result<(), CoreError> {
    for c in list {
        match *c {
            AssignmentCurve::AddPoints(n) => finite("curve points", n)?,
            AssignmentCurve::ScaleTopTo { top_score } => {
                finite("top_score", top_score)?;
                if top_score <= 0.0 {
                    return Err(CoreError::InvalidCurve {
                        reason: "top_score must be positive",
                    });
                }
            }
            AssignmentCurve::SetMax(m) => {
                finite("new_max", m)?;
                if m < 0.0 {
                    return Err(CoreError::InvalidCurve {
                        reason: "new_max must not be negative",
                    });
                }
            }
        }
    }
    Ok(())
}

fn scenario_mut(gb: &mut Gradebook, id: ScenarioId) -> Result<&mut Scenario, CoreError> {
    gb.scenarios
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or(CoreError::UnknownScenario(id.0))
}

fn dup(kind: &'static str, id: uuid::Uuid) -> CoreError {
    CoreError::DuplicateId { kind, id }
}

/// Execute a Command against the gradebook.
pub fn execute(gb: &mut Gradebook, cmd: Command) -> Result<CommandOutput, CoreError> {
    match cmd {
        Command::AddTerm { id, name } => {
            if gb.term(id).is_some() {
                return Err(dup("term", id.0));
            }
            gb.terms.push(Term::new(id, name));
            Ok(CommandOutput::Done)
        }
        Command::RenameTerm { id, name } => {
            gb.term_mut(id).ok_or(CoreError::UnknownTerm(id.0))?.name = name;
            Ok(CommandOutput::Done)
        }
        Command::RemoveTerm { id } => {
            let i = gb
                .terms
                .iter()
                .position(|t| t.id == id)
                .ok_or(CoreError::UnknownTerm(id.0))?;
            let removed = scenario::Removed::from_term(&gb.terms[i]);
            gb.terms.remove(i);
            scenario::prune_overrides(&mut gb.scenarios, &removed);
            Ok(CommandOutput::Done)
        }
        Command::AddCourse {
            term_id,
            id,
            name,
            code,
            credits: cr,
            target,
        } => {
            credits(cr)?;
            if let Some(t) = target {
                finite("target", t)?;
            }
            if gb.course(id).is_some() {
                return Err(dup("course", id.0));
            }
            let term = gb
                .term_mut(term_id)
                .ok_or(CoreError::UnknownTerm(term_id.0))?;
            let mut course = Course::new(id, name, cr);
            course.code = code;
            course.target = target;
            term.courses.push(course);
            Ok(CommandOutput::Done)
        }
        Command::UpdateCourse {
            id,
            name,
            code,
            credits: cr,
            target,
            gpa_status,
            official_letter,
        } => {
            if let Some(c) = cr {
                credits(c)?;
            }
            if let Some(Some(t)) = target {
                finite("target", t)?;
            }
            let course = gb.course_mut(id).ok_or(CoreError::UnknownCourse(id.0))?;
            if let Some(v) = name {
                course.name = v;
            }
            if let Some(v) = code {
                course.code = v;
            }
            if let Some(v) = cr {
                course.credits = v;
            }
            if let Some(v) = target {
                course.target = v;
            }
            if let Some(v) = gpa_status {
                course.gpa_status = v;
            }
            if let Some(v) = official_letter {
                course.official_letter = v;
            }
            Ok(CommandOutput::Done)
        }
        Command::RemoveCourse { id } => {
            let found = gb.terms.iter_mut().find_map(|term| {
                let i = term.courses.iter().position(|c| c.id == id)?;
                Some(term.courses.remove(i))
            });
            let course = found.ok_or(CoreError::UnknownCourse(id.0))?;
            let removed = scenario::Removed::from_course(&course);
            scenario::prune_overrides(&mut gb.scenarios, &removed);
            Ok(CommandOutput::Done)
        }
        Command::AddCategory {
            course_id,
            id,
            name,
            weight: w,
            drop_lowest,
        } => {
            weight(w)?;
            if gb.category(id).is_some() {
                return Err(dup("category", id.0));
            }
            let course = gb
                .course_mut(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            let mut cat = Category::new(id, name, w);
            cat.drop_lowest = drop_lowest;
            course.categories.push(cat);
            Ok(CommandOutput::Done)
        }
        Command::UpdateCategory {
            id,
            name,
            weight: w,
            drop_lowest,
        } => {
            if let Some(w) = w {
                weight(w)?;
            }
            let cat = gb
                .category_mut(id)
                .ok_or(CoreError::UnknownCategory(id.0))?;
            if let Some(v) = name {
                cat.name = v;
            }
            if let Some(v) = w {
                cat.weight = v;
            }
            if let Some(v) = drop_lowest {
                cat.drop_lowest = v;
            }
            Ok(CommandOutput::Done)
        }
        Command::RemoveCategory { id } => {
            let found = gb
                .terms
                .iter_mut()
                .flat_map(|t| t.courses.iter_mut())
                .find_map(|course| {
                    let i = course.categories.iter().position(|c| c.id == id)?;
                    Some(course.categories.remove(i))
                });
            let cat = found.ok_or(CoreError::UnknownCategory(id.0))?;
            let removed = scenario::Removed::from_category(&cat);
            scenario::prune_overrides(&mut gb.scenarios, &removed);
            Ok(CommandOutput::Done)
        }
        Command::AddAssignment {
            category_id,
            id,
            name,
            points_possible,
            score,
            due,
            status,
            curves: cv,
        } => {
            points("points_possible", points_possible)?;
            if let Some(s) = score {
                points("score", s)?;
            }
            curves(&cv)?;
            if gb.assignment(id).is_some() || scenario::hypothetical_id_in_use(gb, id) {
                return Err(dup("assignment", id.0));
            }
            let cat = gb
                .category_mut(category_id)
                .ok_or(CoreError::UnknownCategory(category_id.0))?;
            let mut a = Assignment::new(id, name, points_possible);
            a.score = score;
            a.due = due;
            a.status = status.unwrap_or_default();
            a.curves = cv;
            cat.assignments.push(a);
            Ok(CommandOutput::Done)
        }
        Command::UpdateAssignment {
            id,
            name,
            points_possible,
            score,
            due,
            status,
            curves: cv,
        } => {
            if let Some(p) = points_possible {
                points("points_possible", p)?;
            }
            if let Some(Some(s)) = score {
                points("score", s)?;
            }
            if let Some(list) = &cv {
                curves(list)?;
            }
            let a = gb
                .assignment_mut(id)
                .ok_or(CoreError::UnknownAssignment(id.0))?;
            if let Some(v) = name {
                a.name = v;
            }
            if let Some(v) = points_possible {
                a.points_possible = v;
            }
            if let Some(v) = score {
                a.score = v;
            }
            if let Some(v) = due {
                a.due = v;
            }
            if let Some(v) = status {
                a.status = v;
            }
            if let Some(v) = cv {
                a.curves = v;
            }
            Ok(CommandOutput::Done)
        }
        Command::RemoveAssignment { id } => {
            let found = gb
                .terms
                .iter_mut()
                .flat_map(|t| t.courses.iter_mut())
                .flat_map(|c| c.categories.iter_mut())
                .find_map(|cat| {
                    let i = cat.assignments.iter().position(|a| a.id == id)?;
                    Some(cat.assignments.remove(i))
                });
            found.ok_or(CoreError::UnknownAssignment(id.0))?;
            let removed = scenario::Removed::from_assignment(id);
            scenario::prune_overrides(&mut gb.scenarios, &removed);
            Ok(CommandOutput::Done)
        }
        Command::SetCourseCurve { course_id, curve } => {
            finite("add_points", curve.add_points)?;
            finite("cutoff_shift", curve.cutoff_shift)?;
            gb.course_mut(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?
                .curve = curve;
            Ok(CommandOutput::Done)
        }
        Command::SetCutoffs { course_id, cutoffs } => {
            for (_, v) in &cutoffs.entries {
                finite("cutoff", *v)?;
            }
            gb.course_mut(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?
                .cutoffs = cutoffs;
            Ok(CommandOutput::Done)
        }
        Command::ListTerms => Ok(CommandOutput::Terms {
            terms: gb
                .terms
                .iter()
                .map(|t| TermSummary {
                    id: t.id,
                    name: t.name.clone(),
                    courses: t
                        .courses
                        .iter()
                        .map(|c| CourseSummary {
                            id: c.id,
                            name: c.name.clone(),
                            code: c.code.clone(),
                            credits: c.credits,
                        })
                        .collect(),
                })
                .collect(),
        }),
        Command::ShowCourse { course_id } => {
            let course = gb
                .course(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            Ok(CommandOutput::Course {
                course: Box::new(course.clone()),
            })
        }
        Command::CourseGrade {
            course_id,
            curve_mode,
            scenario_id,
        } => {
            let view = scenario::view(gb, scenario_id)?;
            let course = view
                .course(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            let current = grade::current_grade(course, curve_mode);
            let projected = grade::projected_grade(course, curve_mode);
            Ok(CommandOutput::CourseGrade {
                report: CourseGradeReport {
                    course_id,
                    curve_mode,
                    current,
                    current_letter: current.map(|g| grade::course_letter(course, g, curve_mode)),
                    projected,
                    projected_letter: projected
                        .map(|g| grade::course_letter(course, g, curve_mode)),
                },
            })
        }
        Command::SetTarget { course_id, target } => {
            if let Some(t) = target {
                finite("target", t)?;
            }
            gb.course_mut(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?
                .target = target;
            Ok(CommandOutput::Done)
        }
        Command::SolveTarget {
            course_id,
            target,
            locked,
            with_curve,
            scenario_id,
        } => {
            let view = scenario::view(gb, scenario_id)?;
            let course = view
                .course(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            let result = solver::solve_for_course(course, target, &locked, with_curve)?;
            Ok(CommandOutput::Solve { solve: result })
        }
        Command::CreateScenario { id, name } => {
            if gb.scenarios.iter().any(|s| s.id == id) {
                return Err(dup("scenario", id.0));
            }
            gb.scenarios.push(Scenario {
                id,
                name,
                overrides: Vec::new(),
            });
            Ok(CommandOutput::Done)
        }
        Command::RenameScenario { id, name } => {
            scenario_mut(gb, id)?.name = name;
            Ok(CommandOutput::Done)
        }
        Command::DeleteScenario { id } => {
            let i = gb
                .scenarios
                .iter()
                .position(|s| s.id == id)
                .ok_or(CoreError::UnknownScenario(id.0))?;
            gb.scenarios.remove(i);
            Ok(CommandOutput::Done)
        }
        Command::AddOverride {
            scenario_id,
            overlay,
        } => {
            // Validate against a candidate so bad ids/values never get stored.
            let mut candidate = scenario::find_scenario(gb, scenario_id)?.clone();
            candidate.overrides.push(overlay.clone());
            scenario::apply_scenario(gb, &candidate)?;
            scenario_mut(gb, scenario_id)?.overrides.push(overlay);
            Ok(CommandOutput::Done)
        }
        Command::RemoveOverride { scenario_id, index } => {
            let s = scenario_mut(gb, scenario_id)?;
            if index >= s.overrides.len() {
                return Err(CoreError::OverrideIndex {
                    index,
                    len: s.overrides.len(),
                });
            }
            s.overrides.remove(index);
            Ok(CommandOutput::Done)
        }
        Command::ListScenarios => Ok(CommandOutput::Scenarios {
            scenarios: gb.scenarios.clone(),
        }),
        Command::CompareScenarios { scenario_ids } => Ok(CommandOutput::Comparison {
            columns: scenario::compare(gb, &scenario_ids)?,
        }),
        Command::TermGpa {
            term_id,
            scenario_id,
        } => {
            let view = scenario::view(gb, scenario_id)?;
            let term = view
                .term(term_id)
                .ok_or(CoreError::UnknownTerm(term_id.0))?;
            Ok(CommandOutput::TermGpa {
                report: crate::gpa::term_gpa(term, &view.scale),
            })
        }
        Command::CumulativeGpa { scenario_id } => {
            let view = scenario::view(gb, scenario_id)?;
            Ok(CommandOutput::CumulativeGpa {
                report: crate::gpa::cumulative_gpa(&view),
            })
        }
        Command::SetGpaScale { scale } => {
            for (_, v) in &scale.points {
                points("gpa points", *v)?;
            }
            gb.scale = scale;
            Ok(CommandOutput::Done)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{approx, uid};

    fn ids() -> (TermId, CourseId, CategoryId) {
        (TermId(uid()), CourseId(uid()), CategoryId(uid()))
    }

    fn setup() -> (Gradebook, TermId, CourseId, CategoryId) {
        let (t, c, k) = ids();
        let mut gb = Gradebook::default();
        execute(
            &mut gb,
            Command::AddTerm {
                id: t,
                name: "Fall".into(),
            },
        )
        .unwrap();
        execute(
            &mut gb,
            Command::AddCourse {
                term_id: t,
                id: c,
                name: "Math".into(),
                code: Some("M101".into()),
                credits: 4.0,
                target: Some(90.0),
            },
        )
        .unwrap();
        execute(
            &mut gb,
            Command::AddCategory {
                course_id: c,
                id: k,
                name: "Homework".into(),
                weight: 100.0,
                drop_lowest: 0,
            },
        )
        .unwrap();
        (gb, t, c, k)
    }

    fn add_asg(
        gb: &mut Gradebook,
        k: CategoryId,
        possible: f64,
        score: Option<f64>,
    ) -> AssignmentId {
        let id = AssignmentId(uid());
        execute(
            gb,
            Command::AddAssignment {
                category_id: k,
                id,
                name: "hw".into(),
                points_possible: possible,
                score,
                due: NaiveDate::from_ymd_opt(2026, 9, 1),
                status: None,
                curves: vec![],
            },
        )
        .unwrap();
        id
    }

    #[test]
    fn course_grade_query() {
        let (mut gb, _, c, k) = setup();
        add_asg(&mut gb, k, 100.0, Some(92.0));
        add_asg(&mut gb, k, 100.0, None);
        let out = execute(
            &mut gb,
            Command::CourseGrade {
                course_id: c,
                curve_mode: CurveMode::Apply,
                scenario_id: None,
            },
        )
        .unwrap();
        let CommandOutput::CourseGrade { report } = out else {
            panic!("wrong output")
        };
        approx(report.current.unwrap(), 92.0);
        approx(report.projected.unwrap(), 46.0);
        assert_eq!(report.current_letter, Some(Letter::AMinus));
        assert_eq!(report.projected_letter, Some(Letter::F));
    }

    #[test]
    fn list_and_show() {
        let (mut gb, t, c, _) = setup();
        let CommandOutput::Terms { terms } = execute(&mut gb, Command::ListTerms).unwrap() else {
            panic!("wrong output")
        };
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].id, t);
        assert_eq!(terms[0].courses[0].id, c);
        let CommandOutput::Course { course } =
            execute(&mut gb, Command::ShowCourse { course_id: c }).unwrap()
        else {
            panic!("wrong output")
        };
        assert_eq!(course.name, "Math");
    }

    #[test]
    fn update_and_remove() {
        let (mut gb, t, c, k) = setup();
        let a = add_asg(&mut gb, k, 10.0, Some(5.0));
        execute(
            &mut gb,
            Command::UpdateAssignment {
                id: a,
                name: None,
                points_possible: Some(20.0),
                score: Some(None),
                due: None,
                status: Some(AssignmentStatus::Excused),
                curves: Some(vec![AssignmentCurve::AddPoints(1.0)]),
            },
        )
        .unwrap();
        let got = gb.assignment(a).unwrap();
        assert_eq!(got.score, None);
        assert_eq!(got.points_possible, 20.0);
        assert_eq!(got.status, AssignmentStatus::Excused);
        assert!(got.due.is_some());

        execute(&mut gb, Command::RemoveAssignment { id: a }).unwrap();
        assert!(gb.assignment(a).is_none());
        execute(&mut gb, Command::RemoveCategory { id: k }).unwrap();
        execute(
            &mut gb,
            Command::RenameTerm {
                id: t,
                name: "Spring".into(),
            },
        )
        .unwrap();
        execute(&mut gb, Command::RemoveCourse { id: c }).unwrap();
        execute(&mut gb, Command::RemoveTerm { id: t }).unwrap();
        assert!(gb.terms.is_empty());
    }

    #[test]
    fn validation_errors() {
        let (mut gb, t, c, k) = setup();
        let mk = |possible, score| Command::AddAssignment {
            category_id: k,
            id: AssignmentId(uid()),
            name: "x".into(),
            points_possible: possible,
            score,
            due: None,
            status: None,
            curves: vec![],
        };
        assert!(matches!(
            execute(&mut gb, mk(-1.0, None)),
            Err(CoreError::NegativePoints { .. })
        ));
        assert!(matches!(
            execute(&mut gb, mk(10.0, Some(-1.0))),
            Err(CoreError::NegativePoints { .. })
        ));
        assert!(matches!(
            execute(&mut gb, mk(f64::NAN, None)),
            Err(CoreError::NonFinite { .. })
        ));
        let bad_curve = Command::AddAssignment {
            category_id: k,
            id: AssignmentId(uid()),
            name: "x".into(),
            points_possible: 10.0,
            score: None,
            due: None,
            status: None,
            curves: vec![AssignmentCurve::ScaleTopTo { top_score: 0.0 }],
        };
        assert!(matches!(
            execute(&mut gb, bad_curve),
            Err(CoreError::InvalidCurve { .. })
        ));

        let neg_weight = Command::AddCategory {
            course_id: c,
            id: CategoryId(uid()),
            name: "x".into(),
            weight: -1.0,
            drop_lowest: 0,
        };
        assert_eq!(execute(&mut gb, neg_weight), Err(CoreError::NegativeWeight));
        let neg_credits = Command::AddCourse {
            term_id: t,
            id: CourseId(uid()),
            name: "x".into(),
            code: None,
            credits: -1.0,
            target: None,
        };
        assert_eq!(
            execute(&mut gb, neg_credits),
            Err(CoreError::NegativeCredits)
        );

        // NaN / infinity on every f64 input.
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(matches!(
                execute(&mut gb, mk(10.0, Some(bad))),
                Err(CoreError::NonFinite { .. })
            ));
            assert!(matches!(
                execute(&mut gb, mk(bad, None)),
                Err(CoreError::NonFinite { .. })
            ));
            for curve in [
                AssignmentCurve::AddPoints(bad),
                AssignmentCurve::ScaleTopTo { top_score: bad },
                AssignmentCurve::SetMax(bad),
            ] {
                let cmd = Command::AddAssignment {
                    category_id: k,
                    id: AssignmentId(uid()),
                    name: "x".into(),
                    points_possible: 10.0,
                    score: None,
                    due: None,
                    status: None,
                    curves: vec![curve],
                };
                assert!(matches!(
                    execute(&mut gb, cmd),
                    Err(CoreError::NonFinite { .. })
                ));
            }
            let cat_cmd = Command::AddCategory {
                course_id: c,
                id: CategoryId(uid()),
                name: "x".into(),
                weight: bad,
                drop_lowest: 0,
            };
            assert!(matches!(
                execute(&mut gb, cat_cmd),
                Err(CoreError::NonFinite { .. })
            ));
            let course_cmd = |credits, target| Command::AddCourse {
                term_id: t,
                id: CourseId(uid()),
                name: "x".into(),
                code: None,
                credits,
                target,
            };
            assert!(matches!(
                execute(&mut gb, course_cmd(bad, None)),
                Err(CoreError::NonFinite { .. })
            ));
            assert!(matches!(
                execute(&mut gb, course_cmd(3.0, Some(bad))),
                Err(CoreError::NonFinite { .. })
            ));
            let curve_cmd = Command::SetCourseCurve {
                course_id: c,
                curve: CourseCurve {
                    add_points: bad,
                    cutoff_shift: 0.0,
                },
            };
            assert!(matches!(
                execute(&mut gb, curve_cmd),
                Err(CoreError::NonFinite { .. })
            ));
            let shift_cmd = Command::SetCourseCurve {
                course_id: c,
                curve: CourseCurve {
                    add_points: 0.0,
                    cutoff_shift: bad,
                },
            };
            assert!(matches!(
                execute(&mut gb, shift_cmd),
                Err(CoreError::NonFinite { .. })
            ));
        }

        // Unknown ids.
        let ghost = uid();
        assert_eq!(
            execute(&mut gb, Command::RemoveTerm { id: TermId(ghost) }),
            Err(CoreError::UnknownTerm(ghost))
        );
        assert_eq!(
            execute(
                &mut gb,
                Command::ShowCourse {
                    course_id: CourseId(ghost)
                }
            ),
            Err(CoreError::UnknownCourse(ghost))
        );
        assert_eq!(
            execute(
                &mut gb,
                Command::RemoveAssignment {
                    id: AssignmentId(ghost)
                }
            ),
            Err(CoreError::UnknownAssignment(ghost))
        );

        // Duplicate ids.
        assert!(matches!(
            execute(
                &mut gb,
                Command::AddTerm {
                    id: t,
                    name: "dup".into()
                }
            ),
            Err(CoreError::DuplicateId { kind: "term", .. })
        ));
        let a = add_asg(&mut gb, k, 10.0, None);
        let dup_a = Command::AddAssignment {
            category_id: k,
            id: a,
            name: "x".into(),
            points_possible: 1.0,
            score: None,
            due: None,
            status: None,
            curves: vec![],
        };
        assert!(matches!(
            execute(&mut gb, dup_a),
            Err(CoreError::DuplicateId {
                kind: "assignment",
                ..
            })
        ));
    }

    #[test]
    fn course_curve_and_cutoffs_commands() {
        let (mut gb, _, c, k) = setup();
        add_asg(&mut gb, k, 100.0, Some(88.0));
        execute(
            &mut gb,
            Command::SetCourseCurve {
                course_id: c,
                curve: CourseCurve {
                    add_points: 2.0,
                    cutoff_shift: 0.0,
                },
            },
        )
        .unwrap();
        let mut cut = LetterCutoffs::default();
        cut.set(Letter::AMinus, 92.0);
        execute(
            &mut gb,
            Command::SetCutoffs {
                course_id: c,
                cutoffs: cut,
            },
        )
        .unwrap();
        let CommandOutput::CourseGrade { report } = execute(
            &mut gb,
            Command::CourseGrade {
                course_id: c,
                curve_mode: CurveMode::Apply,
                scenario_id: None,
            },
        )
        .unwrap() else {
            panic!("wrong output")
        };
        approx(report.current.unwrap(), 90.0);
        assert_eq!(report.current_letter, Some(Letter::BPlus));
    }

    #[test]
    fn update_course_double_option_json() {
        let (mut gb, _, c, _) = setup();
        // Absent target: unchanged. Null target: cleared. Value: set.
        let absent: Command = serde_json::from_str(&format!(
            r#"{{"command":"update_course","id":"{}","name":"Calc"}}"#,
            c.0
        ))
        .unwrap();
        execute(&mut gb, absent).unwrap();
        assert_eq!(gb.course(c).unwrap().name, "Calc");
        assert_eq!(gb.course(c).unwrap().target, Some(90.0));

        let null: Command = serde_json::from_str(&format!(
            r#"{{"command":"update_course","id":"{}","target":null,"official_letter":"A-"}}"#,
            c.0
        ))
        .unwrap();
        execute(&mut gb, null).unwrap();
        assert_eq!(gb.course(c).unwrap().target, None);
        assert_eq!(gb.course(c).unwrap().official_letter, Some(Letter::AMinus));

        let set: Command = serde_json::from_str(&format!(
            r#"{{"command":"update_course","id":"{}","target":85.5}}"#,
            c.0
        ))
        .unwrap();
        execute(&mut gb, set).unwrap();
        assert_eq!(gb.course(c).unwrap().target, Some(85.5));
    }

    #[test]
    fn serde_roundtrip_gradebook_and_command() {
        let (mut gb, _, _, k) = setup();
        let a = add_asg(&mut gb, k, 100.0, Some(70.0));
        gb.assignment_mut(a).unwrap().curves = vec![
            AssignmentCurve::AddPoints(2.0),
            AssignmentCurve::ScaleTopTo { top_score: 95.0 },
            AssignmentCurve::SetMax(90.0),
        ];
        gb.scenarios.push(crate::model::Scenario {
            id: crate::ids::ScenarioId(uid()),
            name: "what if".into(),
            overrides: vec![crate::model::Override::SetScore {
                assignment_id: a,
                score: Some(99.0),
            }],
        });
        let json = serde_json::to_string(&gb).unwrap();
        let back: Gradebook = serde_json::from_str(&json).unwrap();
        assert_eq!(gb, back);

        let cmd = Command::UpdateAssignment {
            id: a,
            name: Some("x".into()),
            points_possible: None,
            score: Some(None),
            due: Some(NaiveDate::from_ymd_opt(2026, 10, 2)),
            status: Some(AssignmentStatus::Graded),
            curves: Some(vec![AssignmentCurve::SetMax(50.0)]),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""command":"update_assignment""#));
        let back: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, back);

        let q = Command::CourseGrade {
            course_id: CourseId(uid()),
            curve_mode: CurveMode::Ignore,
            scenario_id: None,
        };
        let back: Command = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
        assert_eq!(q, back);
    }

    #[test]
    fn solve_and_target_commands() {
        let (mut gb, _t, c, k) = setup();
        add_asg(&mut gb, k, 100.0, Some(80.0));
        let pending = add_asg(&mut gb, k, 100.0, None);
        // Stored target is 90 (from setup): average of 80 and x => x = 100.
        let out = execute(
            &mut gb,
            Command::SolveTarget {
                course_id: c,
                target: None,
                locked: vec![],
                with_curve: false,
                scenario_id: None,
            },
        )
        .unwrap();
        let CommandOutput::Solve { solve: result } = out else {
            panic!()
        };
        let solver::SolveStatus::OnTrack { needed } = result.status else {
            panic!("{result:?}")
        };
        assert!((needed - 100.0).abs() < 1e-6);
        // Letter target resolves through the cutoffs (B = 83 -> needs 86).
        let out = execute(
            &mut gb,
            Command::SolveTarget {
                course_id: c,
                target: Some(TargetSpec::Letter(Letter::B)),
                locked: vec![],
                with_curve: false,
                scenario_id: None,
            },
        )
        .unwrap();
        let CommandOutput::Solve { solve: result } = out else {
            panic!()
        };
        approx(result.target_percent, 83.0);
        // Unknown locked assignment errors.
        let err = execute(
            &mut gb,
            Command::SolveTarget {
                course_id: c,
                target: None,
                locked: vec![LockedScore {
                    assignment_id: AssignmentId(uid()),
                    points: 1.0,
                }],
                with_curve: false,
                scenario_id: None,
            },
        );
        assert!(matches!(err, Err(CoreError::UnknownAssignment(_))));
        // SetTarget None then solve with no target errors; scenario view is used.
        execute(
            &mut gb,
            Command::SetTarget {
                course_id: c,
                target: None,
            },
        )
        .unwrap();
        let no_target = |gb: &mut Gradebook, sid| {
            execute(
                gb,
                Command::SolveTarget {
                    course_id: c,
                    target: None,
                    locked: vec![],
                    with_curve: false,
                    scenario_id: sid,
                },
            )
        };
        assert_eq!(no_target(&mut gb, None), Err(CoreError::NoTarget));
        let sid = ScenarioId(uid());
        execute(
            &mut gb,
            Command::CreateScenario {
                id: sid,
                name: "s".into(),
            },
        )
        .unwrap();
        execute(
            &mut gb,
            Command::AddOverride {
                scenario_id: sid,
                overlay: Override::SetScore {
                    assignment_id: pending,
                    score: Some(100.0),
                },
            },
        )
        .unwrap();
        let out = execute(
            &mut gb,
            Command::SolveTarget {
                course_id: c,
                target: Some(TargetSpec::Percent(85.0)),
                locked: vec![],
                with_curve: false,
                scenario_id: Some(sid),
            },
        )
        .unwrap();
        let CommandOutput::Solve { solve: result } = out else {
            panic!()
        };
        // Scenario has no remaining work and 90% >= 85%.
        assert_eq!(result.status, solver::SolveStatus::Secured);
    }

    #[test]
    fn scenario_lifecycle_and_gpa_commands() {
        let (mut gb, t, c, k) = setup();
        let a = add_asg(&mut gb, k, 100.0, Some(90.0));
        let sid = ScenarioId(uid());
        let create = Command::CreateScenario {
            id: sid,
            name: "what if".into(),
        };
        execute(&mut gb, create.clone()).unwrap();
        assert!(matches!(
            execute(&mut gb, create),
            Err(CoreError::DuplicateId { .. })
        ));
        execute(
            &mut gb,
            Command::RenameScenario {
                id: sid,
                name: "renamed".into(),
            },
        )
        .unwrap();
        // Bad override is rejected and not stored.
        let bad = Command::AddOverride {
            scenario_id: sid,
            overlay: Override::SetScore {
                assignment_id: AssignmentId(uid()),
                score: Some(1.0),
            },
        };
        assert!(execute(&mut gb, bad).is_err());
        assert!(gb.scenarios[0].overrides.is_empty());
        execute(
            &mut gb,
            Command::AddOverride {
                scenario_id: sid,
                overlay: Override::SetScore {
                    assignment_id: a,
                    score: Some(100.0),
                },
            },
        )
        .unwrap();
        let before = gb.clone();
        let gpa_of = |gb: &mut Gradebook, s| {
            let CommandOutput::CumulativeGpa { report } =
                execute(gb, Command::CumulativeGpa { scenario_id: s }).unwrap()
            else {
                panic!()
            };
            report.gpa
        };
        approx(gpa_of(&mut gb, None).unwrap(), 3.7);
        approx(gpa_of(&mut gb, Some(sid)).unwrap(), 4.0);
        let CommandOutput::TermGpa { report } = execute(
            &mut gb,
            Command::TermGpa {
                term_id: t,
                scenario_id: Some(sid),
            },
        )
        .unwrap() else {
            panic!()
        };
        approx(report.gpa.unwrap(), 4.0);
        let CommandOutput::CourseGrade { report } = execute(
            &mut gb,
            Command::CourseGrade {
                course_id: c,
                curve_mode: CurveMode::Apply,
                scenario_id: Some(sid),
            },
        )
        .unwrap() else {
            panic!()
        };
        approx(report.current.unwrap(), 100.0);
        let CommandOutput::Comparison { columns } = execute(
            &mut gb,
            Command::CompareScenarios {
                scenario_ids: vec![],
            },
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(columns.len(), 2);
        assert_eq!(gb, before);
        // Remove override: index out of range, then valid.
        assert!(matches!(
            execute(
                &mut gb,
                Command::RemoveOverride {
                    scenario_id: sid,
                    index: 5
                }
            ),
            Err(CoreError::OverrideIndex { .. })
        ));
        execute(
            &mut gb,
            Command::RemoveOverride {
                scenario_id: sid,
                index: 0,
            },
        )
        .unwrap();
        let CommandOutput::Scenarios { scenarios } =
            execute(&mut gb, Command::ListScenarios).unwrap()
        else {
            panic!()
        };
        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].name, "renamed");
        execute(&mut gb, Command::DeleteScenario { id: sid }).unwrap();
        assert!(matches!(
            execute(&mut gb, Command::DeleteScenario { id: sid }),
            Err(CoreError::UnknownScenario(_))
        ));
    }

    #[test]
    fn set_gpa_scale_validates() {
        let mut gb = Gradebook::default();
        let mut scale = GpaScale::default();
        scale.set(Letter::A, 5.0);
        execute(
            &mut gb,
            Command::SetGpaScale {
                scale: scale.clone(),
            },
        )
        .unwrap();
        assert_eq!(gb.scale, scale);
        scale.set(Letter::B, -1.0);
        assert!(
            execute(
                &mut gb,
                Command::SetGpaScale {
                    scale: scale.clone()
                }
            )
            .is_err()
        );
        scale.set(Letter::B, f64::NAN);
        assert!(execute(&mut gb, Command::SetGpaScale { scale }).is_err());
        approx(gb.scale.get(Letter::A).unwrap(), 5.0);
    }

    #[test]
    fn new_commands_serde_round_trip() {
        let cmds = vec![
            Command::SetTarget {
                course_id: CourseId(uid()),
                target: Some(90.0),
            },
            Command::SolveTarget {
                course_id: CourseId(uid()),
                target: Some(TargetSpec::Letter(Letter::AMinus)),
                locked: vec![LockedScore {
                    assignment_id: AssignmentId(uid()),
                    points: 9.0,
                }],
                with_curve: true,
                scenario_id: Some(ScenarioId(uid())),
            },
            Command::SolveTarget {
                course_id: CourseId(uid()),
                target: Some(TargetSpec::Percent(88.5)),
                locked: vec![],
                with_curve: false,
                scenario_id: None,
            },
            Command::CreateScenario {
                id: ScenarioId(uid()),
                name: "n".into(),
            },
            Command::AddOverride {
                scenario_id: ScenarioId(uid()),
                overlay: Override::SetOfficialLetter {
                    course_id: CourseId(uid()),
                    letter: Some(Letter::B),
                },
            },
            Command::RemoveOverride {
                scenario_id: ScenarioId(uid()),
                index: 2,
            },
            Command::ListScenarios,
            Command::CompareScenarios {
                scenario_ids: vec![ScenarioId(uid())],
            },
            Command::TermGpa {
                term_id: TermId(uid()),
                scenario_id: None,
            },
            Command::CumulativeGpa {
                scenario_id: Some(ScenarioId(uid())),
            },
            Command::SetGpaScale {
                scale: GpaScale::default(),
            },
        ];
        for c in cmds {
            let json = serde_json::to_string(&c).unwrap();
            let back: Command = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back, "{json}");
        }
        // Defaults: omitted optional fields parse.
        let json = format!(
            r#"{{"command":"solve_target","course_id":"{}"}}"#,
            uuid::Uuid::from_u128(1)
        );
        let c: Command = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            c,
            Command::SolveTarget {
                target: None,
                with_curve: false,
                ..
            }
        ));
        let json = serde_json::to_string(&Command::AddOverride {
            scenario_id: ScenarioId(uid()),
            overlay: Override::SetGpaStatus {
                course_id: CourseId(uid()),
                gpa_status: GpaStatus::Withdrawn,
            },
        })
        .unwrap();
        assert!(json.contains(r#""override":{"type":"set_gpa_status""#));
    }

    #[test]
    fn removal_prunes_scenario_overrides_and_ids_do_not_collide() {
        let (mut gb, _t, c, k) = setup();
        let a = add_asg(&mut gb, k, 100.0, None);
        let hyp = AssignmentId(uid());
        let sid = ScenarioId(uid());
        gb.scenarios.push(Scenario {
            id: sid,
            name: "s".into(),
            overrides: vec![
                Override::SetScore {
                    assignment_id: a,
                    score: Some(50.0),
                },
                Override::AddAssignment {
                    category_id: k,
                    assignment: Assignment::new(hyp, "h", 10.0),
                },
                Override::SetScore {
                    assignment_id: hyp,
                    score: Some(5.0),
                },
            ],
        });
        // Real add with a hypothetical's id is rejected.
        assert!(matches!(
            execute(
                &mut gb,
                Command::AddAssignment {
                    category_id: k,
                    id: hyp,
                    name: "x".into(),
                    points_possible: 1.0,
                    score: None,
                    due: None,
                    status: None,
                    curves: vec![],
                }
            ),
            Err(CoreError::DuplicateId { .. })
        ));
        execute(&mut gb, Command::RemoveAssignment { id: a }).unwrap();
        assert_eq!(gb.scenarios[0].overrides.len(), 2);
        let out = execute(
            &mut gb,
            Command::CompareScenarios {
                scenario_ids: vec![],
            },
        )
        .unwrap();
        let CommandOutput::Comparison { columns } = out else {
            panic!("expected comparison");
        };
        assert!(columns.iter().all(|c| c.error.is_none()));
        // Removing the course prunes everything nested.
        execute(&mut gb, Command::RemoveCourse { id: c }).unwrap();
        assert!(gb.scenarios[0].overrides.is_empty());
    }
}
