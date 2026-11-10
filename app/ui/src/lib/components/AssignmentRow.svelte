<script lang="ts">
  import { mutate, toast } from "../app.svelte";
  import { STATUSES, cap, describeCurve, parseOptNum, plain } from "../format";
  import type { Assignment, AssignmentStatus } from "../generated/output";
  import AssignmentForm from "./AssignmentForm.svelte";
  import ConfirmButton from "./ConfirmButton.svelte";

  let { a }: { a: Assignment } = $props();

  let editingScore = $state(false);
  let scoreText = $state("");
  let editingAll = $state(false);

  function startScore() {
    scoreText = a.score == null ? "" : String(a.score);
    editingScore = true;
  }

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function saveScore() {
    const v = parseOptNum(scoreText);
    if (v === undefined) return toast("Score must be a number");
    editingScore = false;
    await mutate({ command: "update_assignment", id: a.id, score: v });
  }

  function scoreKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      saveScore();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      editingScore = false;
    }
  }
</script>

<tr>
  <td>
    <div>{a.name}</div>
    {#if (a.curves ?? []).length > 0}
      <div class="small muted">Curve: {(a.curves ?? []).map(describeCurve).join(", ")}</div>
    {/if}
  </td>
  <td class="muted num">{a.due ?? "—"}</td>
  <td>
    <select
      value={a.status ?? "pending"}
      aria-label="Status of {a.name}"
      onchange={(e) =>
        mutate({
          command: "update_assignment",
          id: a.id,
          status: e.currentTarget.value as AssignmentStatus,
        })}
    >
      {#each STATUSES as s (s)}
        <option value={s}>{cap(s)}</option>
      {/each}
    </select>
  </td>
  <td class="num">
    {#if editingScore}
      <input
        use:focusSelect
        class="narrow"
        bind:value={scoreText}
        inputmode="decimal"
        aria-label="Score for {a.name}"
        onkeydown={scoreKey}
        onblur={() => (editingScore = false)}
      />
    {:else}
      <button type="button" class="btn ghost sm score" onclick={startScore} title="Edit score">
        {plain(a.score)} / {plain(a.points_possible)}
      </button>
    {/if}
  </td>
  <td class="num">
    <button type="button" class="btn ghost sm" onclick={() => (editingAll = !editingAll)}>Edit</button>
    <ConfirmButton label="Delete" onconfirm={() => mutate({ command: "remove_assignment", id: a.id })} />
  </td>
</tr>
{#if editingAll}
  <tr>
    <td colspan="5" style="padding:0">
      <AssignmentForm assignment={a} onclose={() => (editingAll = false)} />
    </td>
  </tr>
{/if}

<style>
  .score {
    font-variant-numeric: tabular-nums;
  }
</style>
