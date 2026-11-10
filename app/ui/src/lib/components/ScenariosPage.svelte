<script lang="ts">
  import { newId } from "../api";
  import { app, ask, mutate, query } from "../app.svelte";
  import { describeOverride, loadCatalog } from "../catalog";
  import ComparePanel from "./ComparePanel.svelte";
  import ConfirmButton from "./ConfirmButton.svelte";
  import OverrideForm from "./OverrideForm.svelte";

  const scenarios = query(
    () => () => ask({ command: "list_scenarios" }, "scenarios").then((o) => o.scenarios),
  );
  const catalog = query(() => loadCatalog);

  let selectedId = $state<string | null>(null);
  let newName = $state("");
  let renamingId = $state<string | null>(null);
  let renameText = $state("");

  const selected = $derived(scenarios.value?.find((s) => s.id === selectedId) ?? scenarios.value?.[0] ?? null);

  async function create(e: Event) {
    e.preventDefault();
    const name = newName.trim();
    if (!name) return;
    const id = newId();
    if (await mutate({ command: "create_scenario", id, name })) {
      newName = "";
      selectedId = id;
    }
  }

  async function rename(id: string) {
    const name = renameText.trim();
    if (!name) return;
    if (await mutate({ command: "rename_scenario", id, name })) renamingId = null;
  }

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<div class="stack">
  <header class="spread">
    <h2>Scenarios</h2>
  </header>
  <p class="muted">
    Scenarios overlay hypothetical scores and curves on your real data. They never change it. Pick one in the sidebar to
    view every page through it.
  </p>

  <form class="inline" onsubmit={create}>
    <label class="field">
      <span>New scenario</span>
      <input bind:value={newName} placeholder="e.g. Ace the final" />
    </label>
    <button type="submit" class="btn primary">Create</button>
  </form>

  {#if scenarios.error}
    <div class="empty">{scenarios.error}</div>
  {:else if scenarios.value && scenarios.value.length === 0}
    <div class="empty">No scenarios yet. Create one above to explore what-ifs.</div>
  {:else if scenarios.value}
    <div class="tabs" role="tablist" aria-label="Scenarios">
      {#each scenarios.value as s (s.id)}
        <button type="button" role="tab" aria-selected={selected?.id === s.id} onclick={() => (selectedId = s.id)}>
          {s.name}
        </button>
      {/each}
    </div>

    {#if selected}
      <section class="card stack">
        <div class="spread">
          {#if renamingId === selected.id}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <form
              class="cluster"
              onsubmit={(e) => {
                e.preventDefault();
                rename(selected.id);
              }}
              onkeydown={(e) => e.key === "Escape" && (renamingId = null)}
            >
              <input use:focusSelect bind:value={renameText} aria-label="Scenario name" />
              <button type="submit" class="btn primary sm">Save</button>
              <button type="button" class="btn ghost sm" onclick={() => (renamingId = null)}>Cancel</button>
            </form>
          {:else}
            <h3>{selected.name}</h3>
            <div class="cluster">
              <button
                type="button"
                class="btn sm"
                onclick={() => (app.scenarioId = app.scenarioId === selected.id ? null : selected.id)}
              >
                {app.scenarioId === selected.id ? "Back to real data" : "View through this scenario"}
              </button>
              <button
                type="button"
                class="btn ghost sm"
                onclick={() => {
                  renamingId = selected.id;
                  renameText = selected.name;
                }}>Rename</button
              >
              <ConfirmButton label="Delete" onconfirm={() => mutate({ command: "delete_scenario", id: selected.id })} />
            </div>
          {/if}
        </div>

        <div class="stack tight">
          <span class="muted small">Overrides</span>
          {#if (selected.overrides ?? []).length === 0}
            <p class="muted">No overrides yet.</p>
          {:else}
            <ul class="overrides">
              {#each selected.overrides ?? [] as o, i (i)}
                <li>
                  <span class="grow">{describeOverride(o, catalog.value)}</span>
                  <button
                    type="button"
                    class="btn ghost sm danger"
                    onclick={() => mutate({ command: "remove_override", scenario_id: selected.id, index: i })}
                  >
                    Remove
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        {#if catalog.value}
          {#key selected.id}
            <OverrideForm scenarioId={selected.id} catalog={catalog.value} />
          {/key}
        {/if}
      </section>
    {/if}

    <ComparePanel />
  {/if}
</div>

<style>
  p {
    margin: 0;
  }
  .overrides {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .overrides li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .overrides li:last-child {
    border-bottom: 0;
  }
</style>
