/* eslint-disable */
/**
 * GENERATED from core's JSON schema by `npm run gen:types`. Do not edit.
 */


export type Command = ({
command: "add_term"
id: string
name: string
} | {
command: "rename_term"
id: string
name: string
} | {
command: "set_term_year"
id: string
year?: (number | null)
} | {
command: "remove_term"
id: string
} | {
code?: (string | null)
command: "add_course"
credits: number
id: string
name: string
target?: (number | null)
term_id: string
} | {
code?: (string | null)
command: "update_course"
credits?: (number | null)
gpa_status?: (GpaStatus | null)
id: string
name?: (string | null)
official_letter?: (Letter | null)
target?: (number | null)
} | {
command: "remove_course"
id: string
} | {
command: "add_category"
course_id: string
drop_lowest?: number
id: string
name: string
weight: number
} | {
command: "update_category"
drop_lowest?: (number | null)
id: string
name?: (string | null)
weight?: (number | null)
} | {
command: "remove_category"
id: string
} | {
category_id: string
command: "add_assignment"
curves?: AssignmentCurve[]
due?: (string | null)
id: string
name: string
points_possible: number
score?: (number | null)
status?: (AssignmentStatus | null)
} | {
command: "update_assignment"
curves?: (AssignmentCurve[] | null)
due?: (string | null)
id: string
name?: (string | null)
points_possible?: (number | null)
score?: (number | null)
status?: (AssignmentStatus | null)
} | {
command: "remove_assignment"
id: string
} | {
add_points?: (number | null)
command: "set_course_curve"
course_id: string
cutoff_shift?: (number | null)
} | {
command: "set_cutoffs"
course_id: string
cutoffs: LetterCutoffs
} | {
command: "set_cutoff"
course_id: string
letter: Letter
value?: (number | null)
} | {
command: "list_terms"
} | {
command: "show_course"
course_id: string
scenario_id?: (string | null)
} | {
command: "course_grade"
course_id: string
curve_mode?: ("apply" | "ignore")
/**
 * Evaluate against this what-if scenario instead of the real data.
 */
scenario_id?: (string | null)
} | {
command: "impact"
course_id: string
curve_mode?: ("apply" | "ignore")
scenario_id?: (string | null)
} | {
command: "trends"
course_id: string
curve_mode?: ("apply" | "ignore")
scenario_id?: (string | null)
} | {
baseline?: (number | null)
command: "leverage"
course_id: string
curve_mode?: ("apply" | "ignore")
scenario_id?: (string | null)
} | {
command: "set_target"
course_id: string
target?: (number | null)
} | {
command: "solve_target"
course_id: string
locked?: LockedScore[]
scenario_id?: (string | null)
target?: (TargetSpec | null)
with_curve?: boolean
} | {
command: "create_scenario"
id: string
name: string
} | {
command: "rename_scenario"
id: string
name: string
} | {
command: "delete_scenario"
id: string
} | {
command: "add_override"
override: Override
scenario_id: string
} | {
command: "remove_override"
index: number
scenario_id: string
} | {
command: "list_scenarios"
} | {
command: "compare_scenarios"
scenario_ids?: string[]
} | {
command: "term_gpa"
scenario_id?: (string | null)
term_id: string
} | {
command: "cumulative_gpa"
scenario_id?: (string | null)
} | {
command: "set_gpa_scale"
scale: GpaScale
} | {
command: "set_gpa_scale_entry"
letter: Letter
points: number
} | {
command: "show_gpa_scale"
} | {
command: "import_gradebook"
gradebook: Gradebook
} | {
command: "export_gradebook"
} | {
command: "resolve"
course?: (string | null)
kind: EntityKind
reference: string
scenario?: (string | null)
})
export type GpaStatus = ("normal" | "pass_fail" | "withdrawn" | "retaken")
export type Letter = ("A+" | "A" | "A-" | "B+" | "B" | "B-" | "C+" | "C" | "C-" | "D+" | "D" | "F")
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
export type AssignmentStatus = ("pending" | "submitted" | "graded" | "missing" | "excused")
/**
 * A target grade: `{"percent": 90}` (0-100) or `{"letter": "A-"}` (uses the course cutoff).
 */
export type TargetSpec = ({
percent: number
} | {
letter: Letter
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
/**
 * What an Add* command created.
 */
export type EntityKind = ("term" | "course" | "category" | "assignment" | "scenario")

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
/**
 * A locked expected score: the assignment is assumed to earn `points` raw points (not a percent).
 */
export interface LockedScore {
assignment_id: string
points: number
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
/**
 * What-if scenario: named overlay of hypothetical changes (see `scenario::apply_scenario`).
 */
export interface Scenario {
id: string
name: string
overrides?: Override[]
}
export interface Term {
courses?: Course[]
id: string
name: string
/**
 * Academic year the term belongs to (e.g. 2026), set by the user; used to group and sort
 * terms. `None` when not set.
 */
year?: (number | null)
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
