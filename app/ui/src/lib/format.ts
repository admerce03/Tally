// Display-only helpers. No grade math lives here: only formatting and shape conversion.
import type { AssignmentCurve, AssignmentStatus, ExclusionReason, GpaStatus, Letter } from "./generated/output";

export const LETTERS: Letter[] = ["A+", "A", "A-", "B+", "B", "B-", "C+", "C", "C-", "D+", "D", "F"];
export const STATUSES: AssignmentStatus[] = ["pending", "submitted", "graded", "missing", "excused"];
export const GPA_STATUSES: GpaStatus[] = ["normal", "pass_fail", "withdrawn", "retaken"];

export const GPA_STATUS_LABEL: Record<GpaStatus, string> = {
  normal: "Counts toward GPA",
  pass_fail: "Pass/fail",
  withdrawn: "Withdrawn",
  retaken: "Retaken",
};

export const EXCLUSION_LABEL: Record<ExclusionReason, string> = {
  pass_fail: "Pass/fail",
  withdrawn: "Withdrawn",
  retaken: "Retaken",
  zero_credits: "Zero credits",
  no_grade: "No grade yet",
};

/** Percent trimmed to at most 1 decimal: 87.5%, 90%. */
export function pct(n: number | null | undefined): string {
  return n == null ? "—" : `${Number(n.toFixed(1))}%`;
}

/** "2026-10-03" -> "Oct 3" (adds the year when it is not the current one). Formatting only. */
export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  const opts: Intl.DateTimeFormatOptions =
    d.getFullYear() === new Date().getFullYear()
      ? { month: "short", day: "numeric" }
      : { month: "short", day: "numeric", year: "numeric" };
  return d.toLocaleDateString([], opts);
}

export function dec2(n: number | null | undefined): string {
  return n == null ? "—" : n.toFixed(2);
}

/** Grade-points / credits style number: trims trailing zeros. */
export function plain(n: number | null | undefined): string {
  return n == null ? "—" : String(Number(n.toFixed(4)));
}

/** A grade movement to 1 decimal as an arrow ("↑1.2", "↓0.8"), so it never reads as bonus points; null when it rounds to 0. */
export function movement(n: number | null | undefined): string | null {
  if (n == null) return null;
  const r = Number(n.toFixed(1));
  if (r === 0) return null;
  return r > 0 ? `↑${r}` : `↓${Math.abs(r)}`;
}

/** A year written in a term name ("Fall 2026" -> 2026), to prefill the year field. Display help only. */
export function guessYear(name: string): number | null {
  const m = /\b(19|20)\d{2}\b/.exec(name);
  return m ? Number(m[0]) : null;
}

export function cap(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** "" -> null (clear), valid number -> number, anything else -> undefined (invalid). */
export function parseOptNum(s: string): number | null | undefined {
  const t = s.trim();
  if (t === "") return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : undefined;
}

/** Required number: undefined when blank or invalid. */
export function parseNum(s: string): number | undefined {
  const v = parseOptNum(s);
  return v == null ? undefined : v;
}

export type CurveRow = { kind: "add_points" | "scale_top_to" | "set_max"; value: string };

export const CURVE_LABEL: Record<CurveRow["kind"], string> = {
  add_points: "Add points",
  scale_top_to: "Scale top score to 100%",
  set_max: "Set new max",
};

export function curveToRow(c: AssignmentCurve): CurveRow {
  if ("add_points" in c) return { kind: "add_points", value: String(c.add_points) };
  if ("scale_top_to" in c) return { kind: "scale_top_to", value: String(c.scale_top_to.top_score) };
  return { kind: "set_max", value: String(c.set_max) };
}

/** undefined when any row has an invalid number. */
export function rowsToCurves(rows: CurveRow[]): AssignmentCurve[] | undefined {
  const out: AssignmentCurve[] = [];
  for (const r of rows) {
    const v = parseNum(r.value);
    if (v === undefined) return undefined;
    if (r.kind === "add_points") out.push({ add_points: v });
    else if (r.kind === "scale_top_to") out.push({ scale_top_to: { top_score: v } });
    else out.push({ set_max: v });
  }
  return out;
}

export function describeCurve(c: AssignmentCurve): string {
  if ("add_points" in c) return `+${c.add_points} pts`;
  if ("scale_top_to" in c) return `top ${c.scale_top_to.top_score} = 100%`;
  return `max ${c.set_max}`;
}

/** Round UP to 1 decimal (a safe minimum), without a trailing ".0". Formatting only. */
export function ceil1(x: number): string {
  // The solver's binary search lands up to ~1e-6 above the exact answer; don't round that up.
  const v = Math.ceil(x * 10 - 1e-4) / 10;
  return Number.isInteger(v) ? String(v) : v.toFixed(1);
}
