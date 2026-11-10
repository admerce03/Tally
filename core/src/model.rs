//! Domain data model. Data only; math lives in `grade`.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::ids::{AssignmentId, CategoryId, CourseId, ScenarioId, TermId};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Gradebook {
    #[serde(default)]
    pub terms: Vec<Term>,
    #[serde(default)]
    pub scale: GpaScale,
    #[serde(default)]
    pub scenarios: Vec<Scenario>,
}

impl Gradebook {
    pub fn term(&self, id: TermId) -> Option<&Term> {
        self.terms.iter().find(|t| t.id == id)
    }
    pub fn term_mut(&mut self, id: TermId) -> Option<&mut Term> {
        self.terms.iter_mut().find(|t| t.id == id)
    }
    pub fn course(&self, id: CourseId) -> Option<&Course> {
        self.terms
            .iter()
            .flat_map(|t| &t.courses)
            .find(|c| c.id == id)
    }
    pub fn course_mut(&mut self, id: CourseId) -> Option<&mut Course> {
        self.terms
            .iter_mut()
            .flat_map(|t| t.courses.iter_mut())
            .find(|c| c.id == id)
    }
    pub fn category(&self, id: CategoryId) -> Option<&Category> {
        self.terms
            .iter()
            .flat_map(|t| &t.courses)
            .flat_map(|c| &c.categories)
            .find(|c| c.id == id)
    }
    pub fn category_mut(&mut self, id: CategoryId) -> Option<&mut Category> {
        self.terms
            .iter_mut()
            .flat_map(|t| t.courses.iter_mut())
            .flat_map(|c| c.categories.iter_mut())
            .find(|c| c.id == id)
    }
    pub fn assignment(&self, id: AssignmentId) -> Option<&Assignment> {
        self.terms
            .iter()
            .flat_map(|t| &t.courses)
            .flat_map(|c| &c.categories)
            .flat_map(|c| &c.assignments)
            .find(|a| a.id == id)
    }
    pub fn assignment_mut(&mut self, id: AssignmentId) -> Option<&mut Assignment> {
        self.terms
            .iter_mut()
            .flat_map(|t| t.courses.iter_mut())
            .flat_map(|c| c.categories.iter_mut())
            .flat_map(|c| c.assignments.iter_mut())
            .find(|a| a.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Term {
    pub id: TermId,
    pub name: String,
    #[serde(default)]
    pub courses: Vec<Course>,
}

impl Term {
    pub fn new(id: TermId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            courses: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpaStatus {
    #[default]
    Normal,
    PassFail,
    Withdrawn,
    Retaken,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Course {
    pub id: CourseId,
    pub name: String,
    #[serde(default)]
    pub code: Option<String>,
    pub credits: f64,
    #[serde(default)]
    pub cutoffs: LetterCutoffs,
    #[serde(default)]
    pub curve: CourseCurve,
    /// Target final grade as a percent (0-100).
    #[serde(default)]
    pub target: Option<f64>,
    #[serde(default)]
    pub gpa_status: GpaStatus,
    #[serde(default)]
    pub official_letter: Option<Letter>,
    #[serde(default)]
    pub categories: Vec<Category>,
}

impl Course {
    pub fn new(id: CourseId, name: impl Into<String>, credits: f64) -> Self {
        Self {
            id,
            name: name.into(),
            code: None,
            credits,
            cutoffs: LetterCutoffs::default(),
            curve: CourseCurve::default(),
            target: None,
            gpa_status: GpaStatus::Normal,
            official_letter: None,
            categories: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub weight: f64,
    #[serde(default)]
    pub drop_lowest: u32,
    #[serde(default)]
    pub assignments: Vec<Assignment>,
}

impl Category {
    pub fn new(id: CategoryId, name: impl Into<String>, weight: f64) -> Self {
        Self {
            id,
            name: name.into(),
            weight,
            drop_lowest: 0,
            assignments: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    #[default]
    Pending,
    Submitted,
    Graded,
    Missing,
    Excused,
}

/// A curve is an assumption layered on the stored score; it never mutates it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentCurve {
    /// Add N points to the score.
    AddPoints(f64),
    /// Scale so `top_score` becomes full marks: score * possible / top_score.
    ScaleTopTo { top_score: f64 },
    /// Change the points possible to `new_max` (score unchanged).
    SetMax(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub id: AssignmentId,
    pub name: String,
    pub points_possible: f64,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub due: Option<NaiveDate>,
    #[serde(default)]
    pub status: AssignmentStatus,
    #[serde(default)]
    pub curves: Vec<AssignmentCurve>,
}

impl Assignment {
    pub fn new(id: AssignmentId, name: impl Into<String>, points_possible: f64) -> Self {
        Self {
            id,
            name: name.into(),
            points_possible,
            score: None,
            due: None,
            status: AssignmentStatus::Pending,
            curves: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct CourseCurve {
    /// Added to the final percent.
    #[serde(default)]
    pub add_points: f64,
    /// Subtracted from every letter cutoff.
    #[serde(default)]
    pub cutoff_shift: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveMode {
    #[default]
    Apply,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Letter {
    #[serde(rename = "A+")]
    APlus,
    A,
    #[serde(rename = "A-")]
    AMinus,
    #[serde(rename = "B+")]
    BPlus,
    B,
    #[serde(rename = "B-")]
    BMinus,
    #[serde(rename = "C+")]
    CPlus,
    C,
    #[serde(rename = "C-")]
    CMinus,
    #[serde(rename = "D+")]
    DPlus,
    D,
    F,
}

impl Letter {
    /// Highest to lowest.
    pub const ALL: [Letter; 12] = [
        Letter::APlus,
        Letter::A,
        Letter::AMinus,
        Letter::BPlus,
        Letter::B,
        Letter::BMinus,
        Letter::CPlus,
        Letter::C,
        Letter::CMinus,
        Letter::DPlus,
        Letter::D,
        Letter::F,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Letter::APlus => "A+",
            Letter::A => "A",
            Letter::AMinus => "A-",
            Letter::BPlus => "B+",
            Letter::B => "B",
            Letter::BMinus => "B-",
            Letter::CPlus => "C+",
            Letter::C => "C",
            Letter::CMinus => "C-",
            Letter::DPlus => "D+",
            Letter::D => "D",
            Letter::F => "F",
        }
    }
}

/// Minimum percent for each letter. Letters without an entry are never awarded (F is the fallback).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetterCutoffs {
    pub entries: Vec<(Letter, f64)>,
}

impl LetterCutoffs {
    pub fn get(&self, letter: Letter) -> Option<f64> {
        self.entries
            .iter()
            .find(|(l, _)| *l == letter)
            .map(|(_, v)| *v)
    }

    pub fn set(&mut self, letter: Letter, value: f64) {
        match self.entries.iter_mut().find(|(l, _)| *l == letter) {
            Some(e) => e.1 = value,
            None => self.entries.push((letter, value)),
        }
    }
}

impl Default for LetterCutoffs {
    fn default() -> Self {
        Self {
            entries: vec![
                (Letter::A, 93.0),
                (Letter::AMinus, 90.0),
                (Letter::BPlus, 87.0),
                (Letter::B, 83.0),
                (Letter::BMinus, 80.0),
                (Letter::CPlus, 77.0),
                (Letter::C, 73.0),
                (Letter::CMinus, 70.0),
                (Letter::DPlus, 67.0),
                (Letter::D, 63.0),
                (Letter::F, 0.0),
            ],
        }
    }
}

/// Letter to grade points (editable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpaScale {
    pub points: Vec<(Letter, f64)>,
}

impl GpaScale {
    pub fn get(&self, letter: Letter) -> Option<f64> {
        self.points
            .iter()
            .find(|(l, _)| *l == letter)
            .map(|(_, v)| *v)
    }

    pub fn set(&mut self, letter: Letter, value: f64) {
        match self.points.iter_mut().find(|(l, _)| *l == letter) {
            Some(e) => e.1 = value,
            None => self.points.push((letter, value)),
        }
    }
}

impl Default for GpaScale {
    fn default() -> Self {
        Self {
            points: vec![
                (Letter::APlus, 4.0),
                (Letter::A, 4.0),
                (Letter::AMinus, 3.7),
                (Letter::BPlus, 3.3),
                (Letter::B, 3.0),
                (Letter::BMinus, 2.7),
                (Letter::CPlus, 2.3),
                (Letter::C, 2.0),
                (Letter::CMinus, 1.7),
                (Letter::DPlus, 1.3),
                (Letter::D, 1.0),
                (Letter::F, 0.0),
            ],
        }
    }
}

/// What-if scenario: named overlay of hypothetical changes (see `scenario::apply_scenario`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub id: ScenarioId,
    pub name: String,
    #[serde(default)]
    pub overrides: Vec<Override>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Override {
    SetScore {
        assignment_id: AssignmentId,
        score: Option<f64>,
    },
    SetStatus {
        assignment_id: AssignmentId,
        status: AssignmentStatus,
    },
    AddAssignment {
        category_id: CategoryId,
        assignment: Assignment,
    },
    SetAssignmentCurves {
        assignment_id: AssignmentId,
        curves: Vec<AssignmentCurve>,
    },
    SetCourseCurve {
        course_id: CourseId,
        curve: CourseCurve,
    },
    SetGpaStatus {
        course_id: CourseId,
        gpa_status: GpaStatus,
    },
    SetOfficialLetter {
        course_id: CourseId,
        #[serde(default)]
        letter: Option<Letter>,
    },
}
