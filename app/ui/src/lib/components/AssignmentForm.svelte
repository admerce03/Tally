<script lang="ts">
  import { newId } from "../api";
  import { app, mutate, toast } from "../app.svelte";
  import { STATUSES, cap, curveToRow, parseNum, parseOptNum, rowsToCurves, type CurveRow } from "../format";
  import type { AssignmentStatus, AssignmentView, Override } from "../generated/output";
  import CurveList from "./CurveList.svelte";

  // Add mode: pass categoryId. Edit mode: pass assignment.
  let {
    categoryId,
    assignment,
    onclose,
  }: { categoryId?: string; assignment?: AssignmentView; onclose: () => void } = $props();

  function init() {
    return {
      name: assignment?.name ?? "",
      points: assignment ? String(assignment.points_possible) : "100",
      score: assignment?.score == null ? "" : String(assignment.score),
      due: assignment?.due ?? "",
      status: (assignment?.status ?? "pending") as AssignmentStatus,
      curves: (assignment?.curves ?? []).map(curveToRow) as CurveRow[],
    };
  }
  // In a scenario only score, status and curves of existing assignments can be overridden.
  const lockedFields = $derived(!!app.scenarioId && !!assignment);
  let f = $state(init());

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }

  async function submit(e: Event) {
    e.preventDefault();
    const name = f.name.trim();
    const points = parseNum(f.points);
    const score = parseOptNum(f.score);
    const curves = rowsToCurves(f.curves);
    if (!name) return toast("Name is required");
    if (points === undefined) return toast("Points possible must be a number");
    if (score === undefined) return toast("Score must be a number");
    if (curves === undefined) return toast("Curve values must be numbers");
    const due = f.due || null;
    const sid = app.scenarioId;
    let out: unknown;
    if (sid && assignment) {
      // Scenario mode: only score, status and curves can be overridden; send what changed.
      const overrides: Override[] = [];
      if (score !== (assignment.score ?? null))
        overrides.push({ type: "set_score", assignment_id: assignment.id, score });
      if (f.status !== (assignment.status ?? "pending"))
        overrides.push({ type: "set_status", assignment_id: assignment.id, status: f.status });
      if (JSON.stringify(curves) !== JSON.stringify(assignment.curves ?? []))
        overrides.push({ type: "set_assignment_curves", assignment_id: assignment.id, curves });
      out = true;
      for (const override of overrides) {
        out = await mutate({ command: "add_override", scenario_id: sid, override });
        if (!out) break;
      }
    } else if (sid) {
      out = await mutate({
        command: "add_override",
        scenario_id: sid,
        override: {
          type: "add_assignment",
          category_id: categoryId ?? "",
          assignment: { id: newId(), name, points_possible: points, score, due, status: f.status, curves },
        },
      });
    } else if (assignment) {
      out = await mutate({
        command: "update_assignment",
        id: assignment.id,
        name,
        points_possible: points,
        score,
        due,
        status: f.status,
        curves,
      });
    } else {
      out = await mutate({
        command: "add_assignment",
        category_id: categoryId ?? "",
        id: newId(),
        name,
        points_possible: points,
        score,
        due,
        status: f.status,
        curves,
      });
    }
    if (out) onclose();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form
  class="stack tight form"
  onsubmit={submit}
  onkeydown={(e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onclose();
    }
  }}
>
  <div class="cluster">
    <label class="field grow">
      <span>Name</span>
      <input use:focusOnMount bind:value={f.name} disabled={lockedFields} />
    </label>
    <label class="field">
      <span>Points possible</span>
      <input class="narrow" bind:value={f.points} inputmode="decimal" disabled={lockedFields} />
    </label>
    <label class="field">
      <span>Score (points)</span>
      <input class="narrow" bind:value={f.score} inputmode="decimal" placeholder="ungraded" />
    </label>
    <label class="field">
      <span>Due</span>
      <input type="date" bind:value={f.due} disabled={lockedFields} />
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
  {#if assignment}
    <div class="field">
      <span>Assignment curves (assumptions, scores stay unchanged)</span>
      <CurveList bind:rows={f.curves} />
    </div>
  {/if}
  <div class="cluster">
    <button type="submit" class="btn primary sm">{assignment ? "Save" : "Add assignment"}</button>
    <button type="button" class="btn ghost sm" onclick={onclose}>Cancel</button>
  </div>
</form>

<style>
  .form {
    padding: 12px 16px;
    background: var(--bg-side);
  }
</style>
