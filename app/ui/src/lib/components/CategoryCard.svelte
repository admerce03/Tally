<script lang="ts">
  import { app, mutate, toast } from "../app.svelte";
  import { parseNum, pct, plain } from "../format";
  import type { CategoryBreakdown, CategoryView, NeededScore } from "../generated/output";
  import Alert from "./Alert.svelte";
  import AssignmentForm from "./AssignmentForm.svelte";
  import AssignmentRow from "./AssignmentRow.svelte";

  let {
    category,
    breakdown,
    needed,
    impact,
    leverage,
    overrides,
    realScores,
  }: {
    category: CategoryView;
    breakdown?: CategoryBreakdown;
    needed?: Map<string, NeededScore>;
    impact?: Map<string, number | null>;
    leverage?: Map<string, { value: number; share: number }>;
    /** Per assignment: indices of what-if overrides touching it. */
    overrides?: Map<string, number[]>;
    /** Per assignment: the real score, while viewing a what-if. */
    realScores?: Map<string, number | null>;
  } = $props();

  const inScenario = $derived(!!app.scenarioId);
  // Dropped rows follow the displayed curve mode (breakdown); fall back to show_course's flag.
  const droppedIds = $derived(
    new Set(breakdown?.dropped ?? category.assignments.filter((a) => a.dropped).map((a) => a.id)),
  );

  let editing = $state(false);
  let adding = $state(false);
  let confirmDelete = $state(false);
  let f = $state({ name: "", weight: "", drop: "" });

  function startEdit() {
    f = { name: category.name, weight: String(category.weight), drop: String(category.drop_lowest ?? 0) };
    editing = true;
  }

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    // bind:value fills the field after this action runs (and resets the caret); select after it has.
    setTimeout(() => node.select(), 0);
  }

  async function save(e: Event) {
    e.preventDefault();
    const weight = parseNum(f.weight);
    const drop = parseNum(f.drop);
    if (!f.name.trim()) return toast("Name is required");
    if (weight === undefined) return toast("Weight must be a number");
    if (drop === undefined || !Number.isInteger(drop) || drop < 0) return toast("Drop lowest must be a whole number");
    if (await mutate({ command: "update_category", id: category.id, name: f.name.trim(), weight, drop_lowest: drop }))
      editing = false;
  }

  async function remove() {
    confirmDelete = false;
    await mutate({ command: "remove_category", id: category.id });
  }
</script>

<div class="group">
  {#if editing}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <form
      class="editor"
      onsubmit={save}
      onkeydown={(e) => {
        if (e.key === "Escape" && !confirmDelete) editing = false;
      }}
    >
      <div class="editor-grid">
        <label class="field wide">
          <span>Name</span>
          <input use:focusOnMount bind:value={f.name} />
        </label>
        <label class="field">
          <span>Weight (%)</span>
          <input bind:value={f.weight} inputmode="decimal" />
        </label>
        <label class="field">
          <span>Drop lowest</span>
          <input bind:value={f.drop} inputmode="numeric" />
        </label>
      </div>
      <div class="editor-foot">
        <button type="button" class="btn destructive lead" onclick={() => (confirmDelete = true)}>Delete</button>
        <button type="button" class="btn" onclick={() => (editing = false)}>Cancel</button>
        <button type="submit" class="btn prominent">Done</button>
      </div>
    </form>
  {:else}
    <button
      type="button"
      class="group-row clickable head"
      disabled={inScenario}
      title={inScenario ? undefined : "Edit category"}
      onclick={startEdit}
    >
      <span class="grow ellipsis">
        <span class="t-headline">{category.name}</span>
        <span class="t-callout muted" title="Weight{category.drop_lowest ? `; lowest ${category.drop_lowest} dropped` : ''}">
          {plain(category.weight)}%{category.drop_lowest ? ` · drops ${category.drop_lowest}` : ""}
        </span>
      </span>
      <span class="t-headline num avg" title="Category average">{breakdown?.average != null ? pct(breakdown.average) : ""}</span>
    </button>
  {/if}

  {#each category.assignments as a (a.id)}
    <AssignmentRow
      {a}
      dropped={droppedIds.has(a.id)}
      needed={needed?.get(a.id)}
      impact={impact?.get(a.id) ?? null}
      leverage={leverage?.get(a.id) ?? null}
      overrides={overrides?.get(a.id) ?? []}
      realScore={realScores?.get(a.id)}
    />
  {/each}

  {#if adding}
    <AssignmentForm categoryId={category.id} onclose={() => (adding = false)} />
  {:else}
    <div class="group-row add">
      <button type="button" class="btn borderless" onclick={() => (adding = true)}>Add assignment</button>
    </div>
  {/if}
</div>

{#if confirmDelete}
  <Alert
    title="Delete {category.name}?"
    message={category.assignments.length
      ? `This also deletes its ${category.assignments.length} ${category.assignments.length === 1 ? "assignment" : "assignments"}. This can't be undone.`
      : "This can't be undone."}
    onconfirm={remove}
    oncancel={() => (confirmDelete = false)}
  />
{/if}

<style>
  .head {
    width: 100%;
    background: none;
    border: 0;
    text-align: left;
    color: inherit;
    min-height: 48px;
  }
  .head .t-callout {
    margin-left: 6px;
  }
  .head:disabled,
  .head:disabled:hover {
    cursor: default;
    background: none;
  }
  .avg {
    min-width: 92px;
    text-align: right;
  }
  .add {
    min-height: 40px;
    padding-left: calc(var(--inset) - 4px);
  }
</style>
