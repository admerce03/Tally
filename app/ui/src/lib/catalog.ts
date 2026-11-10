// Display-only lookup of ids to human labels, built from list_terms + show_course.
import { ask } from "./app.svelte";
import { describeCurve, GPA_STATUS_LABEL, plain } from "./format";
import type { Override } from "./generated/output";

export type Catalog = {
  courses: { id: string; label: string }[];
  categories: { id: string; label: string }[];
  assignments: { id: string; label: string }[];
  names: Map<string, string>;
};

export async function loadCatalog(): Promise<Catalog> {
  const terms = (await ask({ command: "list_terms" }, "terms")).terms;
  const ids = terms.flatMap((t) => t.courses.map((c) => c.id));
  const full = await Promise.all(ids.map((id) => ask({ command: "show_course", course_id: id }, "course")));
  const cat: Catalog = { courses: [], categories: [], assignments: [], names: new Map() };
  for (const { course } of full) {
    cat.courses.push({ id: course.id, label: course.name });
    cat.names.set(course.id, course.name);
    for (const k of course.categories ?? []) {
      const label = `${course.name} / ${k.name}`;
      cat.categories.push({ id: k.id, label });
      cat.names.set(k.id, label);
      for (const a of k.assignments ?? []) {
        const al = `${course.name} / ${a.name}`;
        cat.assignments.push({ id: a.id, label: al });
        cat.names.set(a.id, al);
      }
    }
  }
  return cat;
}

export function describeOverride(o: Override, cat: Catalog | null): string {
  const n = (id: string) => cat?.names.get(id) ?? `${id.slice(0, 8)} (missing)`;
  switch (o.type) {
    case "set_score":
      return `Score of ${n(o.assignment_id)} = ${o.score == null ? "ungraded" : `${plain(o.score)} pts`}`;
    case "set_status":
      return `Status of ${n(o.assignment_id)} = ${o.status}`;
    case "add_assignment":
      return `Add "${o.assignment.name}" to ${n(o.category_id)} (${o.assignment.score == null ? "ungraded" : `${plain(o.assignment.score)} pts`} of ${plain(o.assignment.points_possible)})`;
    case "set_assignment_curves":
      return `Curves on ${n(o.assignment_id)}: ${o.curves.length === 0 ? "none" : o.curves.map(describeCurve).join(", ")}`;
    case "set_course_curve":
      return `Course curve on ${n(o.course_id)}: +${plain(o.curve.add_points ?? 0)} pts, cutoffs -${plain(o.curve.cutoff_shift ?? 0)}`;
    case "set_gpa_status":
      return `GPA status of ${n(o.course_id)} = ${GPA_STATUS_LABEL[o.gpa_status]}`;
    case "set_official_letter":
      return `Official letter of ${n(o.course_id)} = ${o.letter ?? "computed"}`;
  }
}
