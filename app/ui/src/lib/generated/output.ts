/* eslint-disable */
/**
 * GENERATED from core's JSON schema by `npm run gen:types`. Do not edit.
 */


export type CommandOutput = ({
result: "done"
} | {
id: string
kind: EntityKind
result: "created"
} | {
result: "terms"
terms: TermSummary[]
} | {
course: CourseView
result: "course"
} | {
report: CourseGradeReport
result: "course_grade"
} | {
result: "solve"
solve: SolveResult
} | {
entries: ImpactEntry[]
result: "impact"
} | {
points: TrendPoint[]
result: "trends"
} | {
report: LeverageReport
result: "leverage"
} | {
result: "scenarios"
scenarios: Scenario[]
} | {
columns: ScenarioColumn[]
result: "comparison"
} | {
report: TermGpaReport
result: "term_gpa"
} | {
report: CumulativeGpaReport
result: "cumulative_gpa"
} | {
result: "gpa_scale"
scale: GpaScale
} | {
gradebook: Gradebook
result: "gradebook"
} | {
id: string
kind: EntityKind
name: string
result: "resolved"
})
/**
 * What an Add* command created.
 */
export type EntityKind = ("term" | "course" | "category" | "assignment" | "scenario")
/**
 * A curve is an assumption layered on the stored score; it never mutates it.
 */
export type AssignmentCurve = ({
add_points: number
} | {
scale_top_to: {
top_score: number
}
} | {
set_max: number
})
export type Letter = ("A+" | "A" | "A-" | "B+" | "B" | "B-" | "C+" | "C" | "C-" | "D+" | "D" | "F")
export type GpaStatus = ("normal" | "pass_fail" | "withdrawn" | "retaken")
export type CurveMode = ("apply" | "ignore")
export type SolveStatus = ({
state: "secured"
} | {
needed: number
state: "on_track"
} | {
needed?: (number | null)
state: "unreachable"
})
export type Override = ({
assignment_id: string
score?: (number | null)
type: "set_score"
} | {
assignment_id: string
status: AssignmentStatus
type: "set_status"
} | {
assignment: Assignment
category_id: string
type: "add_assignment"
} | {
assignment_id: string
curves: AssignmentCurve[]
type: "set_assignment_curves"
} | {
course_id: string
curve: CourseCurve
type: "set_course_curve"
} | {
course_id: string
gpa_status: GpaStatus
type: "set_gpa_status"
} | {
course_id: string
letter?: (Letter | null)
type: "set_official_letter"
})
export type AssignmentStatus = ("pending" | "submitted" | "graded" | "missing" | "excused")
export type ExclusionReason = ("pass_fail" | "withdrawn" | "retaken" | "zero_credits" | "no_grade")

export interface TermSummary {
courses: CourseSummary[]
id: string
name: string
}
export interface CourseSummary {
code?: (string | null)
credits: number
id: string
name: string
}
/**
 * A course with derived flags on every assignment (output of `show_course`).
 */
export interface CourseView {
categories: CategoryView[]
code?: (string | null)
credits: number
curve: CourseCurve
cutoffs: LetterCutoffs
gpa_status: GpaStatus
id: string
name: string
official_letter?: (Letter | null)
target?: (number | null)
}
export interface CategoryView {
assignments: AssignmentView[]
drop_lowest: number
id: string
name: string
weight: number
}
/**
 * An assignment plus flags derived by core, so adapters never re-implement grade rules.
 */
export interface AssignmentView {
/**
 * Counted in the current grade (scored, or missing = 0; never excused).
 */
counts_as_graded: boolean
curves?: AssignmentCurve[]
/**
 * Dropped by the category's drop-lowest rule in the current grade (curves applied).
 */
dropped: boolean
due?: (string | null)
/**
 * Added by the scenario being viewed (never true without a scenario).
 */
hypothetical: boolean
id: string
name: string
points_possible: number
/**
 * Unscored work that still counts as remaining (solver, leverage).
 */
remaining: boolean
score?: (number | null)
status?: ("pending" | "submitted" | "graded" | "missing" | "excused")
}
export interface CourseCurve {
/**
 * Added to the final percent.
 */
add_points?: number
/**
 * Subtracted from every letter cutoff.
 */
cutoff_shift?: number
}
/**
 * Minimum percent for each letter. Letters without an entry are never awarded (F is the fallback).
 */
export interface LetterCutoffs {
entries: CutoffEntry[]
}
/**
 * One letter's minimum percent.
 */
export interface CutoffEntry {
letter: Letter
min: number
}
export interface CourseGradeReport {
/**
 * How the current grade is made up, per category (course order).
 */
categories: CategoryBreakdown[]
course_id: string
/**
 * Renormalized over categories with graded work; `None` if no graded work.
 */
current?: (number | null)
current_letter?: (Letter | null)
curve_mode: CurveMode
/**
 * Full weights, remaining work as 0.
 */
projected?: (number | null)
projected_letter?: (Letter | null)
}
/**
 * Per-category part of the current grade.
 */
export interface CategoryBreakdown {
/**
 * Current category average (percent) after curves and drops; `None` without graded work.
 */
average?: (number | null)
category_id: string
/**
 * Graded assignments dropped by drop-lowest.
 */
dropped: string[]
/**
 * Share (percent 0-100) of the current grade this category carries after renormalizing
 * over categories with graded work; 0 if it is excluded (no graded work, or weight 0).
 */
effective_weight: number
name: string
/**
 * Configured weight.
 */
weight: number
}
export interface SolveResult {
/**
 * Final grade when remaining work scores `needed` (Secured: grade at 0%).
 */
grade_at_needed?: (number | null)
/**
 * Ungraded, non-excused, non-locked assignments with points possible > 0.
 */
remaining_count: number
status: SolveStatus
target_percent: number
}
export interface ImpactEntry {
assignment_id: string
delta?: (number | null)
due?: (string | null)
grade_after?: (number | null)
/**
 * Running grade before this assignment was graded; `None` for the first graded work.
 */
grade_before?: (number | null)
name: string
}
export interface TrendPoint {
/**
 * `None` is the single trailing point for undated work.
 */
date?: (string | null)
grade: number
}
export interface LeverageReport {
baseline_used: number
entries: LeverageEntry[]
}
export interface LeverageEntry {
assignment_id: string
category_id: string
/**
 * Final-grade points gained per 10% scored on this assignment.
 */
leverage: number
name: string
}
/**
 * What-if scenario: named overlay of hypothetical changes (see `scenario::apply_scenario`).
 */
export interface Scenario {
id: string
name: string
overrides?: Override[]
}
export interface Assignment {
curves?: AssignmentCurve[]
due?: (string | null)
id: string
name: string
points_possible: number
score?: (number | null)
status?: ("pending" | "submitted" | "graded" | "missing" | "excused")
}
/**
 * One column of a comparison: the real data (`scenario_id: None`) or a scenario.
 */
export interface ScenarioColumn {
courses: CompareCourse[]
cumulative_gpa?: (number | null)
/**
 * Set when the scenario failed to apply; the result fields are then empty/None.
 */
error?: (string | null)
/**
 * "real" for the baseline, else the scenario name.
 */
name: string
scenario_id?: (string | null)
terms: CompareTerm[]
}
export interface CompareCourse {
course_id: string
current?: (number | null)
current_letter?: (Letter | null)
name: string
projected?: (number | null)
projected_letter?: (Letter | null)
}
export interface CompareTerm {
gpa?: (number | null)
name: string
term_id: string
}
export interface TermGpaReport {
courses: CourseGpaEntry[]
credits: number
/**
 * `None` when no course has eligible credits.
 */
gpa?: (number | null)
name: string
term_id: string
}
export interface CourseGpaEntry {
course_id: string
credits: number
exclusion?: (ExclusionReason | null)
included: boolean
letter?: (Letter | null)
name: string
points?: (number | null)
}
export interface CumulativeGpaReport {
credits: number
gpa?: (number | null)
terms: TermGpaReport[]
}
/**
 * Letter to grade points (editable).
 */
export interface GpaScale {
points: GpaEntry[]
}
/**
 * Grade points for one letter.
 */
export interface GpaEntry {
letter: Letter
points: number
}
export interface Gradebook {
scale?: GpaScale1
scenarios?: Scenario[]
terms?: Term[]
}
/**
 * Letter to grade points (editable).
 */
export interface GpaScale1 {
points: GpaEntry[]
}
export interface Term {
courses?: Course[]
id: string
name: string
}
export interface Course {
categories?: Category[]
code?: (string | null)
credits: number
curve?: CourseCurve1
cutoffs?: LetterCutoffs1
gpa_status?: ("normal" | "pass_fail" | "withdrawn" | "retaken")
id: string
name: string
official_letter?: (Letter | null)
/**
 * Target final grade as a percent (0-100).
 */
target?: (number | null)
}
export interface Category {
assignments?: Assignment[]
drop_lowest?: number
id: string
name: string
weight: number
}
export interface CourseCurve1 {
/**
 * Added to the final percent.
 */
add_points?: number
/**
 * Subtracted from every letter cutoff.
 */
cutoff_shift?: number
}
/**
 * Minimum percent for each letter. Letters without an entry are never awarded (F is the fallback).
 */
export interface LetterCutoffs1 {
entries: CutoffEntry[]
}
