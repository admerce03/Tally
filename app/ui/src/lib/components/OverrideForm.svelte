<script lang="ts">
  import { newId } from "../api";
  import { mutate, toast } from "../app.svelte";
  import {
    GPA_STATUSES,
    GPA_STATUS_LABEL,
    LETTERS,
    STATUSES,
    cap,
    parseNum,
    parseOptNum,
    rowsToCurves,
    type CurveRow,
  } from "../format";
  import type { AssignmentStatus, GpaStatus, Letter, Override } from "../generated/output";
  import type { Catalog } from "../catalog";
  import CurveList from "./CurveList.svelte";

  let { scenarioId, catalog }: { scenarioId: string; catalog: Catalog } = $props();

  type Kind = Override["type"];
  const KINDS: { value: Kind; label: string }[] = [
    { value: "set_score", label: "Set a score" },
    { value: "set_status", label: "Set an assignment status" },
    { value: "add_assignment", label: "Add a hypothetical assignment" },
    { value: "set_assignment_curves", label: "Set assignment curves" },
    { value: "set_course_curve", label: "Set course curve" },
    { value: "set_gpa_status", label: "Set GPA status" },
    { value: "set_official_letter", label: "Set official letter" },
  ];

  function init() {
    return {
      kind: "set_score" as Kind,
      assignmentId: catalog.assignments[0]?.id ?? "",
      categoryId: catalog.categories[0]?.id ?? "",
      courseId: catalog.courses[0]?.id ?? "",
      score: "",
      status: "graded" as AssignmentStatus,
      name: "",
      points: "100",
      due: "",
      curves: [] as CurveRow[],
      addPoints: "0",
      shift: "0",
      gpaStatus: "pass_fail" as GpaStatus,
      letter: "" as Letter | "",
    };
  }
  let f = $state(init());

  function build(): Override | string {
    switch (f.kind) {
      case "set_score": {
        const score = parseOptNum(f.score);
        if (!f.assignmentId) return "Pick an assignment";
        if (score === undefined) return "Score must be a number";
        return { type: "set_score", assignment_id: f.assignmentId, score };
      }
      case "set_status":
        if (!f.assignmentId) return "Pick an assignment";
        return { type: "set_status", assignment_id: f.assignmentId, status: f.status };
      case "add_assignment": {
        const points = parseNum(f.points);
        const score = parseOptNum(f.score);
        if (!f.categoryId) return "Pick a category";
        if (!f.name.trim()) return "Name is required";
        if (points === undefined) return "Points possible must be a number";
        if (score === undefined) return "Score must be a number";
        return {
          type: "add_assignment",
          category_id: f.categoryId,
          assignment: {
            id: newId(),
            name: f.name.trim(),
            points_possible: points,
            score,
            due: f.due || null,
            status: f.status,
            curves: [],
          },
        };
      }
      case "set_assignment_curves": {
        const curves = rowsToCurves(f.curves);
        if (!f.assignmentId) return "Pick an assignment";
        if (curves === undefined) return "Curve values must be numbers";
        return { type: "set_assignment_curves", assignment_id: f.assignmentId, curves };
      }
      case "set_course_curve": {
        const add = parseNum(f.addPoints);
        const shift = parseNum(f.shift);
        if (!f.courseId) return "Pick a course";
        if (add === undefined || shift === undefined) return "Curve values must be numbers";
        return {
          type: "set_course_curve",
          course_id: f.courseId,
          curve: { add_points: add, cutoff_shift: shift },
        };
      }
      case "set_gpa_status":
        if (!f.courseId) return "Pick a course";
        return { type: "set_gpa_status", course_id: f.courseId, gpa_status: f.gpaStatus };
      case "set_official_letter":
        if (!f.courseId) return "Pick a course";
        return { type: "set_official_letter", course_id: f.courseId, letter: f.letter || null };
    }
  }

  async function submit(e: Event) {
    e.preventDefault();
    const built = build();
    if (typeof built === "string") return toast(built);
    await mutate({ command: "add_override", scenario_id: scenarioId, override: built });
  }

  const needsAssignment = $derived(
    f.kind === "set_score" || f.kind === "set_status" || f.kind === "set_assignment_curves",
  );
  const needsCourse = $derived(
    f.kind === "set_course_curve" || f.kind === "set_gpa_status" || f.kind === "set_official_letter",
  );
</script>

<form class="stack tight" onsubmit={submit}>
  <label class="field">
    <span>Add override</span>
    <select bind:value={f.kind}>
      {#each KINDS as k (k.value)}
        <option value={k.value}>{k.label}</option>
      {/each}
    </select>
  </label>

  {#if needsAssignment}
    <label class="field">
      <span>Assignment</span>
      <select bind:value={f.assignmentId}>
        {#each catalog.assignments as a (a.id)}
          <option value={a.id}>{a.label}</option>
        {/each}
      </select>
    </label>
  {/if}
  {#if needsCourse}
    <label class="field">
      <span>Course</span>
      <select bind:value={f.courseId}>
        {#each catalog.courses as c (c.id)}
          <option value={c.id}>{c.label}</option>
        {/each}
      </select>
    </label>
  {/if}

  {#if f.kind === "set_score"}
    <label class="field">
      <span>Score in points (blank = ungraded)</span>
      <input class="mid" bind:value={f.score} inputmode="decimal" />
    </label>
  {:else if f.kind === "set_status"}
    <label class="field">
      <span>Status</span>
      <select bind:value={f.status}>
        {#each STATUSES as s (s)}
          <option value={s}>{cap(s)}</option>
        {/each}
      </select>
    </label>
  {:else if f.kind === "add_assignment"}
    <label class="field">
      <span>Category</span>
      <select bind:value={f.categoryId}>
        {#each catalog.categories as c (c.id)}
          <option value={c.id}>{c.label}</option>
        {/each}
      </select>
    </label>
    <div class="cluster">
      <label class="field grow">
        <span>Name</span>
        <input bind:value={f.name} />
      </label>
      <label class="field">
        <span>Points possible</span>
        <input class="narrow" bind:value={f.points} inputmode="decimal" />
      </label>
      <label class="field">
        <span>Score</span>
        <input class="narrow" bind:value={f.score} inputmode="decimal" placeholder="ungraded" />
      </label>
      <label class="field">
        <span>Due</span>
        <input type="date" bind:value={f.due} />
      </label>
      <label class="field">
        <span>Status</span>
        <select bind:value={f.status}>
          {#each STATUSES as s (s)}
            <option value={s}>{cap(s)}</option>
          {/each}
        </select>
      </label>
    </div>
  {:else if f.kind === "set_assignment_curves"}
    <div class="field">
      <span>Curves (replaces the assignment's curves in this scenario)</span>
      <CurveList bind:rows={f.curves} />
    </div>
  {:else if f.kind === "set_course_curve"}
    <div class="cluster">
      <label class="field">
        <span>Add points</span>
        <input class="narrow" bind:value={f.addPoints} inputmode="decimal" />
      </label>
      <label class="field">
        <span>Cutoff shift</span>
        <input class="narrow" bind:value={f.shift} inputmode="decimal" />
      </label>
    </div>
  {:else if f.kind === "set_gpa_status"}
    <label class="field">
      <span>GPA status</span>
      <select bind:value={f.gpaStatus}>
        {#each GPA_STATUSES as s (s)}
          <option value={s}>{GPA_STATUS_LABEL[s]}</option>
        {/each}
      </select>
    </label>
  {:else}
    <label class="field">
      <span>Official letter</span>
      <select bind:value={f.letter}>
        <option value="">Computed from grade</option>
        {#each LETTERS as l (l)}
          <option value={l}>{l}</option>
        {/each}
      </select>
    </label>
  {/if}

  <div><button type="submit" class="btn primary">Add override</button></div>
</form>
