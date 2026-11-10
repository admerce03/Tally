<script lang="ts">
  import { mutate, toast } from "../app.svelte";
  import { parseNum } from "../format";
  import type { Category } from "../generated/output";
  import AssignmentForm from "./AssignmentForm.svelte";
  import AssignmentRow from "./AssignmentRow.svelte";
  import ConfirmButton from "./ConfirmButton.svelte";

  let { category }: { category: Category } = $props();

  let editing = $state(false);
  let adding = $state(false);
  let f = $state({ name: "", weight: "", drop: "" });

  function startEdit() {
    f = { name: category.name, weight: String(category.weight), drop: String(category.drop_lowest ?? 0) };
    editing = true;
  }

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }

  async function save(e: Event) {
    e.preventDefault();
    const weight = parseNum(f.weight);
    const drop = parseNum(f.drop);
    if (!f.name.trim()) return toast("Name is required");
    if (weight === undefined) return toast("Weight must be a number");
    if (drop === undefined || !Number.isInteger(drop) || drop < 0)
      return toast("Drop lowest must be a whole number");
    if (await mutate({ command: "update_category", id: category.id, name: f.name.trim(), weight, drop_lowest: drop }))
      editing = false;
  }
</script>

<div class="card flush">
  <div class="card-head">
    {#if editing}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <form
        class="inline"
        onsubmit={save}
        onkeydown={(e) => e.key === "Escape" && (editing = false)}
      >
        <label class="field">
          <span>Name</span>
          <input use:focusOnMount bind:value={f.name} />
        </label>
        <label class="field">
          <span>Weight</span>
          <input class="narrow" bind:value={f.weight} inputmode="decimal" />
        </label>
        <label class="field">
          <span>Drop lowest</span>
          <input class="narrow" bind:value={f.drop} inputmode="numeric" />
        </label>
        <button type="submit" class="btn primary sm">Save</button>
        <button type="button" class="btn ghost sm" onclick={() => (editing = false)}>Cancel</button>
      </form>
    {:else}
      <div class="cluster">
        <h3>{category.name}</h3>
        <span class="pill">weight {category.weight}</span>
        {#if (category.drop_lowest ?? 0) > 0}
          <span class="pill">drop lowest {category.drop_lowest}</span>
        {/if}
      </div>
      <div class="cluster">
        <button type="button" class="btn ghost sm" onclick={startEdit}>Edit</button>
        <ConfirmButton label="Delete" onconfirm={() => mutate({ command: "remove_category", id: category.id })} />
      </div>
    {/if}
  </div>

  {#if (category.assignments ?? []).length > 0}
    <table>
      <thead>
        <tr>
          <th>Assignment</th>
          <th class="num">Due</th>
          <th>Status</th>
          <th class="num">Score / points</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each category.assignments ?? [] as a (a.id)}
          <AssignmentRow {a} />
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="muted card-body">No assignments yet.</p>
  {/if}

  {#if adding}
    <AssignmentForm categoryId={category.id} onclose={() => (adding = false)} />
  {:else}
    <div class="card-body">
      <button type="button" class="btn sm" onclick={() => (adding = true)}>+ Assignment</button>
    </div>
  {/if}
</div>
