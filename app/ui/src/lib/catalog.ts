// Display-only lookups built from list_terms + show_course: which course an id belongs to, and the
// names used to describe a what-if's changes. No grade math.
import { ask } from "./app.svelte";
import { GPA_STATUS_LABEL, plain } from "./format";
import type { Override, Scenario } from "./generated/output";

export type Catalog = {
  /** Every course, in term order. */
  courses: { id: string; name: string }[];
  /** Course, category or assignment id -> its course id. */
  courseOf: Map<string, string>;
  /** Assignment id -> name and points possible. */
  assignments: Map<string, { name: string; possible: number }>;
};

export async function loadCatalog(): Promise<Catalog> {
  const terms = (await ask({ command: "list_terms" }, "terms")).terms;
  const ids = terms.flatMap((t) => t.courses.map((c) => c.id));
  const full = await Promise.all(ids.map((id) => ask({ command: "show_course", course_id: id }, "course")));
  const cat: Catalog = { courses: [], courseOf: new Map(), assignments: new Map() };
  for (const { course } of full) {
    cat.courses.push({ id: course.id, name: course.name });
    cat.courseOf.set(course.id, course.id);
    for (const k of course.categories ?? []) {
      cat.courseOf.set(k.id, course.id);
      for (const a of k.assignments ?? []) {
        cat.courseOf.set(a.id, course.id);
        cat.assignments.set(a.id, { name: a.name, possible: a.points_possible });
      }
    }
  }
  return cat;
}

/** The course an override is about, if it still exists (hypothetical assignments via their category). */
export function overrideCourse(o: Override, cat: Catalog, s?: Scenario): string | undefined {
  switch (o.type) {
    case "set_score":
    case "set_status":
    case "set_assignment_curves": {
      const direct = cat.courseOf.get(o.assignment_id);
      if (direct) return direct;
      const added = s?.overrides?.find((x) => x.type === "add_assignment" && x.assignment.id === o.assignment_id);
      return added?.type === "add_assignment" ? cat.courseOf.get(added.category_id) : undefined;
    }
    case "add_assignment":
      return cat.courseOf.get(o.category_id);
    default:
      return cat.courseOf.get(o.course_id);
  }
}

/** Courses a what-if touches, in first-change order. */
export function scenarioCourses(s: Scenario, cat: Catalog): string[] {
  const out: string[] = [];
  for (const o of s.overrides ?? []) {
    const c = overrideCourse(o, cat, s);
    if (c && !out.includes(c)) out.push(c);
  }
  return out;
}

/** Short phrases for a what-if's changes: "Exam 3 → 96", "+ Quiz 5", "Curve +5". */
export function describeChanges(s: Scenario, cat: Catalog): string[] {
  const added = new Map<string, string>();
  for (const o of s.overrides ?? []) if (o.type === "add_assignment") added.set(o.assignment.id, o.assignment.name);
  const name = (id: string) => cat.assignments.get(id)?.name ?? added.get(id) ?? "Removed assignment";
  return (s.overrides ?? []).map((o) => {
    switch (o.type) {
      case "set_score":
        return `${name(o.assignment_id)} → ${o.score == null ? "—" : plain(o.score)}`;
      case "set_status":
        return `${name(o.assignment_id)} ${o.status}`;
      case "set_assignment_curves":
        return `${name(o.assignment_id)} curve`;
      case "add_assignment":
        return `+ ${o.assignment.name}${o.assignment.score == null ? "" : ` → ${plain(o.assignment.score)}`}`;
      case "set_course_curve":
        return `Curve +${plain(o.curve.add_points ?? 0)}`;
      case "set_gpa_status":
        return GPA_STATUS_LABEL[o.gpa_status];
      case "set_official_letter":
        return `Letter ${o.letter ?? "from grade"}`;
    }
  });
}
