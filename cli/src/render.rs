//! Human-readable rendering of core command outputs. Formatting only; no grade math.

use std::fmt::Write as _;

use tally_core::gpa::{CourseGpaEntry, ExclusionReason, TermGpaReport};
use tally_core::scenario::ScenarioColumn;
use tally_core::solver::SolveStatus;
use tally_core::{AssignmentStatus, CommandOutput, CourseView, GpaStatus, Letter};
use uuid::Uuid;

use crate::plan::short_id;

fn pct(v: f64) -> String {
    format!("{v:.2}%")
}

fn opt_pct(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_string(), pct)
}

fn opt_letter(l: Option<Letter>) -> &'static str {
    l.map_or("-", Letter::as_str)
}

fn opt_gpa(v: Option<f64>) -> String {
    v.map_or_else(|| "n/a".to_string(), |g| format!("{g:.2}"))
}

fn signed(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_string(), |d| format!("{d:+.2}"))
}

fn grade_cell(g: Option<f64>, l: Option<Letter>) -> String {
    match g {
        Some(g) => format!("{} {}", pct(g), opt_letter(l)),
        None => "-".to_string(),
    }
}

fn status_word(s: AssignmentStatus) -> &'static str {
    match s {
        AssignmentStatus::Pending => "pending",
        AssignmentStatus::Submitted => "submitted",
        AssignmentStatus::Graded => "graded",
        AssignmentStatus::Missing => "missing",
        AssignmentStatus::Excused => "excused",
    }
}

fn gpa_status_word(s: GpaStatus) -> &'static str {
    match s {
        GpaStatus::Normal => "normal",
        GpaStatus::PassFail => "pass-fail",
        GpaStatus::Withdrawn => "withdrawn",
        GpaStatus::Retaken => "retaken",
    }
}

fn exclusion_word(r: ExclusionReason) -> &'static str {
    match r {
        ExclusionReason::PassFail => "pass/fail",
        ExclusionReason::Withdrawn => "withdrawn",
        ExclusionReason::Retaken => "retaken",
        ExclusionReason::ZeroCredits => "zero credits",
        ExclusionReason::NoGrade => "no grade yet",
    }
}

fn id(u: Uuid) -> String {
    format!("[{}]", short_id(u))
}

/// Left-aligned columns separated by two spaces; trailing spaces trimmed.
fn table(header: &[&str], rows: &[Vec<String>]) -> String {
    let cols = header.len();
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(cols) {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let mut out = String::new();
    let mut line = |cells: &mut dyn Iterator<Item = &str>| {
        let mut s = String::new();
        for (i, cell) in cells.enumerate() {
            if i > 0 {
                s.push_str("  ");
            }
            let _ = write!(s, "{cell:<w$}", w = widths[i]);
        }
        out.push_str(s.trim_end());
        out.push('\n');
    };
    line(&mut header.iter().copied());
    for row in rows {
        line(&mut row.iter().map(String::as_str));
    }
    out.trim_end().to_string()
}

pub fn render(out: &CommandOutput) -> String {
    match out {
        CommandOutput::Done => "ok".to_string(),
        CommandOutput::Gradebook { gradebook } => {
            serde_json::to_string_pretty(gradebook).unwrap_or_else(|e| format!("error: {e}"))
        }
        CommandOutput::Created { kind, id: new_id } => {
            format!("added {} {}", kind.as_str(), id(*new_id))
        }
        CommandOutput::Resolved {
            kind,
            id: found,
            name,
        } => format!("{} {} ({name})", kind.as_str(), found),
        CommandOutput::Terms { terms } => {
            if terms.is_empty() {
                return "no terms".to_string();
            }
            let mut s = String::new();
            for t in terms {
                let year = t.year.map(|y| format!(" ({y})")).unwrap_or_default();
                let _ = writeln!(s, "{}{year} {}", t.name, id(t.id.0));
                for c in &t.courses {
                    let code = c
                        .code
                        .as_deref()
                        .map(|c| format!(" ({c})"))
                        .unwrap_or_default();
                    let _ = writeln!(s, "  {}{code}  {} cr  {}", c.name, c.credits, id(c.id.0));
                }
            }
            s.trim_end().to_string()
        }
        CommandOutput::Course { course } => render_course(course),
        CommandOutput::CourseGrade { report } => format!(
            "current    {}\nprojected  {}\ncurves     {}",
            grade_cell(report.current, report.current_letter),
            grade_cell(report.projected, report.projected_letter),
            match report.curve_mode {
                tally_core::CurveMode::Apply => "applied",
                tally_core::CurveMode::Ignore => "ignored",
            }
        ),
        CommandOutput::Solve { solve } => {
            let n = solve.remaining_count;
            let head = format!("target {}", pct(solve.target_percent));
            let body = match solve.status {
                SolveStatus::Secured => {
                    "secured: target is met even if all remaining work scores 0%".to_string()
                }
                SolveStatus::OnTrack { needed } => {
                    let end = solve
                        .grade_at_needed
                        .map(|g| format!(" (final grade {})", pct(g)))
                        .unwrap_or_default();
                    format!(
                        "on track: need {} on each of {n} remaining assignment(s){end}",
                        pct(needed)
                    )
                }
                SolveStatus::Unreachable { needed: Some(x) } => format!(
                    "unreachable: would need {} on each of {n} remaining assignment(s)",
                    pct(x)
                ),
                SolveStatus::Unreachable { needed: None } if n == 0 => {
                    "unreachable: no remaining work and the target is not met".to_string()
                }
                SolveStatus::Unreachable { needed: None } => {
                    "unreachable: not reachable even with a very high score".to_string()
                }
            };
            format!("{head}\n{body}")
        }
        CommandOutput::Impact { entries } => {
            if entries.is_empty() {
                return "no graded assignments".to_string();
            }
            let rows: Vec<Vec<String>> = entries
                .iter()
                .map(|e| {
                    vec![
                        e.name.clone(),
                        e.due.map_or_else(|| "-".to_string(), |d| d.to_string()),
                        opt_pct(e.grade_before),
                        opt_pct(e.grade_after),
                        signed(e.delta),
                    ]
                })
                .collect();
            table(&["assignment", "due", "before", "after", "change"], &rows)
        }
        CommandOutput::Trends { points } => {
            if points.is_empty() {
                return "no graded assignments".to_string();
            }
            let rows: Vec<Vec<String>> = points
                .iter()
                .map(|p| {
                    vec![
                        p.date
                            .map_or_else(|| "undated".to_string(), |d| d.to_string()),
                        pct(p.grade),
                    ]
                })
                .collect();
            table(&["date", "grade"], &rows)
        }
        CommandOutput::Leverage { report } => {
            if report.entries.is_empty() {
                return "no remaining assignments".to_string();
            }
            let rows: Vec<Vec<String>> = report
                .entries
                .iter()
                .map(|e| vec![e.name.clone(), format!("{:+.2}", e.leverage)])
                .collect();
            format!(
                "baseline {} (final-grade points per 10% scored)\n{}",
                pct(report.baseline_used),
                table(&["assignment", "leverage"], &rows)
            )
        }
        CommandOutput::Scenarios { scenarios } => {
            if scenarios.is_empty() {
                return "no scenarios".to_string();
            }
            let mut s = String::new();
            for sc in scenarios {
                let _ = writeln!(
                    s,
                    "{} {}  {} override(s)",
                    sc.name,
                    id(sc.id.0),
                    sc.overrides.len()
                );
                for (i, o) in sc.overrides.iter().enumerate() {
                    let json = serde_json::to_string(o).unwrap_or_default();
                    let _ = writeln!(s, "  {i}: {json}");
                }
            }
            s.trim_end().to_string()
        }
        CommandOutput::Comparison { columns } => render_comparison(columns),
        CommandOutput::TermGpa { report } => render_term_gpa(report),
        CommandOutput::CumulativeGpa { report } => {
            let mut parts: Vec<String> = report.terms.iter().map(render_term_gpa).collect();
            parts.push(format!(
                "cumulative GPA {}  ({} credits)",
                opt_gpa(report.gpa),
                report.credits
            ));
            parts.join("\n\n")
        }
        CommandOutput::GpaScale { scale } => {
            let rows: Vec<Vec<String>> = scale
                .points
                .iter()
                .map(|e| vec![e.letter.as_str().to_string(), format!("{:.2}", e.points)])
                .collect();
            table(&["letter", "points"], &rows)
        }
    }
}

fn render_course(c: &CourseView) -> String {
    let mut s = String::new();
    let code = c
        .code
        .as_deref()
        .map(|c| format!(" ({c})"))
        .unwrap_or_default();
    let _ = writeln!(s, "{}{code} {}", c.name, id(c.id.0));
    let _ = writeln!(
        s,
        "credits {}  target {}  gpa status {}  official letter {}",
        c.credits,
        opt_pct(c.target),
        gpa_status_word(c.gpa_status),
        opt_letter(c.official_letter)
    );
    let _ = writeln!(
        s,
        "curve: {:+} points, cutoffs shifted {}",
        c.curve.add_points, c.curve.cutoff_shift
    );
    let cutoffs: Vec<String> = c
        .cutoffs
        .entries
        .iter()
        .map(|e| format!("{} {}", e.letter.as_str(), e.min))
        .collect();
    let _ = writeln!(s, "cutoffs: {}", cutoffs.join(" | "));
    for cat in &c.categories {
        let _ = writeln!(
            s,
            "{}  weight {}  drop lowest {}  {}",
            cat.name,
            cat.weight,
            cat.drop_lowest,
            id(cat.id.0)
        );
        for av in &cat.assignments {
            let a = &av.assignment;
            let score = a.score.map_or_else(|| "-".to_string(), |v| v.to_string());
            let due = a.due.map(|d| format!("  due {d}")).unwrap_or_default();
            let curves = if a.curves.is_empty() {
                String::new()
            } else {
                let list: Vec<String> = a
                    .curves
                    .iter()
                    .map(|c| match c {
                        tally_core::AssignmentCurve::AddPoints(n) => format!("add:{n}"),
                        tally_core::AssignmentCurve::ScaleTopTo { top_score } => {
                            format!("top:{top_score}")
                        }
                        tally_core::AssignmentCurve::SetMax(n) => format!("max:{n}"),
                    })
                    .collect();
                format!("  curves {}", list.join(","))
            };
            let mut tags = String::new();
            if av.dropped {
                tags.push_str("  [dropped]");
            }
            if av.hypothetical {
                tags.push_str("  [hypothetical]");
            }
            let _ = writeln!(
                s,
                "  {}  {score}/{}{due}  {}{curves}{tags}  {}",
                a.name,
                a.points_possible,
                status_word(a.status),
                id(a.id.0)
            );
        }
    }
    s.trim_end().to_string()
}

fn gpa_row(c: &CourseGpaEntry) -> Vec<String> {
    let note = match (c.included, c.exclusion) {
        (true, _) => "counted".to_string(),
        (false, Some(r)) => format!("excluded: {}", exclusion_word(r)),
        (false, None) => "excluded".to_string(),
    };
    vec![
        c.name.clone(),
        opt_letter(c.letter).to_string(),
        c.points
            .map_or_else(|| "-".to_string(), |p| format!("{p:.2}")),
        c.credits.to_string(),
        note,
    ]
}

fn render_term_gpa(r: &TermGpaReport) -> String {
    let rows: Vec<Vec<String>> = r.courses.iter().map(gpa_row).collect();
    let mut s = format!(
        "{}: GPA {}  ({} credits)",
        r.name,
        opt_gpa(r.gpa),
        r.credits
    );
    if !rows.is_empty() {
        s.push('\n');
        s.push_str(&table(
            &["course", "letter", "points", "credits", ""],
            &rows,
        ));
    }
    s
}

fn render_comparison(columns: &[ScenarioColumn]) -> String {
    let Some(base) = columns.first() else {
        return "nothing to compare".to_string();
    };
    let mut header: Vec<String> = vec![String::new()];
    header.extend(columns.iter().map(|c| c.name.clone()));
    let header_refs: Vec<&str> = header.iter().map(String::as_str).collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for course in &base.courses {
        let mut row = vec![format!("{} (current / projected)", course.name)];
        for col in columns {
            row.push(
                match col.courses.iter().find(|c| c.course_id == course.course_id) {
                    Some(c) => format!(
                        "{} / {}",
                        grade_cell(c.current, c.current_letter),
                        grade_cell(c.projected, c.projected_letter)
                    ),
                    None => "-".to_string(),
                },
            );
        }
        rows.push(row);
    }
    for term in &base.terms {
        let mut row = vec![format!("{} GPA", term.name)];
        for col in columns {
            row.push(match col.terms.iter().find(|t| t.term_id == term.term_id) {
                Some(t) => opt_gpa(t.gpa),
                None => "-".to_string(),
            });
        }
        rows.push(row);
    }
    let mut cum = vec!["cumulative GPA".to_string()];
    cum.extend(columns.iter().map(|c| opt_gpa(c.cumulative_gpa)));
    rows.push(cum);
    let mut s = table(&header_refs, &rows);
    for col in columns {
        if let Some(e) = &col.error {
            let _ = write!(s, "\n{}: error: {e}", col.name);
        }
    }
    s
}
