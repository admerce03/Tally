//! Command-line grammar (clap). Parsing only; no logic.

use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand};
use tally_core::{AssignmentStatus, GpaStatus, Letter};

#[derive(Debug, Parser)]
#[command(
    name = "grades",
    version,
    about = "Tally: grades, GPA, target solver, what-if scenarios and analysis"
)]
pub struct Cli {
    /// Print the command output as JSON (errors as {"error": "..."}).
    #[arg(long, global = true)]
    pub json: bool,
    /// Database file (default: the platform data directory, Tally/tally.db).
    #[arg(long, global = true, env = "TALLY_DB", value_name = "PATH")]
    pub db: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Top,
}

#[derive(Debug, Subcommand)]
pub enum Top {
    /// Terms (semesters).
    #[command(subcommand)]
    Term(TermCmd),
    /// Courses.
    #[command(subcommand)]
    Course(CourseCmd),
    /// Grade categories within a course.
    #[command(subcommand)]
    Category(CategoryCmd),
    /// Assignments within a category.
    #[command(subcommand)]
    Assignment(AssignmentCmd),
    /// What score do I need on the remaining work to hit a target?
    Solve(SolveArgs),
    /// What-if scenarios.
    #[command(subcommand)]
    Scenario(ScenarioCmd),
    /// GPA per term, cumulative, and the scale.
    #[command(subcommand)]
    Gpa(GpaCmd),
    /// Running-grade change per graded assignment.
    Impact(AnalysisArgs),
    /// Grade over time.
    Trends(AnalysisArgs),
    /// Final-grade change per 10% scored on each remaining assignment.
    Leverage(LeverageArgs),
    /// Resolve a name, code or UUID (prefix) to an id.
    Resolve(ResolveArgs),
}

#[derive(Debug, clap::Args)]
pub struct ResolveArgs {
    /// term, course, category, assignment or scenario.
    pub kind: tally_core::EntityKind,
    /// Name, code, UUID or unique UUID prefix.
    pub reference: String,
    /// Course to search in (required for category; narrows assignment).
    #[arg(long)]
    pub course: Option<String>,
    /// Also search this scenario's hypothetical assignments.
    #[arg(long)]
    pub scenario: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum TermCmd {
    Add { name: String },
    List,
    Rename { term: String, name: String },
    Rm { term: String },
}

#[derive(Debug, Subcommand)]
pub enum CourseCmd {
    Add {
        term: String,
        name: String,
        #[arg(long, allow_negative_numbers = true)]
        credits: f64,
        #[arg(long)]
        code: Option<String>,
        /// Target final grade percent.
        #[arg(long, allow_negative_numbers = true)]
        target: Option<f64>,
    },
    Show {
        course: String,
    },
    Update {
        course: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, conflicts_with = "clear_code")]
        code: Option<String>,
        #[arg(long)]
        clear_code: bool,
        #[arg(long, allow_negative_numbers = true)]
        credits: Option<f64>,
        #[arg(long, allow_negative_numbers = true, conflicts_with = "clear_target")]
        target: Option<f64>,
        #[arg(long)]
        clear_target: bool,
        /// normal | pass-fail | withdrawn | retaken
        #[arg(long, value_parser = parse_gpa_status)]
        gpa_status: Option<GpaStatus>,
        /// Letter reported by the school (overrides the computed letter for GPA).
        #[arg(long, value_parser = parse_letter, conflicts_with = "clear_official_letter")]
        official_letter: Option<Letter>,
        #[arg(long)]
        clear_official_letter: bool,
    },
    Rm {
        course: String,
    },
    /// Current and projected grade.
    Grade {
        course: String,
        /// Ignore curves.
        #[arg(long)]
        no_curve: bool,
        #[arg(long)]
        scenario: Option<String>,
    },
    /// Set or clear the target grade percent.
    Target {
        course: String,
        #[arg(
            allow_negative_numbers = true,
            required_unless_present = "clear",
            conflicts_with = "clear"
        )]
        percent: Option<f64>,
        #[arg(long)]
        clear: bool,
    },
    /// Set the course curve (add points to the final grade, shift letter cutoffs).
    Curve {
        course: String,
        #[arg(long, allow_negative_numbers = true)]
        add_points: Option<f64>,
        #[arg(long, allow_negative_numbers = true)]
        cutoff_shift: Option<f64>,
        /// Remove the curve.
        #[arg(long, conflicts_with_all = ["add_points", "cutoff_shift"])]
        clear: bool,
    },
    /// Show the letter cutoffs, or set some: `A-=90 B+=87`.
    Cutoffs {
        course: String,
        pairs: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum CategoryCmd {
    Add {
        course: String,
        name: String,
        #[arg(long, allow_negative_numbers = true)]
        weight: f64,
        #[arg(long, default_value_t = 0)]
        drop_lowest: u32,
    },
    Update {
        course: String,
        category: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, allow_negative_numbers = true)]
        weight: Option<f64>,
        #[arg(long)]
        drop_lowest: Option<u32>,
    },
    Rm {
        course: String,
        category: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum AssignmentCmd {
    Add {
        course: String,
        category: String,
        name: String,
        #[arg(long, allow_negative_numbers = true)]
        points: f64,
        #[arg(long, allow_negative_numbers = true)]
        score: Option<f64>,
        /// YYYY-MM-DD
        #[arg(long)]
        due: Option<NaiveDate>,
        /// pending | submitted | graded | missing | excused
        #[arg(long, value_parser = parse_status)]
        status: Option<AssignmentStatus>,
        /// add:N | top:N | max:N (repeatable, applied in order)
        #[arg(long)]
        curve: Vec<String>,
    },
    /// `--curve` replaces all existing curves.
    Update {
        assignment: String,
        /// Restrict the name lookup to this course.
        #[arg(long)]
        course: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, allow_negative_numbers = true)]
        points: Option<f64>,
        #[arg(long, allow_negative_numbers = true, conflicts_with = "clear_score")]
        score: Option<f64>,
        #[arg(long)]
        clear_score: bool,
        #[arg(long, conflicts_with = "clear_due")]
        due: Option<NaiveDate>,
        #[arg(long)]
        clear_due: bool,
        #[arg(long, value_parser = parse_status)]
        status: Option<AssignmentStatus>,
        #[arg(long, conflicts_with = "clear_curves")]
        curve: Vec<String>,
        #[arg(long)]
        clear_curves: bool,
    },
    Rm {
        assignment: String,
        #[arg(long)]
        course: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct SolveArgs {
    pub course: String,
    /// Percent (90) or letter (A-); default is the course's stored target.
    #[arg(long)]
    pub target: Option<String>,
    /// Expected score on an assignment: `<assignment>=<points>` (repeatable).
    #[arg(long)]
    pub lock: Vec<String>,
    /// Include the course and assignment curves (default: requirement without curves).
    #[arg(long)]
    pub with_curve: bool,
    #[arg(long)]
    pub scenario: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ScenarioCmd {
    Create {
        name: String,
    },
    Rename {
        scenario: String,
        name: String,
    },
    Rm {
        scenario: String,
    },
    List,
    /// Add a hypothetical change to a scenario.
    Override {
        scenario: String,
        #[command(subcommand)]
        change: OverrideCmd,
    },
    /// Remove the override at INDEX (see `scenario list`).
    Unoverride {
        scenario: String,
        index: usize,
    },
    /// Real data and scenarios side by side (default: all scenarios).
    Compare {
        scenarios: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum OverrideCmd {
    /// SCORE is a number, or `none` to clear.
    SetScore {
        assignment: String,
        score: String,
        #[arg(long)]
        course: Option<String>,
    },
    SetStatus {
        assignment: String,
        #[arg(value_parser = parse_status)]
        status: AssignmentStatus,
        #[arg(long)]
        course: Option<String>,
    },
    /// A hypothetical assignment.
    AddAssignment {
        course: String,
        category: String,
        name: String,
        #[arg(long, allow_negative_numbers = true)]
        points: f64,
        #[arg(long, allow_negative_numbers = true)]
        score: Option<f64>,
        #[arg(long)]
        due: Option<NaiveDate>,
        #[arg(long, value_parser = parse_status)]
        status: Option<AssignmentStatus>,
        #[arg(long)]
        curve: Vec<String>,
    },
    /// Replace an assignment's curves (no `--curve` clears them).
    SetAssignmentCurves {
        assignment: String,
        #[arg(long)]
        curve: Vec<String>,
        #[arg(long)]
        course: Option<String>,
    },
    SetCourseCurve {
        course: String,
        #[arg(long, allow_negative_numbers = true, default_value_t = 0.0)]
        add_points: f64,
        #[arg(long, allow_negative_numbers = true, default_value_t = 0.0)]
        cutoff_shift: f64,
    },
    SetGpaStatus {
        course: String,
        #[arg(value_parser = parse_gpa_status)]
        gpa_status: GpaStatus,
    },
    /// LETTER is a letter, or `none` to clear.
    SetOfficialLetter { course: String, letter: String },
}

#[derive(Debug, Subcommand)]
pub enum GpaCmd {
    Term {
        term: String,
        #[arg(long)]
        scenario: Option<String>,
    },
    Cumulative {
        #[arg(long)]
        scenario: Option<String>,
    },
    /// Show the scale, or set some points: `A-=3.7 B+=3.3`.
    Scale { pairs: Vec<String> },
}

#[derive(Debug, Args)]
pub struct AnalysisArgs {
    pub course: String,
    /// Ignore curves.
    #[arg(long)]
    pub no_curve: bool,
    #[arg(long)]
    pub scenario: Option<String>,
}

#[derive(Debug, Args)]
pub struct LeverageArgs {
    #[command(flatten)]
    pub base: AnalysisArgs,
    /// Percent assumed for the other remaining work (default: current grade).
    #[arg(long, allow_negative_numbers = true)]
    pub baseline: Option<f64>,
}

pub fn parse_letter(s: &str) -> Result<Letter, String> {
    s.parse::<Letter>().map_err(|e| e.to_string())
}

pub fn parse_status(s: &str) -> Result<AssignmentStatus, String> {
    let key = s.trim().to_lowercase().replace('-', "_");
    serde_json::from_value(serde_json::Value::String(key)).map_err(|_| {
        format!("invalid status '{s}' (expected pending, submitted, graded, missing, excused)")
    })
}

pub fn parse_gpa_status(s: &str) -> Result<GpaStatus, String> {
    let key = s.trim().to_lowercase().replace('-', "_");
    serde_json::from_value(serde_json::Value::String(key)).map_err(|_| {
        format!("invalid gpa status '{s}' (expected normal, pass-fail, withdrawn, retaken)")
    })
}
