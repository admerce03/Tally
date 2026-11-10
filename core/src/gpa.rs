//! GPA: per-course letter and points, per-term and cumulative, with exclusions.

use serde::{Deserialize, Serialize};

use crate::grade::{course_letter, current_grade};
use crate::ids::{CourseId, TermId};
use crate::model::{Course, CurveMode, GpaScale, GpaStatus, Gradebook, Letter, Term};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    PassFail,
    Withdrawn,
    Retaken,
    ZeroCredits,
    NoGrade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CourseGpaEntry {
    pub course_id: CourseId,
    pub name: String,
    pub letter: Option<Letter>,
    pub points: Option<f64>,
    pub credits: f64,
    pub included: bool,
    pub exclusion: Option<ExclusionReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TermGpaReport {
    pub term_id: TermId,
    pub name: String,
    /// `None` when no course has eligible credits.
    pub gpa: Option<f64>,
    pub credits: f64,
    pub courses: Vec<CourseGpaEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CumulativeGpaReport {
    pub gpa: Option<f64>,
    pub credits: f64,
    pub terms: Vec<TermGpaReport>,
}

/// Letter used for GPA: the official letter if set, else computed from the current grade
/// (curves applied). `None` if there is no graded work.
pub fn gpa_letter(course: &Course) -> Option<Letter> {
    course.official_letter.or_else(|| {
        current_grade(course, CurveMode::Apply).map(|g| course_letter(course, g, CurveMode::Apply))
    })
}

/// Breakdown for a single course.
pub fn course_entry(course: &Course, scale: &GpaScale) -> CourseGpaEntry {
    let letter = gpa_letter(course);
    let points = letter.and_then(|l| scale.get(l));
    let exclusion = match course.gpa_status {
        GpaStatus::PassFail => Some(ExclusionReason::PassFail),
        GpaStatus::Withdrawn => Some(ExclusionReason::Withdrawn),
        GpaStatus::Retaken => Some(ExclusionReason::Retaken),
        GpaStatus::Normal if course.credits <= 0.0 => Some(ExclusionReason::ZeroCredits),
        GpaStatus::Normal if points.is_none() => Some(ExclusionReason::NoGrade),
        GpaStatus::Normal => None,
    };
    CourseGpaEntry {
        course_id: course.id,
        name: course.name.clone(),
        letter,
        points,
        credits: course.credits,
        included: exclusion.is_none(),
        exclusion,
    }
}

/// (gpa, eligible credits) over entries: sum(points x credits) / sum(credits).
pub fn aggregate<'a>(entries: impl IntoIterator<Item = &'a CourseGpaEntry>) -> (Option<f64>, f64) {
    let (mut quality, mut credits) = (0.0, 0.0);
    for e in entries.into_iter().filter(|e| e.included) {
        if let Some(p) = e.points {
            quality += p * e.credits;
            credits += e.credits;
        }
    }
    ((credits > 0.0).then(|| quality / credits), credits)
}

pub fn term_gpa(term: &Term, scale: &GpaScale) -> TermGpaReport {
    let courses: Vec<CourseGpaEntry> = term
        .courses
        .iter()
        .map(|c| course_entry(c, scale))
        .collect();
    let (gpa, credits) = aggregate(&courses);
    TermGpaReport {
        term_id: term.id,
        name: term.name.clone(),
        gpa,
        credits,
        courses,
    }
}

/// Cumulative over all eligible courses (credit-weighted, not an average of term GPAs).
pub fn cumulative_gpa(gb: &Gradebook) -> CumulativeGpaReport {
    let terms: Vec<TermGpaReport> = gb.terms.iter().map(|t| term_gpa(t, &gb.scale)).collect();
    let (gpa, credits) = aggregate(terms.iter().flat_map(|t| &t.courses));
    CumulativeGpaReport {
        gpa,
        credits,
        terms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    /// Course whose single graded assignment yields `pct` percent.
    fn graded_course(pct: f64, credits: f64) -> Course {
        let mut c = course(vec![cat(100.0, 0, vec![asg(100.0, Some(pct))])]);
        c.credits = credits;
        c
    }

    fn term(courses: Vec<Course>) -> Term {
        let mut t = Term::new(crate::ids::TermId(uid()), "T");
        t.courses = courses;
        t
    }

    fn gb(courses: Vec<Course>) -> Gradebook {
        Gradebook {
            terms: vec![term(courses)],
            ..Default::default()
        }
    }

    #[test]
    fn weighted_by_credits() {
        // A (4.0) x 3 + B (3.0) x 1 = 15 / 4.
        let r = cumulative_gpa(&gb(vec![
            graded_course(95.0, 3.0),
            graded_course(84.0, 1.0),
        ]));
        approx(r.gpa.unwrap(), 3.75);
        approx(r.credits, 4.0);
    }

    #[test]
    fn pass_fail_withdrawn_retaken_excluded() {
        let mut pf = graded_course(50.0, 3.0);
        pf.gpa_status = GpaStatus::PassFail;
        let mut wd = graded_course(50.0, 3.0);
        wd.gpa_status = GpaStatus::Withdrawn;
        let mut rt = graded_course(50.0, 3.0);
        rt.gpa_status = GpaStatus::Retaken;
        let r = cumulative_gpa(&gb(vec![pf, wd, rt, graded_course(95.0, 3.0)]));
        approx(r.gpa.unwrap(), 4.0);
        let reasons: Vec<_> = r.terms[0].courses.iter().map(|c| c.exclusion).collect();
        assert_eq!(
            reasons,
            vec![
                Some(ExclusionReason::PassFail),
                Some(ExclusionReason::Withdrawn),
                Some(ExclusionReason::Retaken),
                None
            ]
        );
    }

    #[test]
    fn zero_credit_course_excluded() {
        let r = cumulative_gpa(&gb(vec![
            graded_course(50.0, 0.0),
            graded_course(95.0, 2.0),
        ]));
        approx(r.gpa.unwrap(), 4.0);
        assert_eq!(
            r.terms[0].courses[0].exclusion,
            Some(ExclusionReason::ZeroCredits)
        );
    }

    #[test]
    fn no_eligible_courses_is_none() {
        assert_eq!(cumulative_gpa(&Gradebook::default()).gpa, None);
        let mut wd = graded_course(90.0, 3.0);
        wd.gpa_status = GpaStatus::Withdrawn;
        let r = cumulative_gpa(&gb(vec![wd, graded_course(50.0, 0.0)]));
        assert_eq!(r.gpa, None);
        approx(r.credits, 0.0);
        // No graded work: no letter.
        let empty = course(vec![cat(100.0, 0, vec![asg(100.0, None)])]);
        let r = cumulative_gpa(&gb(vec![empty]));
        assert_eq!(r.gpa, None);
        assert_eq!(
            r.terms[0].courses[0].exclusion,
            Some(ExclusionReason::NoGrade)
        );
    }

    #[test]
    fn official_letter_overrides_computed() {
        let mut c = graded_course(95.0, 3.0);
        c.official_letter = Some(Letter::BMinus);
        let r = term_gpa(&term(vec![c]), &GpaScale::default());
        approx(r.gpa.unwrap(), 2.7);
        assert_eq!(r.courses[0].letter, Some(Letter::BMinus));
        // Official letter counts even without graded work.
        let mut e = course(vec![]);
        e.official_letter = Some(Letter::A);
        approx(
            term_gpa(&term(vec![e]), &GpaScale::default()).gpa.unwrap(),
            4.0,
        );
    }

    #[test]
    fn course_curve_applies_to_letter() {
        let mut c = graded_course(86.0, 3.0);
        c.curve.cutoff_shift = 5.0; // A- cutoff becomes 85, A 88
        let r = term_gpa(&term(vec![c]), &GpaScale::default());
        assert_eq!(r.courses[0].letter, Some(Letter::AMinus));
    }

    #[test]
    fn custom_scale() {
        let mut g = gb(vec![graded_course(95.0, 3.0)]);
        g.scale.set(Letter::A, 4.3);
        approx(cumulative_gpa(&g).gpa.unwrap(), 4.3);
    }

    #[test]
    fn letter_missing_from_scale_is_no_grade() {
        let mut g = gb(vec![graded_course(95.0, 3.0)]);
        g.scale.points.retain(|e| e.letter != Letter::A);
        let r = cumulative_gpa(&g);
        assert_eq!(r.gpa, None);
        assert_eq!(
            r.terms[0].courses[0].exclusion,
            Some(ExclusionReason::NoGrade)
        );
    }
}
