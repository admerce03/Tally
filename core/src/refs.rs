//! Reference resolution: turn a user-supplied string into an entity id.
//!
//! A reference is a full UUID, a unique UUID prefix (at least 4 hex characters), or a
//! case-insensitive exact name (courses also match their code). Pure: works on `&Gradebook`.

use uuid::Uuid;

use crate::command::EntityKind;
use crate::error::CoreError;
use crate::ids::{AssignmentId, CategoryId, CourseId, ScenarioId, TermId};
use crate::model::{Course, Gradebook, Override};

const MIN_PREFIX: usize = 4;

struct Candidate {
    id: Uuid,
    name: String,
    /// Extra exact-match name (course code).
    alias: Option<String>,
}

impl Candidate {
    fn new(id: Uuid, name: &str, alias: Option<&str>) -> Self {
        Self {
            id,
            name: name.to_string(),
            alias: alias.map(str::to_string),
        }
    }

    fn label(&self) -> String {
        let short: String = self.id.to_string().chars().take(8).collect();
        format!("{} ({short})", self.name)
    }

    fn matches_name(&self, lower: &str) -> bool {
        self.name.to_lowercase() == lower
            || self
                .alias
                .as_ref()
                .is_some_and(|a| a.to_lowercase() == lower)
    }
}

fn pick(
    kind: &'static str,
    reference: &str,
    found: &[&Candidate],
) -> Option<Result<Uuid, CoreError>> {
    match found {
        [] => None,
        [one] => Some(Ok(one.id)),
        many => Some(Err(CoreError::AmbiguousRef {
            kind,
            reference: reference.to_string(),
            candidates: many.iter().map(|c| c.label()).collect(),
        })),
    }
}

fn is_prefix_like(lower: &str) -> bool {
    lower.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        && lower.chars().filter(char::is_ascii_hexdigit).count() >= MIN_PREFIX
}

fn resolve(kind: &'static str, reference: &str, cands: &[Candidate]) -> Result<Uuid, CoreError> {
    let unknown = || CoreError::UnknownRef {
        kind,
        reference: reference.to_string(),
    };
    let r = reference.trim();
    if let Ok(u) = Uuid::parse_str(r) {
        return cands
            .iter()
            .find(|c| c.id == u)
            .map(|c| c.id)
            .ok_or_else(unknown);
    }
    let lower = r.to_lowercase();
    let by_name: Vec<&Candidate> = cands.iter().filter(|c| c.matches_name(&lower)).collect();
    if let Some(res) = pick(kind, reference, &by_name) {
        return res;
    }
    if is_prefix_like(&lower) {
        let by_prefix: Vec<&Candidate> = cands
            .iter()
            .filter(|c| {
                c.id.hyphenated().to_string().starts_with(&lower)
                    || c.id.simple().to_string().starts_with(&lower)
            })
            .collect();
        if let Some(res) = pick(kind, reference, &by_prefix) {
            return res;
        }
    }
    Err(unknown())
}

pub fn resolve_term(gb: &Gradebook, reference: &str) -> Result<TermId, CoreError> {
    let cands: Vec<Candidate> = gb
        .terms
        .iter()
        .map(|t| Candidate::new(t.id.0, &t.name, None))
        .collect();
    resolve("term", reference, &cands).map(TermId)
}

/// Matches the course name or code.
pub fn resolve_course(gb: &Gradebook, reference: &str) -> Result<CourseId, CoreError> {
    let cands: Vec<Candidate> = gb
        .terms
        .iter()
        .flat_map(|t| &t.courses)
        .map(|c| Candidate::new(c.id.0, &c.name, c.code.as_deref()))
        .collect();
    resolve("course", reference, &cands).map(CourseId)
}

/// Categories are scoped to one course (names repeat across courses).
pub fn resolve_category(
    gb: &Gradebook,
    course: CourseId,
    reference: &str,
) -> Result<CategoryId, CoreError> {
    let course = gb
        .course(course)
        .ok_or(CoreError::UnknownCourse(course.0))?;
    let cands: Vec<Candidate> = course
        .categories
        .iter()
        .map(|c| Candidate::new(c.id.0, &c.name, None))
        .collect();
    resolve("category", reference, &cands).map(CategoryId)
}

/// With `course: Some`, only that course's assignments are candidates. With `scenario: Some`,
/// that scenario's hypothetical assignments are candidates too (scoped the same way).
pub fn resolve_assignment(
    gb: &Gradebook,
    course: Option<CourseId>,
    scenario: Option<ScenarioId>,
    reference: &str,
) -> Result<AssignmentId, CoreError> {
    let mut cands = Vec::new();
    let mut add_course = |c: &Course| {
        for a in c.categories.iter().flat_map(|k| &k.assignments) {
            cands.push(Candidate::new(a.id.0, &a.name, None));
        }
    };
    match course {
        Some(id) => add_course(gb.course(id).ok_or(CoreError::UnknownCourse(id.0))?),
        None => gb
            .terms
            .iter()
            .flat_map(|t| &t.courses)
            .for_each(&mut add_course),
    }
    if let Some(sid) = scenario {
        let sc = crate::scenario::find_scenario(gb, sid)?;
        for o in &sc.overrides {
            let Override::AddAssignment {
                category_id,
                assignment,
            } = o
            else {
                continue;
            };
            let in_scope = course.is_none_or(|cid| {
                gb.course(cid)
                    .is_some_and(|c| c.categories.iter().any(|k| k.id == *category_id))
            });
            if in_scope {
                cands.push(Candidate::new(assignment.id.0, &assignment.name, None));
            }
        }
    }
    resolve("assignment", reference, &cands).map(AssignmentId)
}

pub fn resolve_scenario(gb: &Gradebook, reference: &str) -> Result<ScenarioId, CoreError> {
    let cands: Vec<Candidate> = gb
        .scenarios
        .iter()
        .map(|s| Candidate::new(s.id.0, &s.name, None))
        .collect();
    resolve("scenario", reference, &cands).map(ScenarioId)
}

/// Resolve a reference of any kind to its id and display name. Categories need `course`;
/// assignments are narrowed by `course` and, with `scenario`, include hypothetical ones.
pub fn resolve_entity(
    gb: &Gradebook,
    kind: EntityKind,
    reference: &str,
    course: Option<CourseId>,
    scenario: Option<ScenarioId>,
) -> Result<(Uuid, String), CoreError> {
    match kind {
        EntityKind::Term => {
            let id = resolve_term(gb, reference)?;
            let name = gb.term(id).map(|t| t.name.clone()).unwrap_or_default();
            Ok((id.0, name))
        }
        EntityKind::Course => {
            let id = resolve_course(gb, reference)?;
            let name = gb.course(id).map(|c| c.name.clone()).unwrap_or_default();
            Ok((id.0, name))
        }
        EntityKind::Category => {
            let course = course.ok_or(CoreError::MissingContext {
                kind: "category",
                needs: "a course",
            })?;
            let id = resolve_category(gb, course, reference)?;
            let name = gb.category(id).map(|c| c.name.clone()).unwrap_or_default();
            Ok((id.0, name))
        }
        EntityKind::Assignment => {
            let id = resolve_assignment(gb, course, scenario, reference)?;
            let real = gb.assignment(id).map(|a| a.name.clone());
            let name = real.or_else(|| {
                let sc = scenario.and_then(|s| crate::scenario::find_scenario(gb, s).ok())?;
                sc.overrides.iter().find_map(|o| match o {
                    Override::AddAssignment { assignment, .. } if assignment.id == id => {
                        Some(assignment.name.clone())
                    }
                    _ => None,
                })
            });
            Ok((id.0, name.unwrap_or_default()))
        }
        EntityKind::Scenario => {
            let id = resolve_scenario(gb, reference)?;
            let name = gb
                .scenarios
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.name.clone())
                .unwrap_or_default();
            Ok((id.0, name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Assignment, Category, Scenario, Term};

    fn id(prefix: u128) -> Uuid {
        Uuid::from_u128(prefix << 108)
    }

    fn gb() -> Gradebook {
        let mut a1 = Assignment::new(AssignmentId(id(0xaaaa1)), "Quiz 1", 10.0);
        a1.score = Some(5.0);
        let a2 = Assignment::new(AssignmentId(id(0xaaaa2)), "Quiz 1", 10.0);
        let mut cat = Category::new(CategoryId(id(0xbbbb1)), "Homework", 10.0);
        cat.assignments = vec![a1, a2];
        let mut c1 = Course::new(CourseId(id(0xcccc1)), "Calculus", 4.0);
        c1.code = Some("MATH101".into());
        c1.categories = vec![cat];
        let mut other = Category::new(CategoryId(id(0xbbbb2)), "Homework", 10.0);
        other
            .assignments
            .push(Assignment::new(AssignmentId(id(0xdddd1)), "Essay", 5.0));
        let mut c2 = Course::new(CourseId(id(0xcccc2)), "Physics", 3.0);
        c2.categories = vec![other];
        let mut t = Term::new(TermId(id(0xeeee1)), "Fall 2026");
        t.courses = vec![c1, c2];
        Gradebook {
            terms: vec![t, Term::new(TermId(id(0xeeee2)), "Spring 2027")],
            scale: Default::default(),
            scenarios: vec![Scenario {
                id: ScenarioId(id(0xffff1)),
                name: "Best case".into(),
                overrides: vec![],
            }],
        }
    }

    #[test]
    fn full_uuid_prefix_and_name() {
        let g = gb();
        let full = id(0xcccc1).to_string();
        assert_eq!(resolve_course(&g, &full), Ok(CourseId(id(0xcccc1))));
        assert_eq!(resolve_course(&g, "cccc1"), Ok(CourseId(id(0xcccc1))));
        assert_eq!(resolve_course(&g, "CCCC2"), Ok(CourseId(id(0xcccc2))));
        assert_eq!(resolve_course(&g, "calculus"), Ok(CourseId(id(0xcccc1))));
        assert_eq!(resolve_course(&g, "math101"), Ok(CourseId(id(0xcccc1))));
        assert_eq!(resolve_term(&g, "spring 2027"), Ok(TermId(id(0xeeee2))));
        assert_eq!(
            resolve_scenario(&g, "BEST CASE"),
            Ok(ScenarioId(id(0xffff1)))
        );
    }

    #[test]
    fn unknown_and_short_prefix() {
        let g = gb();
        assert!(matches!(
            resolve_course(&g, "nope"),
            Err(CoreError::UnknownRef { kind: "course", .. })
        ));
        // Full UUID that does not exist.
        assert!(matches!(
            resolve_term(&g, &Uuid::from_u128(7).to_string()),
            Err(CoreError::UnknownRef { .. })
        ));
        // Prefix below the 4-char minimum is not a prefix and not a name.
        assert!(matches!(
            resolve_course(&g, "ccc"),
            Err(CoreError::UnknownRef { .. })
        ));
        assert!(matches!(
            resolve_scenario(&Gradebook::default(), "x"),
            Err(CoreError::UnknownRef { .. })
        ));
    }

    #[test]
    fn ambiguous_prefix_lists_candidates() {
        let g = gb();
        let Err(CoreError::AmbiguousRef { candidates, .. }) = resolve_course(&g, "cccc") else {
            panic!("expected ambiguous");
        };
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0].starts_with("Calculus"));
        let msg = resolve_course(&g, "cccc").unwrap_err().to_string();
        assert!(msg.contains("Calculus") && msg.contains("Physics"));
    }

    #[test]
    fn category_is_course_scoped() {
        let g = gb();
        let c1 = CourseId(id(0xcccc1));
        let c2 = CourseId(id(0xcccc2));
        assert_eq!(
            resolve_category(&g, c1, "homework"),
            Ok(CategoryId(id(0xbbbb1)))
        );
        assert_eq!(
            resolve_category(&g, c2, "homework"),
            Ok(CategoryId(id(0xbbbb2)))
        );
        // Category of another course is not visible.
        assert!(matches!(
            resolve_category(&g, c1, "bbbb2"),
            Err(CoreError::UnknownRef { .. })
        ));
        assert!(matches!(
            resolve_category(&g, CourseId(Uuid::from_u128(1)), "homework"),
            Err(CoreError::UnknownCourse(_))
        ));
    }

    #[test]
    fn assignment_scope_optional() {
        let g = gb();
        let c1 = CourseId(id(0xcccc1));
        // Same name twice in one course: ambiguous, but a prefix disambiguates.
        assert!(matches!(
            resolve_assignment(&g, Some(c1), None, "quiz 1"),
            Err(CoreError::AmbiguousRef { .. })
        ));
        assert_eq!(
            resolve_assignment(&g, Some(c1), None, "aaaa2"),
            Ok(AssignmentId(id(0xaaaa2)))
        );
        assert_eq!(
            resolve_assignment(&g, None, None, "essay"),
            Ok(AssignmentId(id(0xdddd1)))
        );
        assert!(matches!(
            resolve_assignment(&g, Some(c1), None, "essay"),
            Err(CoreError::UnknownRef { .. })
        ));
    }

    #[test]
    fn name_beats_uuid_prefix() {
        let mut g = gb();
        g.terms[1].name = "2026".into();
        let mut cafe = Course::new(CourseId(id(0xcafe5)), "Cafe", 2.0);
        cafe.code = Some("dead".into());
        g.terms[0].courses.push(cafe);
        g.terms[0]
            .courses
            .push(Course::new(CourseId(id(0xcafe6)), "Other", 2.0));
        g.terms.push(Term::new(TermId(id(0x2026a)), "Winter"));
        // "cafe" is a prefix of two ids but also the exact name of one course: name wins.
        assert_eq!(resolve_course(&g, "Cafe"), Ok(CourseId(id(0xcafe5))));
        assert_eq!(resolve_course(&g, "cafe"), Ok(CourseId(id(0xcafe5))));
        assert_eq!(resolve_course(&g, "dead"), Ok(CourseId(id(0xcafe5))));
        // "2026" is a term name; the id starting with 2026a is not consulted.
        assert_eq!(resolve_term(&g, "2026"), Ok(TermId(id(0xeeee2))));
        // Without a name match the prefix still works.
        assert_eq!(resolve_term(&g, "2026a"), Ok(TermId(id(0x2026a))));
        assert_eq!(resolve_course(&g, "cafe6"), Ok(CourseId(id(0xcafe6))));
        assert!(matches!(
            resolve_course(&g, "caf"),
            Err(CoreError::UnknownRef { .. })
        ));
    }

    #[test]
    fn dashes_only_is_not_a_prefix() {
        let g = gb();
        for r in ["----", "-", "--ab-", "a-b"] {
            assert!(
                matches!(resolve_course(&g, r), Err(CoreError::UnknownRef { .. })),
                "{r}"
            );
        }
        let hyphenated = id(0xcccc1).hyphenated().to_string();
        assert_eq!(
            resolve_course(&g, &hyphenated[..13]),
            Ok(CourseId(id(0xcccc1)))
        );
    }

    #[test]
    fn scenario_context_finds_hypothetical_assignments() {
        let mut g = gb();
        let c1 = CourseId(id(0xcccc1));
        let c2 = CourseId(id(0xcccc2));
        let hypo = Assignment::new(AssignmentId(id(0x9999a)), "Bonus project", 20.0);
        g.scenarios[0].overrides.push(Override::AddAssignment {
            category_id: CategoryId(id(0xbbbb1)),
            assignment: hypo,
        });
        let sid = g.scenarios[0].id;
        let want = AssignmentId(id(0x9999a));
        assert_eq!(
            resolve_assignment(&g, None, Some(sid), "bonus project"),
            Ok(want)
        );
        assert_eq!(
            resolve_assignment(&g, Some(c1), Some(sid), "9999a"),
            Ok(want)
        );
        // Not visible to real data, or outside its course scope.
        assert!(matches!(
            resolve_assignment(&g, None, None, "bonus project"),
            Err(CoreError::UnknownRef { .. })
        ));
        assert!(matches!(
            resolve_assignment(&g, Some(c2), Some(sid), "bonus project"),
            Err(CoreError::UnknownRef { .. })
        ));
        assert_eq!(
            resolve_assignment(&g, None, Some(sid), "essay"),
            Ok(AssignmentId(id(0xdddd1)))
        );
        assert!(matches!(
            resolve_assignment(&g, None, Some(ScenarioId(Uuid::from_u128(5))), "essay"),
            Err(CoreError::UnknownScenario(_))
        ));
    }

    #[test]
    fn resolve_entity_returns_id_and_name_for_every_kind() {
        let g = gb();
        let c1 = CourseId(id(0xcccc1));
        let r = |k, s: &str, c, sc| resolve_entity(&g, k, s, c, sc);
        assert_eq!(
            r(EntityKind::Term, "spring 2027", None, None),
            Ok((id(0xeeee2), "Spring 2027".to_string()))
        );
        assert_eq!(
            r(EntityKind::Course, "math101", None, None),
            Ok((id(0xcccc1), "Calculus".to_string()))
        );
        assert_eq!(
            r(EntityKind::Category, "homework", Some(c1), None),
            Ok((id(0xbbbb1), "Homework".to_string()))
        );
        assert_eq!(
            r(EntityKind::Assignment, "essay", None, None),
            Ok((id(0xdddd1), "Essay".to_string()))
        );
        assert_eq!(
            r(EntityKind::Scenario, "best case", None, None),
            Ok((id(0xffff1), "Best case".to_string()))
        );
    }

    #[test]
    fn resolve_entity_errors_and_hypothetical_names() {
        let mut g = gb();
        assert!(matches!(
            resolve_entity(&g, EntityKind::Category, "homework", None, None),
            Err(CoreError::MissingContext { .. })
        ));
        assert!(matches!(
            resolve_entity(&g, EntityKind::Assignment, "quiz 1", None, None),
            Err(CoreError::AmbiguousRef { .. })
        ));
        assert!(matches!(
            resolve_entity(&g, EntityKind::Course, "nope", None, None),
            Err(CoreError::UnknownRef { .. })
        ));
        g.scenarios[0].overrides.push(Override::AddAssignment {
            category_id: CategoryId(id(0xbbbb1)),
            assignment: Assignment::new(AssignmentId(id(0x9999a)), "Bonus project", 20.0),
        });
        let sid = g.scenarios[0].id;
        assert_eq!(
            resolve_entity(&g, EntityKind::Assignment, "bonus project", None, Some(sid)),
            Ok((id(0x9999a), "Bonus project".to_string()))
        );
    }
}
