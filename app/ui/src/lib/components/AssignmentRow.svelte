<script lang="ts">
  import { tick } from "svelte";
  import { app, mutate, toast } from "../app.svelte";
  import { ceil1, dec2, describeCurve, fmtDate, movement, parseOptNum, plain } from "../format";
  import type { AssignmentView, NeededScore } from "../generated/output";
  import AssignmentForm from "./AssignmentForm.svelte";
  import Icon from "./Icon.svelte";

  let {
    a,
    dropped = false,
    needed,
    impact = null,
    leverage = null,
    overrides = [],
    realScore,
  }: {
    a: AssignmentView;
    dropped?: boolean;
    needed?: NeededScore;
    /** Change in the running grade when this was graded (core `impact`). */
    impact?: number | null;
    /** Core `leverage` for remaining work, with its share of the course's largest (bar width only). */
    leverage?: { value: number; share: number } | null;
    /** Indices of every override in the active what-if that touches this assignment. */
    overrides?: number[];
    /** The real (stored) score, shown faintly beside a what-if one. Undefined outside a what-if. */
    realScore?: number | null;
  } = $props();

  const scenarioId = $derived(app.scenarioId);
  const changed = $derived(overrides.length > 0);

  let editingScore = $state(false);
  let scoreText = $state("");
  let editingAll = $state(false);

  let scoreInput = $state<HTMLInputElement>();

  async function startScore(e: Event) {
    e.stopPropagation();
    scoreText = a.score == null ? "" : String(a.score);
    editingScore = true;
    // Select the old value once the field exists and is filled, so typing replaces it.
    await tick();
    scoreInput?.focus();
    scoreInput?.select();
  }

  /** Undo every what-if change to this assignment (highest index first so the rest stay valid). */
  async function revert(e?: Event) {
    e?.stopPropagation();
    if (!scenarioId) return;
    for (const index of [...overrides].sort((x, y) => y - x))
      if (!(await mutate({ command: "remove_override", scenario_id: scenarioId, index }))) break;
  }

  /** Real data outside a what-if. Inside one: a hypothetical score; clearing it reverts. */
  async function saveScore() {
    const v = parseOptNum(scoreText);
    if (v === undefined) return toast("Score must be a number");
    editingScore = false;
    if (!scenarioId) return void (await mutate({ command: "update_assignment", id: a.id, score: v }));
    if (v === null && changed) return revert();
    if (v === (a.score ?? null)) return;
    await mutate({
      command: "add_override",
      scenario_id: scenarioId,
      override: { type: "set_score", assignment_id: a.id, score: v },
    });
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

  const notes = $derived.by(() => {
    const out: string[] = [];
    if (a.due) out.push(fmtDate(a.due));
    if ((a.curves ?? []).length > 0) out.push(`Curve ${(a.curves ?? []).map(describeCurve).join(", ")}`);
    if (a.status === "missing") out.push("Missing");
    if (a.status === "excused") out.push("Excused");
    return out;
  });

  const remaining = $derived(a.score == null && a.status !== "excused" && a.status !== "missing");
  const moved = $derived(movement(impact));
  const scoreTip = $derived(
    scenarioId
      ? changed
        ? "What-if score"
        : "Type a what-if score"
      : dropped
        ? "Dropped: the lowest in its category doesn't count"
        : "Edit score",
  );
</script>

{#snippet score()}
  <button type="button" class="value-btn num" class:dropped title={scoreTip} onclick={startScore}>
    <span class:faint={a.score == null}>{a.score == null ? "—" : plain(a.score)}</span><span class="of"
      >{` / ${plain(a.points_possible)}`}</span
    >
  </button>
{/snippet}

{#if editingAll}
  <AssignmentForm assignment={a} onclose={() => (editingAll = false)} />
{:else}
  <!-- Clicking anywhere on the row opens the editor; the name button gives keyboard access. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="group-row clickable" onclick={() => (editingAll = true)}>
    <div class="grow">
      <button type="button" class="name" class:hypo={a.hypothetical} aria-label="Edit {a.name}">{a.name}</button>
      {#if notes.length > 0}
        <div class="t-callout muted">{notes.join(" · ")}</div>
      {/if}
    </div>
    {#if leverage && remaining}
      <span
        class="lever"
        role="img"
        aria-label="Weight {dec2(leverage.value)}"
        title="Each 10% here moves your final grade {dec2(leverage.value)} points"
      >
        <span style:width="{Math.max(leverage.share, 0.06) * 100}%"></span>
      </span>
    {/if}
    <div class="score">
      {#if editingScore}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <input
          bind:this={scoreInput}
          class="narrow right"
          bind:value={scoreText}
          inputmode="decimal"
          placeholder={scenarioId && changed ? "real" : "—"}
          aria-label="Score for {a.name}"
          onclick={(e) => e.stopPropagation()}
          onkeydown={scoreKey}
          onblur={() => (editingScore = false)}
        />
      {:else if changed}
        <div class="line">
          {#if realScore != null && !a.hypothetical}
            <span class="real num" title="Real score">{plain(realScore)}</span>
          {/if}
          <span class="chip">
            {@render score()}
            <button type="button" class="undo" aria-label="Revert {a.name}" title="Revert" onclick={revert}>
              <Icon name="close" size={10} />
            </button>
          </span>
        </div>
      {:else}
        {@render score()}
      {/if}
      {#if !editingScore}
        {#if needed && a.score == null}
          {@const n = needed.needed_points}
          {#if n != null && n > needed.points_possible}
            <div class="t-callout neg num" title="More than full marks">{ceil1(n)} needed</div>
          {:else if n != null && n > 0}
            <div class="t-callout accent num">{ceil1(n)} needed</div>
          {/if}
        {:else if moved}
          <div
            class="t-callout num"
            class:pos={moved.startsWith("↑")}
            class:neg={moved.startsWith("↓")}
            title="Moved your grade {moved.slice(1)} points when it was graded"
          >
            {moved}
          </div>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .name {
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
    text-align: left;
    font: inherit;
  }
  .name:focus-visible {
    outline-offset: 4px;
  }
  .name.hypo {
    color: var(--accent);
  }
  .score {
    text-align: right;
    min-width: 92px;
  }
  .line {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--s2);
  }
  .of {
    color: var(--label-2);
  }
  .dropped {
    text-decoration: line-through;
    text-decoration-color: var(--label-3);
    color: var(--label-3);
  }
  .dropped .of {
    color: inherit;
  }
  /* A what-if value: a quiet tinted chip with its own revert button. */
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 1px 2px 1px 8px;
    border-radius: 999px;
    background: var(--accent-tint);
    color: var(--accent);
  }
  .chip .value-btn,
  .chip .of {
    color: inherit;
  }
  .chip .value-btn:hover {
    background: none;
  }
  .undo {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
    opacity: 0.7;
  }
  .undo:hover {
    opacity: 1;
    background: var(--accent-tint);
  }
  .real {
    color: var(--label-3);
    text-decoration: line-through;
  }
  .lever {
    flex: none;
    width: 40px;
    height: 4px;
    border-radius: 2px;
    background: var(--separator);
    overflow: hidden;
  }
  .lever > span {
    display: block;
    height: 100%;
    background: var(--label-3);
    border-radius: 2px;
  }
</style>
