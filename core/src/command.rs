//! Typed Commands: the single entry point for every capability (CLI/MCP/UI are thin adapters).

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};

use crate::analysis::{self, ImpactEntry, LeverageReport, TrendPoint};
use crate::error::CoreError;
use crate::gpa::{CumulativeGpaReport, TermGpaReport};
use crate::grade::{self, CategoryBreakdown};
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Create a term (semester/quarter). `id` is a new UUID chosen by the caller. Returns the created id.
    AddTerm { id: TermId, name: String },
    /// Rename a term.
    RenameTerm { id: TermId, name: String },
    /// Set (or clear, with `year` omitted or null) the academic year a term belongs to, e.g. 2026. Apps group and sort terms by it.
    SetTermYear {
        id: TermId,
        #[serde(default)]
        year: Option<i32>,
    },
    /// Delete a term with all its courses, categories and assignments.
    RemoveTerm { id: TermId },
    /// Add a course to a term. `credits` is credit hours; `target` is the goal final grade as a percent (0-100). Returns the created id.
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
    /// Partially update a course. Omitted fields are unchanged; `code`, `target` and `official_letter` accept `null` to clear. `gpa_status` is normal, pass_fail, withdrawn or retaken (only normal counts toward GPA).
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
    /// Delete a course with its categories and assignments.
    RemoveCourse { id: CourseId },
    /// Add a grading category (e.g. Homework) to a course. `weight` is the relative weight (any positive scale, e.g. 30 for 30%); `drop_lowest` drops the N lowest graded assignments. Returns the created id.
    AddCategory {
        course_id: CourseId,
        id: CategoryId,
        name: String,
        weight: f64,
        #[serde(default)]
        drop_lowest: u32,
    },
    /// Partially update a category; omitted fields are unchanged.
    UpdateCategory {
        id: CategoryId,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        weight: Option<f64>,
        #[serde(default)]
        drop_lowest: Option<u32>,
    },
    /// Delete a category with its assignments.
    RemoveCategory { id: CategoryId },
    /// Add an assignment to a category. Points are raw points (not percents); `score` is raw points earned (omit if not graded yet); `due` is YYYY-MM-DD; `status` defaults to pending; `curves` are optional assignment curves (assumptions, scores stay unchanged). Returns the created id.
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
    /// Partially update an assignment. Omitted fields are unchanged; `score` and `due` accept `null` to clear; `curves` replaces the curve list. Scores are raw points.
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
    /// Delete an assignment.
    RemoveAssignment { id: AssignmentId },
    /// Partially update a course curve (an assumption, not data): `add_points` is added to the final percent, `cutoff_shift` lowers every letter cutoff. Omitted fields are unchanged; both 0 clears the curve.
    SetCourseCurve {
        course_id: CourseId,
        #[serde(default)]
        add_points: Option<f64>,
        #[serde(default)]
        cutoff_shift: Option<f64>,
    },
    /// Replace all letter cutoffs of a course (minimum percent 0-100 per letter).
    SetCutoffs {
        course_id: CourseId,
        cutoffs: LetterCutoffs,
    },
    /// Set one letter cutoff (percent 0-100) of a course; `value: null` removes that letter.
    SetCutoff {
        course_id: CourseId,
        letter: Letter,
        #[serde(default)]
        value: Option<f64>,
    },
    /// List all terms with their courses (ids, names, credits). Start here to discover ids; use show_course for categories and assignments.
    ListTerms,
    /// Show one course in full: settings, curves, cutoffs, categories and assignments with their ids, plus derived per-assignment flags (remaining, counts_as_graded, dropped, hypothetical). With `scenario_id`, shows the course as that what-if scenario sees it.
    ShowCourse {
        course_id: CourseId,
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    /// Current grade (renormalized over categories with graded work) and projected grade (full weights, ungraded work as 0), as percents 0-100 with letters. Curves apply unless `curve_mode` is ignore.
    CourseGrade {
        course_id: CourseId,
        #[serde(default)]
        curve_mode: CurveMode,
        /// Evaluate against this what-if scenario instead of the real data.
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    /// Running-grade change as each graded assignment was graded, in chronological order (percentage points).
    Impact {
        course_id: CourseId,
        #[serde(default)]
        curve_mode: CurveMode,
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    /// Running course grade over time (percents 0-100), one point per date.
    Trends {
        course_id: CourseId,
        #[serde(default)]
        curve_mode: CurveMode,
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    /// For each remaining assignment, the final-grade change (percentage points) per 10% scored on it. `baseline` (percent 0-100) is what other remaining work is assumed to score.
    Leverage {
        course_id: CourseId,
        #[serde(default)]
        baseline: Option<f64>,
        #[serde(default)]
        curve_mode: CurveMode,
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    /// Set or clear (`null`) a course's target final grade, as a percent 0-100.
    SetTarget {
        course_id: CourseId,
        target: Option<f64>,
    },
    /// Solve for the uniform percent (0-100 of points possible) needed on all remaining work to hit a target. Result is secured, on_track{needed} or unreachable. `target` is a percent or letter (omit to use the course target). `locked` fixes expected scores on specific assignments, in raw points. Ignores curves unless `with_curve`. `needed_scores` lists, per remaining assignment (course list order), the raw points needed.
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
    /// Create a named what-if scenario (hypothetical scores/curves overlaid on real data; real data is never changed). Returns the created id.
    CreateScenario { id: ScenarioId, name: String },
    /// Rename a what-if scenario.
    RenameScenario { id: ScenarioId, name: String },
    /// Delete a what-if scenario.
    DeleteScenario { id: ScenarioId },
    /// Add a hypothetical change to a scenario (set score/status, add assignment, set curves, set course curve, GPA status, official letter). Scores are raw points. A `set_*` change replaces the scenario's existing change of the same type for the same assignment or course (keeping its position); `add_assignment` always appends.
    AddOverride {
        scenario_id: ScenarioId,
        #[serde(rename = "override")]
        overlay: Override,
    },
    /// Remove a scenario override by its zero-based index.
    RemoveOverride {
        scenario_id: ScenarioId,
        index: usize,
    },
    /// List all what-if scenarios with their overrides.
    ListScenarios,
    /// Compare real data and the given scenarios side by side (course grades and GPA). Empty `scenario_ids` means all scenarios.
    CompareScenarios {
        #[serde(default)]
        scenario_ids: Vec<ScenarioId>,
    },
    /// GPA of one term on the GPA scale (4.0 default), with excluded courses and reasons.
    TermGpa {
        term_id: TermId,
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    /// Cumulative GPA across all terms (sum of points x credits / sum of credits), with excluded courses and reasons.
    CumulativeGpa {
        #[serde(default)]
        scenario_id: Option<ScenarioId>,
    },
    /// Replace the whole GPA scale (letter to grade points).
    SetGpaScale { scale: GpaScale },
    /// Set the grade points of one letter on the GPA scale.
    SetGpaScaleEntry { letter: Letter, points: f64 },
    /// Show the GPA scale (letter to grade points).
    ShowGpaScale,
    /// Replace the whole gradebook (terms, courses, scale, scenarios) with the given one. Used for importing a backup and for bootstrapping sync.
    ImportGradebook { gradebook: Gradebook },
    /// Return the whole gradebook as JSON (read-only), suitable for backup or `import_gradebook`.
    ExportGradebook,
    /// Resolve a name, code, UUID or unique UUID prefix to an id (read-only). `course` is required for `category` and narrows `assignment`; `scenario` also searches that scenario's hypothetical assignments. Ambiguous or unknown references are errors.
    Resolve {
        kind: EntityKind,
        reference: String,
        #[serde(default)]
        course: Option<CourseId>,
        #[serde(default)]
        scenario: Option<ScenarioId>,
    },
}

impl Command {
    /// Every serde `command` tag, one per variant. Adapters use it for parity checks.
    pub const TAGS: &'static [&'static str] = &[
        "add_term",
        "rename_term",
        "set_term_year",
        "remove_term",
        "add_course",
        "update_course",
        "remove_course",
        "add_category",
        "update_category",
        "remove_category",
        "add_assignment",
        "update_assignment",
        "remove_assignment",
        "set_course_curve",
        "set_cutoffs",
        "set_cutoff",
        "list_terms",
        "show_course",
        "course_grade",
        "impact",
        "trends",
        "leverage",
        "set_target",
        "solve_target",
        "create_scenario",
        "rename_scenario",
        "delete_scenario",
        "add_override",
        "remove_override",
        "list_scenarios",
        "compare_scenarios",
        "term_gpa",
        "cumulative_gpa",
        "set_gpa_scale",
        "set_gpa_scale_entry",
        "show_gpa_scale",
        "import_gradebook",
        "export_gradebook",
        "resolve",
    ];

    /// The serde tag of this command. Exhaustive on purpose: a new command must be listed here.
    pub fn tag(&self) -> &'static str {
        match self {
            Command::AddTerm { .. } => "add_term",
            Command::RenameTerm { .. } => "rename_term",
            Command::SetTermYear { .. } => "set_term_year",
            Command::RemoveTerm { .. } => "remove_term",
            Command::AddCourse { .. } => "add_course",
            Command::UpdateCourse { .. } => "update_course",
            Command::RemoveCourse { .. } => "remove_course",
            Command::AddCategory { .. } => "add_category",
            Command::UpdateCategory { .. } => "update_category",
            Command::RemoveCategory { .. } => "remove_category",
            Command::AddAssignment { .. } => "add_assignment",
            Command::UpdateAssignment { .. } => "update_assignment",
            Command::RemoveAssignment { .. } => "remove_assignment",
            Command::SetCourseCurve { .. } => "set_course_curve",
            Command::SetCutoffs { .. } => "set_cutoffs",
            Command::SetCutoff { .. } => "set_cutoff",
            Command::ListTerms => "list_terms",
            Command::ShowCourse { .. } => "show_course",
            Command::CourseGrade { .. } => "course_grade",
            Command::Impact { .. } => "impact",
            Command::Trends { .. } => "trends",
            Command::Leverage { .. } => "leverage",
            Command::SetTarget { .. } => "set_target",
            Command::SolveTarget { .. } => "solve_target",
            Command::CreateScenario { .. } => "create_scenario",
            Command::RenameScenario { .. } => "rename_scenario",
            Command::DeleteScenario { .. } => "delete_scenario",
            Command::AddOverride { .. } => "add_override",
            Command::RemoveOverride { .. } => "remove_override",
            Command::ListScenarios => "list_scenarios",
            Command::CompareScenarios { .. } => "compare_scenarios",
            Command::TermGpa { .. } => "term_gpa",
            Command::CumulativeGpa { .. } => "cumulative_gpa",
            Command::SetGpaScale { .. } => "set_gpa_scale",
            Command::SetGpaScaleEntry { .. } => "set_gpa_scale_entry",
            Command::ShowGpaScale => "show_gpa_scale",
            Command::ImportGradebook { .. } => "import_gradebook",
            Command::ExportGradebook => "export_gradebook",
            Command::Resolve { .. } => "resolve",
        }
    }

    /// True if executing this command may change the gradebook (adapters save only then).
    /// Exhaustive on purpose: a new command must be classified here.
    pub fn is_mutation(&self) -> bool {
        match self {
            Command::AddTerm { .. }
            | Command::RenameTerm { .. }
            | Command::SetTermYear { .. }
            | Command::RemoveTerm { .. }
            | Command::AddCourse { .. }
            | Command::UpdateCourse { .. }
            | Command::RemoveCourse { .. }
            | Command::AddCategory { .. }
            | Command::UpdateCategory { .. }
            | Command::RemoveCategory { .. }
            | Command::AddAssignment { .. }
            | Command::UpdateAssignment { .. }
            | Command::RemoveAssignment { .. }
            | Command::SetCourseCurve { .. }
            | Command::SetCutoffs { .. }
            | Command::SetCutoff { .. }
            | Command::SetTarget { .. }
            | Command::CreateScenario { .. }
            | Command::RenameScenario { .. }
            | Command::DeleteScenario { .. }
            | Command::AddOverride { .. }
            | Command::RemoveOverride { .. }
            | Command::SetGpaScale { .. }
            | Command::SetGpaScaleEntry { .. }
            | Command::ImportGradebook { .. } => true,
            Command::ListTerms
            | Command::ShowCourse { .. }
            | Command::CourseGrade { .. }
            | Command::Impact { .. }
            | Command::Trends { .. }
            | Command::Leverage { .. }
            | Command::SolveTarget { .. }
            | Command::ListScenarios
            | Command::CompareScenarios { .. }
            | Command::TermGpa { .. }
            | Command::CumulativeGpa { .. }
            | Command::ShowGpaScale
            | Command::ExportGradebook
            | Command::Resolve { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CourseSummary {
    pub id: CourseId,
    pub name: String,
    pub code: Option<String>,
    pub credits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TermSummary {
    pub id: TermId,
    pub name: String,
    /// Academic year, when set (`set_term_year`).
    pub year: Option<i32>,
    pub courses: Vec<CourseSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CourseGradeReport {
    pub course_id: CourseId,
    pub curve_mode: CurveMode,
    /// Renormalized over categories with graded work; `None` if no graded work.
    pub current: Option<f64>,
    pub current_letter: Option<Letter>,
    /// Full weights, remaining work as 0.
    pub projected: Option<f64>,
    pub projected_letter: Option<Letter>,
    /// How the current grade is made up, per category (course order).
    pub categories: Vec<CategoryBreakdown>,
}

/// An assignment plus flags derived by core, so adapters never re-implement grade rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AssignmentView {
    #[serde(flatten)]
    pub assignment: Assignment,
    /// Unscored work that still counts as remaining (solver, leverage).
    pub remaining: bool,
    /// Counted in the current grade (scored, or missing = 0; never excused).
    pub counts_as_graded: bool,
    /// Dropped by the category's drop-lowest rule in the current grade (curves applied).
    pub dropped: bool,
    /// Added by the scenario being viewed (never true without a scenario).
    pub hypothetical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CategoryView {
    pub id: CategoryId,
    pub name: String,
    pub weight: f64,
    pub drop_lowest: u32,
    pub assignments: Vec<AssignmentView>,
}

/// A course with derived flags on every assignment (output of `show_course`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CourseView {
    pub id: CourseId,
    pub name: String,
    pub code: Option<String>,
    pub credits: f64,
    pub cutoffs: LetterCutoffs,
    pub curve: CourseCurve,
    pub target: Option<f64>,
    pub gpa_status: GpaStatus,
    pub official_letter: Option<Letter>,
    pub categories: Vec<CategoryView>,
}

impl CourseView {
    /// Flags are computed with curves applied (the displayed grade).
    pub fn new(course: &Course, hypothetical: &std::collections::HashSet<AssignmentId>) -> Self {
        let categories = course
            .categories
            .iter()
            .map(|cat| {
                let dropped = grade::dropped_assignments(cat, CurveMode::Apply);
                CategoryView {
                    id: cat.id,
                    name: cat.name.clone(),
                    weight: cat.weight,
                    drop_lowest: cat.drop_lowest,
                    assignments: cat
                        .assignments
                        .iter()
                        .map(|a| AssignmentView {
                            assignment: a.clone(),
                            remaining: grade::is_remaining(a),
                            counts_as_graded: grade::counts_as_graded(a),
                            dropped: dropped.contains(&a.id),
                            hypothetical: hypothetical.contains(&a.id),
                        })
                        .collect(),
                }
            })
            .collect();
        Self {
            id: course.id,
            name: course.name.clone(),
            code: course.code.clone(),
            credits: course.credits,
            cutoffs: course.cutoffs.clone(),
            curve: course.curve,
            target: course.target,
            gpa_status: course.gpa_status,
            official_letter: course.official_letter,
            categories,
        }
    }
}

/// What an Add* command created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Term,
    Course,
    Category,
    Assignment,
    Scenario,
}

impl std::str::FromStr for EntityKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_lowercase().as_str() {
            "term" => Ok(EntityKind::Term),
            "course" => Ok(EntityKind::Course),
            "category" => Ok(EntityKind::Category),
            "assignment" => Ok(EntityKind::Assignment),
            "scenario" => Ok(EntityKind::Scenario),
            other => Err(format!(
                "unknown kind '{other}' (expected term, course, category, assignment or scenario)"
            )),
        }
    }
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Term => "term",
            EntityKind::Course => "course",
            EntityKind::Category => "category",
            EntityKind::Assignment => "assignment",
            EntityKind::Scenario => "scenario",
        }
    }
}

fn created(kind: EntityKind, id: uuid::Uuid) -> CommandOutput {
    CommandOutput::Created { kind, id }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum CommandOutput {
    Done,
    /// Returned by every Add* command so callers can chain on the new id.
    Created {
        kind: EntityKind,
        id: uuid::Uuid,
    },
    Terms {
        terms: Vec<TermSummary>,
    },
    Course {
        course: Box<CourseView>,
    },
    CourseGrade {
        report: CourseGradeReport,
    },
    Solve {
        solve: SolveResult,
    },
    Impact {
        entries: Vec<ImpactEntry>,
    },
    Trends {
        points: Vec<TrendPoint>,
    },
    Leverage {
        report: LeverageReport,
    },
    Scenarios {
        scenarios: Vec<Scenario>,
    },
    Comparison {
        columns: Vec<ScenarioColumn>,
    },
    TermGpa {
        report: TermGpaReport,
    },
    CumulativeGpa {
        report: CumulativeGpaReport,
    },
    GpaScale {
        scale: GpaScale,
    },
    Gradebook {
        gradebook: Gradebook,
    },
    Resolved {
        kind: EntityKind,
        id: uuid::Uuid,
        name: String,
    },
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
            Ok(created(EntityKind::Term, id.0))
        }
        Command::RenameTerm { id, name } => {
            gb.term_mut(id).ok_or(CoreError::UnknownTerm(id.0))?.name = name;
            Ok(CommandOutput::Done)
        }
        Command::SetTermYear { id, year } => {
            if year.is_some_and(|y| !(1000..=9999).contains(&y)) {
                return Err(CoreError::InvalidYear);
            }
            gb.term_mut(id).ok_or(CoreError::UnknownTerm(id.0))?.year = year;
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
            Ok(created(EntityKind::Course, id.0))
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
            Ok(created(EntityKind::Category, id.0))
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
            Ok(created(EntityKind::Assignment, id.0))
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
        Command::SetCourseCurve {
            course_id,
            add_points,
            cutoff_shift,
        } => {
            if let Some(v) = add_points {
                finite("add_points", v)?;
            }
            if let Some(v) = cutoff_shift {
                finite("cutoff_shift", v)?;
            }
            let curve = &mut gb
                .course_mut(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?
                .curve;
            if let Some(v) = add_points {
                curve.add_points = v;
            }
            if let Some(v) = cutoff_shift {
                curve.cutoff_shift = v;
            }
            Ok(CommandOutput::Done)
        }
        Command::SetCutoffs { course_id, cutoffs } => {
            for v in cutoffs.entries.iter().map(|e| &e.min) {
                finite("cutoff", *v)?;
            }
            gb.course_mut(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?
                .cutoffs = cutoffs;
            Ok(CommandOutput::Done)
        }
        Command::SetCutoff {
            course_id,
            letter,
            value,
        } => {
            if let Some(v) = value {
                finite("cutoff", v)?;
            }
            let course = gb
                .course_mut(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            match value {
                Some(v) => course.cutoffs.set(letter, v),
                None => course.cutoffs.entries.retain(|e| e.letter != letter),
            }
            Ok(CommandOutput::Done)
        }
        Command::ImportGradebook { gradebook } => {
            *gb = gradebook;
            Ok(CommandOutput::Done)
        }
        Command::ExportGradebook => Ok(CommandOutput::Gradebook {
            gradebook: gb.clone(),
        }),
        Command::ListTerms => Ok(CommandOutput::Terms {
            terms: gb
                .terms
                .iter()
                .map(|t| TermSummary {
                    id: t.id,
                    name: t.name.clone(),
                    year: t.year,
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
        Command::ShowCourse {
            course_id,
            scenario_id,
        } => {
            let view = scenario::view(gb, scenario_id)?;
            let course = view
                .course(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            let hypothetical = match scenario_id {
                Some(id) => scenario::hypothetical_ids(scenario::find_scenario(gb, id)?),
                None => Default::default(),
            };
            Ok(CommandOutput::Course {
                course: Box::new(CourseView::new(course, &hypothetical)),
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
                    categories: grade::category_breakdown(course, curve_mode),
                },
            })
        }
        Command::Impact {
            course_id,
            curve_mode,
            scenario_id,
        } => {
            let view = scenario::view(gb, scenario_id)?;
            let course = view
                .course(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            Ok(CommandOutput::Impact {
                entries: analysis::impact(course, curve_mode),
            })
        }
        Command::Trends {
            course_id,
            curve_mode,
            scenario_id,
        } => {
            let view = scenario::view(gb, scenario_id)?;
            let course = view
                .course(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            Ok(CommandOutput::Trends {
                points: analysis::trends(course, curve_mode),
            })
        }
        Command::Leverage {
            course_id,
            baseline,
            curve_mode,
            scenario_id,
        } => {
            let view = scenario::view(gb, scenario_id)?;
            let course = view
                .course(course_id)
                .ok_or(CoreError::UnknownCourse(course_id.0))?;
            Ok(CommandOutput::Leverage {
                report: analysis::leverage(course, baseline, curve_mode)?,
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
            Ok(created(EntityKind::Scenario, id.0))
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
            scenario::upsert_override(&mut candidate.overrides, overlay.clone());
            scenario::apply_scenario(gb, &candidate)?;
            scenario::upsert_override(&mut scenario_mut(gb, scenario_id)?.overrides, overlay);
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
            for v in scale.points.iter().map(|e| &e.points) {
                points("gpa points", *v)?;
            }
            gb.scale = scale;
            Ok(CommandOutput::Done)
        }
        Command::SetGpaScaleEntry { letter, points: p } => {
            points("gpa points", p)?;
            gb.scale.set(letter, p);
            Ok(CommandOutput::Done)
        }
        Command::ShowGpaScale => Ok(CommandOutput::GpaScale {
            scale: gb.scale.clone(),
        }),
        Command::Resolve {
            kind,
            reference,
            course,
            scenario,
        } => {
            let (id, name) = crate::refs::resolve_entity(gb, kind, &reference, course, scenario)?;
            Ok(CommandOutput::Resolved { kind, id, name })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CourseCurve;
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
        let CommandOutput::Course { course } = execute(
            &mut gb,
            Command::ShowCourse {
                course_id: c,
                scenario_id: None,
            },
        )
        .unwrap() else {
            panic!("wrong output")
        };
        assert_eq!(course.name, "Math");
    }

    #[test]
    fn show_course_flags_and_scenario() {
        let (mut gb, _, c, k) = setup();
        let low = add_asg(&mut gb, k, 100.0, Some(50.0));
        let high = add_asg(&mut gb, k, 100.0, Some(90.0));
        let todo = add_asg(&mut gb, k, 100.0, None);
        execute(
            &mut gb,
            Command::UpdateCategory {
                id: k,
                name: None,
                weight: None,
                drop_lowest: Some(1),
            },
        )
        .unwrap();
        let show = |gb: &mut Gradebook, scenario_id| {
            let CommandOutput::Course { course } = execute(
                gb,
                Command::ShowCourse {
                    course_id: c,
                    scenario_id,
                },
            )
            .unwrap() else {
                panic!("wrong output")
            };
            course
        };
        let flags = |v: &CourseView, id: AssignmentId| {
            v.categories
                .iter()
                .flat_map(|c| &c.assignments)
                .find(|a| a.assignment.id == id)
                .cloned()
                .unwrap()
        };
        let real = show(&mut gb, None);
        let (l, h, t) = (flags(&real, low), flags(&real, high), flags(&real, todo));
        assert!(l.dropped && l.counts_as_graded && !l.remaining && !l.hypothetical);
        assert!(!h.dropped && h.counts_as_graded);
        assert!(t.remaining && !t.counts_as_graded && !t.dropped);

        // Scenario: score the pending work and add a hypothetical assignment.
        let sid = ScenarioId(uid());
        execute(
            &mut gb,
            Command::CreateScenario {
                id: sid,
                name: "s".into(),
            },
        )
        .unwrap();
        let hyp_id = AssignmentId(uid());
        let mut hyp = Assignment::new(hyp_id, "hyp", 100.0);
        hyp.score = Some(10.0);
        for overlay in [
            Override::SetScore {
                assignment_id: todo,
                score: Some(95.0),
            },
            Override::AddAssignment {
                category_id: k,
                assignment: hyp,
            },
        ] {
            execute(
                &mut gb,
                Command::AddOverride {
                    scenario_id: sid,
                    overlay,
                },
            )
            .unwrap();
        }
        let before = gb.clone();
        let view = show(&mut gb, Some(sid));
        assert_eq!(gb, before, "viewing a scenario must not mutate real data");
        let (t, y) = (flags(&view, todo), flags(&view, hyp_id));
        assert!(!t.remaining && t.counts_as_graded && !t.hypothetical);
        assert!(y.hypothetical && y.counts_as_graded);
        // Hypothetical is lowest (10) and dropped; the 50 is no longer dropped.
        assert!(y.dropped);
        assert!(!flags(&view, low).dropped);
        // Real view is unchanged and has no hypothetical rows.
        let real = show(&mut gb, None);
        assert!(
            real.categories[0]
                .assignments
                .iter()
                .all(|a| !a.hypothetical)
        );
        assert_eq!(real.categories[0].assignments.len(), 3);
        assert!(
            execute(
                &mut gb,
                Command::ShowCourse {
                    course_id: c,
                    scenario_id: Some(ScenarioId(uid()))
                }
            )
            .is_err()
        );
    }

    #[test]
    fn course_grade_report_has_breakdown() {
        let (mut gb, _, c, k) = setup();
        add_asg(&mut gb, k, 100.0, Some(80.0));
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
        assert_eq!(report.categories.len(), 1);
        assert_eq!(report.categories[0].category_id, k);
        approx(report.categories[0].effective_weight, 100.0);
        approx(report.categories[0].average.unwrap(), 80.0);
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
                add_points: Some(bad),
                cutoff_shift: None,
            };
            assert!(matches!(
                execute(&mut gb, curve_cmd),
                Err(CoreError::NonFinite { .. })
            ));
            let shift_cmd = Command::SetCourseCurve {
                course_id: c,
                add_points: None,
                cutoff_shift: Some(bad),
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
                    course_id: CourseId(ghost),
                    scenario_id: None,
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
                add_points: Some(2.0),
                cutoff_shift: None,
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
    fn partial_course_curve_keeps_other_field() {
        let (mut gb, _, c, _) = setup();
        let set = |gb: &mut Gradebook, add_points, cutoff_shift| {
            execute(
                gb,
                Command::SetCourseCurve {
                    course_id: c,
                    add_points,
                    cutoff_shift,
                },
            )
            .unwrap();
        };
        set(&mut gb, Some(2.0), None);
        set(&mut gb, None, Some(1.5));
        let curve = gb.course(c).unwrap().curve;
        approx(curve.add_points, 2.0);
        approx(curve.cutoff_shift, 1.5);
        set(&mut gb, Some(3.0), None);
        approx(gb.course(c).unwrap().curve.cutoff_shift, 1.5);
        set(&mut gb, Some(0.0), Some(0.0));
        assert_eq!(gb.course(c).unwrap().curve, CourseCurve::default());
        assert!(
            Command::SetCourseCurve {
                course_id: c,
                add_points: None,
                cutoff_shift: None
            }
            .is_mutation()
        );
        assert!(
            Command::SetGpaScaleEntry {
                letter: Letter::A,
                points: 4.0
            }
            .is_mutation()
        );
    }

    #[test]
    fn set_cutoff_sets_and_removes() {
        let (mut gb, _, c, _) = setup();
        let run = |gb: &mut Gradebook, letter, value| {
            execute(
                gb,
                Command::SetCutoff {
                    course_id: c,
                    letter,
                    value,
                },
            )
        };
        run(&mut gb, Letter::AMinus, Some(91.0)).unwrap();
        approx(
            gb.course(c).unwrap().cutoffs.get(Letter::AMinus).unwrap(),
            91.0,
        );
        // Other cutoffs untouched.
        approx(gb.course(c).unwrap().cutoffs.get(Letter::B).unwrap(), 83.0);
        run(&mut gb, Letter::APlus, Some(97.0)).unwrap();
        approx(
            gb.course(c).unwrap().cutoffs.get(Letter::APlus).unwrap(),
            97.0,
        );
        run(&mut gb, Letter::APlus, None).unwrap();
        assert!(gb.course(c).unwrap().cutoffs.get(Letter::APlus).is_none());
        assert!(matches!(
            run(&mut gb, Letter::A, Some(f64::NAN)),
            Err(CoreError::NonFinite { .. })
        ));
        assert!(matches!(
            execute(
                &mut gb,
                Command::SetCutoff {
                    course_id: CourseId(uid()),
                    letter: Letter::A,
                    value: None
                }
            ),
            Err(CoreError::UnknownCourse(_))
        ));
    }

    #[test]
    fn set_gpa_scale_entry_edits_one_letter() {
        let mut gb = Gradebook::default();
        let run = |gb: &mut Gradebook, points| {
            execute(
                gb,
                Command::SetGpaScaleEntry {
                    letter: Letter::A,
                    points,
                },
            )
        };
        run(&mut gb, 4.3).unwrap();
        approx(gb.scale.get(Letter::A).unwrap(), 4.3);
        approx(gb.scale.get(Letter::B).unwrap(), 3.0);
        assert!(run(&mut gb, -1.0).is_err());
        assert!(run(&mut gb, f64::NAN).is_err());
        approx(gb.scale.get(Letter::A).unwrap(), 4.3);
    }

    #[test]
    fn add_commands_return_created_ids() {
        let (t, c, k) = ids();
        let mut gb = Gradebook::default();
        let out = execute(
            &mut gb,
            Command::AddTerm {
                id: t,
                name: "Fall".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            CommandOutput::Created {
                kind: EntityKind::Term,
                id: t.0
            }
        );
        let out = execute(
            &mut gb,
            Command::AddCourse {
                term_id: t,
                id: c,
                name: "M".into(),
                code: None,
                credits: 3.0,
                target: None,
            },
        )
        .unwrap();
        assert_eq!(
            out,
            CommandOutput::Created {
                kind: EntityKind::Course,
                id: c.0
            }
        );
        let out = execute(
            &mut gb,
            Command::AddCategory {
                course_id: c,
                id: k,
                name: "H".into(),
                weight: 1.0,
                drop_lowest: 0,
            },
        )
        .unwrap();
        assert_eq!(
            out,
            CommandOutput::Created {
                kind: EntityKind::Category,
                id: k.0
            }
        );
        let a = AssignmentId(uid());
        let out = execute(
            &mut gb,
            Command::AddAssignment {
                category_id: k,
                id: a,
                name: "x".into(),
                points_possible: 1.0,
                score: None,
                due: None,
                status: None,
                curves: vec![],
            },
        )
        .unwrap();
        assert_eq!(
            out,
            CommandOutput::Created {
                kind: EntityKind::Assignment,
                id: a.0
            }
        );
        let s = ScenarioId(uid());
        let out = execute(
            &mut gb,
            Command::CreateScenario {
                id: s,
                name: "S".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            CommandOutput::Created {
                kind: EntityKind::Scenario,
                id: s.0
            }
        );
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["result"], "created");
        assert_eq!(json["kind"], "scenario");
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
    fn set_term_year_sets_clears_and_validates() {
        let (mut gb, t, _c, _k) = setup();
        let set = |gb: &mut Gradebook, year| execute(gb, Command::SetTermYear { id: t, year });
        set(&mut gb, Some(2026)).unwrap();
        assert_eq!(gb.term(t).unwrap().year, Some(2026));
        let CommandOutput::Terms { terms } = execute(&mut gb, Command::ListTerms).unwrap() else {
            panic!()
        };
        assert_eq!(terms[0].year, Some(2026));
        assert!(matches!(
            set(&mut gb, Some(26)),
            Err(CoreError::InvalidYear)
        ));
        assert_eq!(gb.term(t).unwrap().year, Some(2026));
        set(&mut gb, None).unwrap();
        assert_eq!(gb.term(t).unwrap().year, None);
        assert!(matches!(
            execute(
                &mut gb,
                Command::SetTermYear {
                    id: TermId(uid()),
                    year: Some(2026)
                }
            ),
            Err(CoreError::UnknownTerm(_))
        ));
        // Old data without the field still loads.
        let json = serde_json::json!({"id": t.0, "name": "Fall"});
        let term: Term = serde_json::from_value(json).unwrap();
        assert_eq!(term.year, None);
        // Omitted year clears.
        let cmd: Command =
            serde_json::from_value(serde_json::json!({"command": "set_term_year", "id": t.0}))
                .unwrap();
        assert_eq!(cmd, Command::SetTermYear { id: t, year: None });
    }

    #[test]
    fn add_override_replaces_same_kind_for_same_target() {
        let (mut gb, _t, _c, k) = setup();
        let a = add_asg(&mut gb, k, 100.0, None);
        let b = add_asg(&mut gb, k, 100.0, None);
        let sid = ScenarioId(uid());
        execute(
            &mut gb,
            Command::CreateScenario {
                id: sid,
                name: "s".into(),
            },
        )
        .unwrap();
        let set = |gb: &mut Gradebook, id, score| {
            execute(
                gb,
                Command::AddOverride {
                    scenario_id: sid,
                    overlay: Override::SetScore {
                        assignment_id: id,
                        score: Some(score),
                    },
                },
            )
            .unwrap();
        };
        set(&mut gb, a, 80.0);
        set(&mut gb, b, 70.0);
        set(&mut gb, a, 100.0);
        assert_eq!(
            gb.scenarios[0].overrides,
            vec![
                Override::SetScore {
                    assignment_id: a,
                    score: Some(100.0),
                },
                Override::SetScore {
                    assignment_id: b,
                    score: Some(70.0),
                },
            ]
        );
        // A rejected replacement leaves the stored override untouched.
        let bad = Command::AddOverride {
            scenario_id: sid,
            overlay: Override::SetScore {
                assignment_id: a,
                score: Some(f64::NAN),
            },
        };
        assert!(execute(&mut gb, bad).is_err());
        assert_eq!(
            gb.scenarios[0].overrides[0],
            Override::SetScore {
                assignment_id: a,
                score: Some(100.0),
            }
        );
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
    fn is_mutation_classifies_commands() {
        let t = TermId(uid());
        let c = CourseId(uid());
        assert!(
            Command::AddTerm {
                id: t,
                name: "x".into()
            }
            .is_mutation()
        );
        assert!(
            Command::SetTarget {
                course_id: c,
                target: None
            }
            .is_mutation()
        );
        assert!(
            Command::CreateScenario {
                id: ScenarioId(uid()),
                name: "x".into()
            }
            .is_mutation()
        );
        assert!(
            Command::SetGpaScale {
                scale: GpaScale::default()
            }
            .is_mutation()
        );
        assert!(!Command::ListTerms.is_mutation());
        assert!(!Command::ShowGpaScale.is_mutation());
        assert!(!Command::CumulativeGpa { scenario_id: None }.is_mutation());
        assert!(
            !Command::SolveTarget {
                course_id: c,
                target: None,
                locked: vec![],
                with_curve: false,
                scenario_id: None
            }
            .is_mutation()
        );
        assert!(
            !Command::CourseGrade {
                course_id: c,
                curve_mode: CurveMode::Apply,
                scenario_id: None
            }
            .is_mutation()
        );
    }

    #[test]
    fn show_gpa_scale_returns_scale() {
        let mut gb = Gradebook::default();
        let mut scale = GpaScale::default();
        scale.set(Letter::A, 4.3);
        execute(
            &mut gb,
            Command::SetGpaScale {
                scale: scale.clone(),
            },
        )
        .unwrap();
        let out = execute(&mut gb, Command::ShowGpaScale).unwrap();
        assert_eq!(out, CommandOutput::GpaScale { scale });
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
    fn tags_match_serde_tags() {
        let set: std::collections::BTreeSet<&str> = Command::TAGS.iter().copied().collect();
        assert_eq!(set.len(), Command::TAGS.len(), "TAGS has duplicates");
        let uid = || uuid::Uuid::from_u128(7);
        let samples = [
            Command::ListTerms,
            Command::ShowGpaScale,
            Command::RemoveTerm { id: TermId(uid()) },
            Command::Resolve {
                kind: EntityKind::Term,
                reference: "x".into(),
                course: None,
                scenario: None,
            },
        ];
        for c in samples {
            let v = serde_json::to_value(&c).unwrap();
            assert_eq!(v["command"], c.tag());
            assert!(Command::TAGS.contains(&c.tag()));
        }
        // Every tag in TAGS is the tag() of some variant: tag() is exhaustive, so
        // checking the count against the match arms catches a stale list.
        let src = include_str!("command.rs");
        let arms = src
            .lines()
            .filter(|l| l.trim_start().starts_with("Command::") && l.contains("=> \""))
            .count();
        assert_eq!(arms, Command::TAGS.len());
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
            Command::Resolve {
                kind: EntityKind::Category,
                reference: "hw".into(),
                course: Some(CourseId(uid())),
                scenario: None,
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
    fn resolve_command_returns_id_and_does_not_mutate() {
        let (mut gb, t, c, k) = setup();
        let before = gb.clone();
        let cmd = |kind, reference: &str, course| Command::Resolve {
            kind,
            reference: reference.into(),
            course,
            scenario: None,
        };
        assert!(!cmd(EntityKind::Term, "x", None).is_mutation());
        let out = execute(&mut gb, cmd(EntityKind::Course, "m101", None)).unwrap();
        assert_eq!(
            out,
            CommandOutput::Resolved {
                kind: EntityKind::Course,
                id: c.0,
                name: "Math".into()
            }
        );
        let out = execute(&mut gb, cmd(EntityKind::Term, "fall", None)).unwrap();
        assert!(matches!(out, CommandOutput::Resolved { id, .. } if id == t.0));
        let out = execute(&mut gb, cmd(EntityKind::Category, &k.to_string(), Some(c))).unwrap();
        assert!(matches!(out, CommandOutput::Resolved { id, .. } if id == k.0));
        let err = execute(&mut gb, cmd(EntityKind::Course, "nope", None)).unwrap_err();
        assert!(matches!(err, CoreError::UnknownRef { .. }));
        assert_eq!(gb, before);
        let json = serde_json::to_value(CommandOutput::Resolved {
            kind: EntityKind::Term,
            id: t.0,
            name: "Fall".into(),
        })
        .unwrap();
        assert_eq!(json["result"], "resolved");
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
