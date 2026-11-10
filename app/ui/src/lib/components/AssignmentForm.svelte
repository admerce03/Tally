<script lang="ts">
  import { newId } from "../api";
  import { app, mutate, toast } from "../app.svelte";
  import { STATUSES, cap, curveToRow, parseNum, parseOptNum, rowsToCurves, type CurveRow } from "../format";
  import type { AssignmentStatus, AssignmentView, Override } from "../generated/output";
  import Alert from "./Alert.svelte";
  import CurveList from "./CurveList.svelte";

  // Add mode: pass categoryId. Edit mode: pass assignment. Rendered as an in-place row editor.
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
  let confirmDelete = $state(false);

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    // bind:value fills the field after this action runs (and resets the caret); select after it has.
    setTimeout(() => node.select(), 0);
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

  async function remove() {
    confirmDelete = false;
    if (assignment && (await mutate({ command: "remove_assignment", id: assignment.id }))) onclose();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form
  class="editor"
  onsubmit={submit}
  onkeydown={(e) => {
    if (e.key === "Escape" && !confirmDelete) {
      e.stopPropagation();
      onclose();
    }
  }}
>
  <div class="editor-grid">
    <label class="field wide">
      <span>Name</span>
      <input use:focusOnMount bind:value={f.name} disabled={lockedFields} />
    </label>
    <label class="field">
      <span>Points possible</span>
      <input bind:value={f.points} inputmode="decimal" disabled={lockedFields} />
    </label>
    {#if !assignment}
      <label class="field">
        <span>Score (points)</span>
        <input bind:value={f.score} inputmode="decimal" placeholder="not graded" />
      </label>
    {/if}
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
    <div class="field wide">
      <span title="Assumptions: the stored score never changes">Curves</span>
      <CurveList bind:rows={f.curves} />
    </div>
  </div>
  <div class="editor-foot">
    {#if assignment && !app.scenarioId}
      <button type="button" class="btn destructive lead" onclick={() => (confirmDelete = true)}>Delete</button>
    {/if}
    <button type="button" class="btn" onclick={onclose}>Cancel</button>
    <button type="submit" class="btn prominent">{assignment ? "Done" : "Add"}</button>
  </div>
</form>

{#if confirmDelete && assignment}
  <Alert
    title="Delete {assignment.name}?"
    message="This can't be undone."
    onconfirm={remove}
    oncancel={() => (confirmDelete = false)}
  />
{/if}
