//! Turn parsed arguments into a core `Command`. Resolves refs through core; no grade math.

use anyhow::{Context, Result, anyhow, bail};
use tally_core::solver::{LockedScore, TargetSpec};
use tally_core::{
    Assignment, AssignmentCurve, AssignmentId, AssignmentStatus, CategoryId, Command, CourseCurve,
    CourseId, CurveMode, Gradebook, Letter, Override, ScenarioId, TermId, resolve_assignment,
    resolve_category, resolve_course, resolve_scenario, resolve_term,
};
use uuid::Uuid;

use crate::args::*;

/// One or more core Commands to run in order (multi-pair edits like `cutoffs A=90 B=80`);
/// the last output is the result.
pub struct Planned {
    pub commands: Vec<Command>,
}

impl Planned {
    fn plain(command: Command) -> Self {
        Self {
            commands: vec![command],
        }
    }
}

pub fn short_id(id: Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

fn mode(no_curve: bool) -> CurveMode {
    if no_curve {
        CurveMode::Ignore
    } else {
        CurveMode::Apply
    }
}

fn scenario_opt(gb: &Gradebook, s: &Option<String>) -> Result<Option<ScenarioId>> {
    Ok(s.as_deref().map(|r| resolve_scenario(gb, r)).transpose()?)
}

fn course_opt(gb: &Gradebook, c: &Option<String>) -> Result<Option<CourseId>> {
    Ok(c.as_deref().map(|r| resolve_course(gb, r)).transpose()?)
}

fn parse_curve(s: &str) -> Result<AssignmentCurve> {
    let (kind, value) = s
        .split_once(':')
        .ok_or_else(|| anyhow!("invalid curve '{s}' (expected add:N, top:N or max:N)"))?;
    let n: f64 = value
        .trim()
        .parse()
        .with_context(|| format!("invalid curve number in '{s}'"))?;
    match kind.trim().to_lowercase().as_str() {
        "add" => Ok(AssignmentCurve::AddPoints(n)),
        "top" => Ok(AssignmentCurve::ScaleTopTo { top_score: n }),
        "max" => Ok(AssignmentCurve::SetMax(n)),
        _ => bail!("invalid curve '{s}' (expected add:N, top:N or max:N)"),
    }
}

fn parse_curves(list: &[String]) -> Result<Vec<AssignmentCurve>> {
    list.iter().map(|s| parse_curve(s)).collect()
}

fn split_pair(s: &str) -> Result<(&str, f64)> {
    let (k, v) = s
        .rsplit_once('=')
        .ok_or_else(|| anyhow!("expected KEY=NUMBER, got '{s}'"))?;
    let n = v
        .trim()
        .parse()
        .with_context(|| format!("invalid number in '{s}'"))?;
    Ok((k.trim(), n))
}

fn letter_pairs(pairs: &[String]) -> Result<Vec<(Letter, f64)>> {
    pairs
        .iter()
        .map(|p| {
            let (k, v) = split_pair(p)?;
            Ok((k.parse::<Letter>()?, v))
        })
        .collect()
}

/// `LETTER=NUMBER`, or `LETTER=none` to remove the cutoff.
fn cutoff_pairs(pairs: &[String]) -> Result<Vec<(Letter, Option<f64>)>> {
    pairs
        .iter()
        .map(|p| match p.rsplit_once('=') {
            Some((k, v)) if v.trim().eq_ignore_ascii_case("none") => {
                Ok((k.trim().parse::<Letter>()?, None))
            }
            _ => split_pair(p).and_then(|(k, v)| Ok((k.parse::<Letter>()?, Some(v)))),
        })
        .collect()
}

fn new_assignment(
    name: String,
    points: f64,
    score: Option<f64>,
    due: Option<chrono::NaiveDate>,
    status: Option<AssignmentStatus>,
    curve: &[String],
) -> Result<Assignment> {
    let mut a = Assignment::new(AssignmentId(Uuid::new_v4()), name, points);
    a.score = score;
    a.due = due;
    a.status = status.unwrap_or_default();
    a.curves = parse_curves(curve)?;
    Ok(a)
}

fn clearable<T>(value: Option<T>, clear: bool) -> Option<Option<T>> {
    if clear { Some(None) } else { value.map(Some) }
}

pub fn plan(gb: &Gradebook, top: Top) -> Result<Planned> {
    Ok(match top {
        Top::Term(c) => plan_term(gb, c)?,
        Top::Course(c) => plan_course(gb, c)?,
        Top::Category(c) => plan_category(gb, c)?,
        Top::Assignment(c) => plan_assignment(gb, c)?,
        Top::Solve(a) => plan_solve(gb, a)?,
        Top::Scenario(c) => plan_scenario(gb, c)?,
        Top::Gpa(c) => plan_gpa(gb, c)?,
        Top::Impact(a) => Planned::plain(Command::Impact {
            course_id: resolve_course(gb, &a.course)?,
            curve_mode: mode(a.no_curve),
            scenario_id: scenario_opt(gb, &a.scenario)?,
        }),
        Top::Trends(a) => Planned::plain(Command::Trends {
            course_id: resolve_course(gb, &a.course)?,
            curve_mode: mode(a.no_curve),
            scenario_id: scenario_opt(gb, &a.scenario)?,
        }),
        Top::Resolve(r) => Planned::plain(Command::Resolve {
            kind: r.kind,
            reference: r.reference,
            course: course_opt(gb, &r.course)?,
            scenario: scenario_opt(gb, &r.scenario)?,
        }),
        Top::Leverage(l) => Planned::plain(Command::Leverage {
            course_id: resolve_course(gb, &l.base.course)?,
            baseline: l.baseline,
            curve_mode: mode(l.base.no_curve),
            scenario_id: scenario_opt(gb, &l.base.scenario)?,
        }),
    })
}

fn plan_term(gb: &Gradebook, c: TermCmd) -> Result<Planned> {
    Ok(match c {
        TermCmd::Add { name } => {
            let id = TermId(Uuid::new_v4());
            Planned::plain(Command::AddTerm {
                id,
                name: name.clone(),
            })
        }
        TermCmd::List => Planned::plain(Command::ListTerms),
        TermCmd::Rename { term, name } => Planned::plain(Command::RenameTerm {
            id: resolve_term(gb, &term)?,
            name,
        }),
        TermCmd::Rm { term } => Planned::plain(Command::RemoveTerm {
            id: resolve_term(gb, &term)?,
        }),
    })
}

fn plan_course(gb: &Gradebook, c: CourseCmd) -> Result<Planned> {
    Ok(match c {
        CourseCmd::Add {
            term,
            name,
            credits,
            code,
            target,
        } => {
            let id = CourseId(Uuid::new_v4());
            Planned::plain(Command::AddCourse {
                term_id: resolve_term(gb, &term)?,
                id,
                name: name.clone(),
                code,
                credits,
                target,
            })
        }
        CourseCmd::Show { course, scenario } => Planned::plain(Command::ShowCourse {
            course_id: resolve_course(gb, &course)?,
            scenario_id: scenario_opt(gb, &scenario)?,
        }),
        CourseCmd::Update {
            course,
            name,
            code,
            clear_code,
            credits,
            target,
            clear_target,
            gpa_status,
            official_letter,
            clear_official_letter,
        } => Planned::plain(Command::UpdateCourse {
            id: resolve_course(gb, &course)?,
            name,
            code: clearable(code, clear_code),
            credits,
            target: clearable(target, clear_target),
            gpa_status,
            official_letter: clearable(official_letter, clear_official_letter),
        }),
        CourseCmd::Rm { course } => Planned::plain(Command::RemoveCourse {
            id: resolve_course(gb, &course)?,
        }),
        CourseCmd::Grade {
            course,
            no_curve,
            scenario,
        } => Planned::plain(Command::CourseGrade {
            course_id: resolve_course(gb, &course)?,
            curve_mode: mode(no_curve),
            scenario_id: scenario_opt(gb, &scenario)?,
        }),
        CourseCmd::Target {
            course,
            percent,
            clear,
        } => Planned::plain(Command::SetTarget {
            course_id: resolve_course(gb, &course)?,
            target: if clear { None } else { percent },
        }),
        CourseCmd::Curve {
            course,
            add_points,
            cutoff_shift,
            clear,
        } => {
            if !clear && add_points.is_none() && cutoff_shift.is_none() {
                bail!("give --add-points and/or --cutoff-shift, or --clear");
            }
            Planned::plain(Command::SetCourseCurve {
                course_id: resolve_course(gb, &course)?,
                add_points: if clear { Some(0.0) } else { add_points },
                cutoff_shift: if clear { Some(0.0) } else { cutoff_shift },
            })
        }
        CourseCmd::Cutoffs { course, pairs } => {
            let course_id = resolve_course(gb, &course)?;
            if pairs.is_empty() {
                Planned::plain(Command::ShowCourse {
                    course_id,
                    scenario_id: None,
                })
            } else {
                Planned {
                    commands: cutoff_pairs(&pairs)?
                        .into_iter()
                        .map(|(letter, value)| Command::SetCutoff {
                            course_id,
                            letter,
                            value,
                        })
                        .collect(),
                }
            }
        }
    })
}

fn plan_category(gb: &Gradebook, c: CategoryCmd) -> Result<Planned> {
    Ok(match c {
        CategoryCmd::Add {
            course,
            name,
            weight,
            drop_lowest,
        } => {
            let id = CategoryId(Uuid::new_v4());
            Planned::plain(Command::AddCategory {
                course_id: resolve_course(gb, &course)?,
                id,
                name: name.clone(),
                weight,
                drop_lowest,
            })
        }
        CategoryCmd::Update {
            course,
            category,
            name,
            weight,
            drop_lowest,
        } => {
            let course_id = resolve_course(gb, &course)?;
            Planned::plain(Command::UpdateCategory {
                id: resolve_category(gb, course_id, &category)?,
                name,
                weight,
                drop_lowest,
            })
        }
        CategoryCmd::Rm { course, category } => {
            let course_id = resolve_course(gb, &course)?;
            Planned::plain(Command::RemoveCategory {
                id: resolve_category(gb, course_id, &category)?,
            })
        }
    })
}

fn plan_assignment(gb: &Gradebook, c: AssignmentCmd) -> Result<Planned> {
    Ok(match c {
        AssignmentCmd::Add {
            course,
            category,
            name,
            points,
            score,
            due,
            status,
            curve,
        } => {
            let course_id = resolve_course(gb, &course)?;
            let category_id = resolve_category(gb, course_id, &category)?;
            let id = AssignmentId(Uuid::new_v4());
            Planned::plain(Command::AddAssignment {
                category_id,
                id,
                name: name.clone(),
                points_possible: points,
                score,
                due,
                status,
                curves: parse_curves(&curve)?,
            })
        }
        AssignmentCmd::Update {
            assignment,
            course,
            name,
            points,
            score,
            clear_score,
            due,
            clear_due,
            status,
            curve,
            clear_curves,
        } => {
            let scope = course_opt(gb, &course)?;
            let curves = if clear_curves {
                Some(Vec::new())
            } else if curve.is_empty() {
                None
            } else {
                Some(parse_curves(&curve)?)
            };
            Planned::plain(Command::UpdateAssignment {
                id: resolve_assignment(gb, scope, None, &assignment)?,
                name,
                points_possible: points,
                score: clearable(score, clear_score),
                due: clearable(due, clear_due),
                status,
                curves,
            })
        }
        AssignmentCmd::Rm { assignment, course } => {
            let scope = course_opt(gb, &course)?;
            Planned::plain(Command::RemoveAssignment {
                id: resolve_assignment(gb, scope, None, &assignment)?,
            })
        }
    })
}

fn plan_solve(gb: &Gradebook, a: SolveArgs) -> Result<Planned> {
    let course_id = resolve_course(gb, &a.course)?;
    let target = a
        .target
        .as_deref()
        .map(str::parse::<TargetSpec>)
        .transpose()?;
    let scenario_id = scenario_opt(gb, &a.scenario)?;
    let mut locked = Vec::new();
    for l in &a.lock {
        let (name, points) = split_pair(l)?;
        locked.push(LockedScore {
            assignment_id: resolve_assignment(gb, Some(course_id), scenario_id, name)?,
            points,
        });
    }
    Ok(Planned::plain(Command::SolveTarget {
        course_id,
        target,
        locked,
        with_curve: a.with_curve,
        scenario_id,
    }))
}

fn plan_scenario(gb: &Gradebook, c: ScenarioCmd) -> Result<Planned> {
    Ok(match c {
        ScenarioCmd::Create { name } => {
            let id = ScenarioId(Uuid::new_v4());
            Planned::plain(Command::CreateScenario {
                id,
                name: name.clone(),
            })
        }
        ScenarioCmd::Rename { scenario, name } => Planned::plain(Command::RenameScenario {
            id: resolve_scenario(gb, &scenario)?,
            name,
        }),
        ScenarioCmd::Rm { scenario } => Planned::plain(Command::DeleteScenario {
            id: resolve_scenario(gb, &scenario)?,
        }),
        ScenarioCmd::List => Planned::plain(Command::ListScenarios),
        ScenarioCmd::Override { scenario, change } => {
            let scenario_id = resolve_scenario(gb, &scenario)?;
            Planned::plain(Command::AddOverride {
                scenario_id,
                overlay: plan_override(gb, scenario_id, change)?,
            })
        }
        ScenarioCmd::Unoverride { scenario, index } => Planned::plain(Command::RemoveOverride {
            scenario_id: resolve_scenario(gb, &scenario)?,
            index,
        }),
        ScenarioCmd::Compare { scenarios } => Planned::plain(Command::CompareScenarios {
            scenario_ids: scenarios
                .iter()
                .map(|s| resolve_scenario(gb, s))
                .collect::<Result<_, _>>()?,
        }),
    })
}

fn plan_override(gb: &Gradebook, scenario: ScenarioId, c: OverrideCmd) -> Result<Override> {
    Ok(match c {
        OverrideCmd::SetScore {
            assignment,
            score,
            course,
        } => {
            let score = if score.trim().eq_ignore_ascii_case("none") {
                None
            } else {
                Some(
                    score
                        .trim()
                        .parse()
                        .with_context(|| format!("invalid score '{score}' (number or none)"))?,
                )
            };
            let scope = course_opt(gb, &course)?;
            Override::SetScore {
                assignment_id: resolve_assignment(gb, scope, Some(scenario), &assignment)?,
                score,
            }
        }
        OverrideCmd::SetStatus {
            assignment,
            status,
            course,
        } => {
            let scope = course_opt(gb, &course)?;
            Override::SetStatus {
                assignment_id: resolve_assignment(gb, scope, Some(scenario), &assignment)?,
                status,
            }
        }
        OverrideCmd::AddAssignment {
            course,
            category,
            name,
            points,
            score,
            due,
            status,
            curve,
        } => {
            let course_id = resolve_course(gb, &course)?;
            Override::AddAssignment {
                category_id: resolve_category(gb, course_id, &category)?,
                assignment: new_assignment(name, points, score, due, status, &curve)?,
            }
        }
        OverrideCmd::SetAssignmentCurves {
            assignment,
            curve,
            course,
        } => {
            let scope = course_opt(gb, &course)?;
            Override::SetAssignmentCurves {
                assignment_id: resolve_assignment(gb, scope, Some(scenario), &assignment)?,
                curves: parse_curves(&curve)?,
            }
        }
        OverrideCmd::SetCourseCurve {
            course,
            add_points,
            cutoff_shift,
        } => Override::SetCourseCurve {
            course_id: resolve_course(gb, &course)?,
            curve: CourseCurve {
                add_points,
                cutoff_shift,
            },
        },
        OverrideCmd::SetGpaStatus { course, gpa_status } => Override::SetGpaStatus {
            course_id: resolve_course(gb, &course)?,
            gpa_status,
        },
        OverrideCmd::SetOfficialLetter { course, letter } => Override::SetOfficialLetter {
            course_id: resolve_course(gb, &course)?,
            letter: if letter.trim().eq_ignore_ascii_case("none") {
                None
            } else {
                Some(letter.parse::<Letter>()?)
            },
        },
    })
}

fn plan_gpa(gb: &Gradebook, c: GpaCmd) -> Result<Planned> {
    Ok(match c {
        GpaCmd::Term { term, scenario } => Planned::plain(Command::TermGpa {
            term_id: resolve_term(gb, &term)?,
            scenario_id: scenario_opt(gb, &scenario)?,
        }),
        GpaCmd::Cumulative { scenario } => Planned::plain(Command::CumulativeGpa {
            scenario_id: scenario_opt(gb, &scenario)?,
        }),
        GpaCmd::Scale { pairs } => {
            if pairs.is_empty() {
                Planned::plain(Command::ShowGpaScale)
            } else {
                Planned {
                    commands: letter_pairs(&pairs)?
                        .into_iter()
                        .map(|(letter, points)| Command::SetGpaScaleEntry { letter, points })
                        .collect(),
                }
            }
        }
    })
}

/// Every core Command tag and the `grades` subcommand that plans it. The parity test below
/// fails if a Command is added to core without a row here (and so, a CLI path).
#[cfg(test)]
const CLI_PATHS: &[(&str, &str)] = &[
    ("add_term", "term add"),
    ("rename_term", "term rename"),
    ("remove_term", "term rm"),
    ("add_course", "course add"),
    ("update_course", "course update"),
    ("remove_course", "course rm"),
    ("add_category", "category add"),
    ("update_category", "category update"),
    ("remove_category", "category rm"),
    ("add_assignment", "assignment add"),
    ("update_assignment", "assignment update"),
    ("remove_assignment", "assignment rm"),
    ("set_course_curve", "course curve"),
    ("set_cutoffs", "course cutoffs"),
    ("set_cutoff", "course cutoffs"),
    ("list_terms", "term list"),
    ("show_course", "course show"),
    ("course_grade", "course grade"),
    ("impact", "impact"),
    ("trends", "trends"),
    ("leverage", "leverage"),
    ("set_target", "course target"),
    ("solve_target", "solve"),
    ("create_scenario", "scenario create"),
    ("rename_scenario", "scenario rename"),
    ("delete_scenario", "scenario rm"),
    ("add_override", "scenario override"),
    ("remove_override", "scenario unoverride"),
    ("list_scenarios", "scenario list"),
    ("compare_scenarios", "scenario compare"),
    ("term_gpa", "gpa term"),
    ("cumulative_gpa", "gpa cumulative"),
    ("set_gpa_scale", "gpa scale"),
    ("set_gpa_scale_entry", "gpa scale"),
    ("show_gpa_scale", "gpa scale"),
    ("resolve", "resolve"),
];

#[cfg(test)]
mod parity_tests {
    use super::*;

    #[test]
    fn every_core_command_has_a_cli_path() {
        for tag in Command::TAGS {
            assert!(
                CLI_PATHS.iter().any(|(t, _)| t == tag),
                "core command '{tag}' has no CLI path in CLI_PATHS"
            );
        }
        for (tag, path) in CLI_PATHS {
            assert!(
                Command::TAGS.contains(tag),
                "CLI_PATHS lists unknown command '{tag}' ({path})"
            );
        }
    }
}
