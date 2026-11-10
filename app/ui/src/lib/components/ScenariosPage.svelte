<script lang="ts">
  import { app, ask, mutate, navigate, query, toast } from "../app.svelte";
  import { describeChanges, loadCatalog, scenarioCourses } from "../catalog";
  import { dec2, pct } from "../format";
  import type { Scenario, ScenarioColumn } from "../generated/output";
  import { startWhatIf, switchWhatIf } from "../whatif";
  import Alert from "./Alert.svelte";
  import ComparePanel from "./ComparePanel.svelte";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";

  // Saved what-ifs, each with what it changes and what that does (real → what-if), plus actions.
  const scenarios = query(
    () => () => ask({ command: "list_scenarios" }, "scenarios").then((o) => o.scenarios),
  );
  const catalog = query(() => loadCatalog);
  // Every number shown comes from this comparison (real data + every what-if).
  const cmp = query(
    () => () => ask({ command: "compare_scenarios", scenario_ids: [] }, "comparison").then((o) => o.columns),
  );
  const realCol = $derived(cmp.value?.find((c) => c.scenario_id == null) ?? null);

  let newOpen = $state(false);
  let renaming = $state<string | null>(null);
  let renameText = $state("");
  let deleting = $state<Scenario | null>(null);

  type Effect = { label: string; was: string; now: string; dir: "up" | "down" | "same" };

  /** Direction of a change between two core numbers (display only: which color to use). */
  function dir(was: number | null | undefined, now: number | null | undefined, fmtWas: string, fmtNow: string) {
    if (fmtWas === fmtNow || was == null || now == null) return "same" as const;
    return now > was ? ("up" as const) : ("down" as const);
  }

  function effects(s: Scenario): Effect[] {
    const cat = catalog.value;
    const col = cmp.value?.find((c) => c.scenario_id === s.id);
    if (!cat || !col || !realCol) return [];
    const out: Effect[] = [];
    for (const id of scenarioCourses(s, cat)) {
      const r = realCol.courses.find((c) => c.course_id === id);
      const n = col.courses.find((c) => c.course_id === id);
      if (!r || !n) continue;
      const was = `${pct(r.current)} ${r.current_letter ?? ""}`.trim();
      const now = `${pct(n.current)} ${n.current_letter ?? ""}`.trim();
      out.push({ label: r.name, was, now, dir: dir(r.current, n.current, was, now) });
    }
    const was = dec2(realCol.cumulative_gpa);
    const now = dec2(col.cumulative_gpa);
    out.push({ label: "GPA", was, now, dir: dir(realCol.cumulative_gpa, col.cumulative_gpa, was, now) });
    return out;
  }

  function colError(s: Scenario): string | null {
    return cmp.value?.find((c: ScenarioColumn) => c.scenario_id === s.id)?.error ?? null;
  }

  async function open(s: Scenario) {
    await switchWhatIf(s.id);
    const cat = catalog.value;
    const course = cat ? scenarioCourses(s, cat)[0] : undefined;
    if (course) navigate({ name: "course", id: course });
  }

  async function create(courseId: string, name: string) {
    navigate({ name: "course", id: courseId });
    await startWhatIf(name);
  }

  function startRename(s: Scenario) {
    renaming = s.id;
    renameText = s.name;
  }

  async function commitRename(s: Scenario) {
    if (renaming !== s.id) return;
    renaming = null;
    const n = renameText.trim();
    if (!n) return toast("Name is required");
    if (n !== s.name) await mutate({ command: "rename_scenario", id: s.id, name: n });
  }

  async function remove(s: Scenario) {
    deleting = null;
    if ((await mutate({ command: "delete_scenario", id: s.id })) && app.scenarioId === s.id) app.scenarioId = null;
  }

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    setTimeout(() => node.select(), 0);
  }
</script>

<div class="page">
  <header class="spread">
    <h1 class="t-large">What-if</h1>
    <span class="anchor">
      <button
        type="button"
        class="btn"
        aria-haspopup="menu"
        disabled={!catalog.value?.courses.length}
        onclick={() => (newOpen = !newOpen)}
      >
        <Icon name="plus" size={14} />
        New what-if
      </button>
      {#if newOpen && catalog.value}
        <Menu
          end
          onclose={() => (newOpen = false)}
          items={catalog.value.courses.map((c) => ({ label: c.name, onselect: () => create(c.id, c.name) }))}
        />
      {/if}
    </span>
  </header>

  {#if scenarios.error}
    <div class="empty">{scenarios.error}</div>
  {:else if scenarios.value && scenarios.value.length === 0}
    <p class="muted">None yet. Hypothetical scores never change your real grades.</p>
  {:else if scenarios.value}
    <section class="group" aria-label="Saved what-ifs">
      {#each scenarios.value as s (s.id)}
        {@const changes = catalog.value ? describeChanges(s, catalog.value) : []}
        {@const err = colError(s)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="group-row clickable wi" onclick={() => renaming !== s.id && open(s)}>
          <div class="line">
            {#if renaming === s.id}
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
              <input
                use:focusSelect
                class="rename"
                bind:value={renameText}
                aria-label="What-if name"
                onclick={(e) => e.stopPropagation()}
                onblur={() => commitRename(s)}
                onkeydown={(e) => {
                  if (e.key === "Enter") commitRename(s);
                  if (e.key === "Escape") renaming = null;
                }}
              />
            {:else}
              <button type="button" class="name t-headline ellipsis" class:accent={app.scenarioId === s.id}>
                {s.name}
              </button>
            {/if}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="actions" onclick={(e) => e.stopPropagation()}>
              <button type="button" class="btn borderless" onclick={() => open(s)}>Open</button>
              <button type="button" class="btn quiet" onclick={() => startRename(s)}>Rename</button>
              <button type="button" class="btn destructive" onclick={() => (deleting = s)}>Delete</button>
            </div>
          </div>
          <div class="line wrap">
            <div class="t-callout muted changes" title={changes.join(" · ")}>
              {changes.length ? changes.join(" · ") : "No changes"}
            </div>
            <div class="effects">
              {#if err}
                <span class="t-callout error-text">{err}</span>
              {:else}
                {#each effects(s) as e (e.label)}
                  <div class="effect">
                    <span class="stat-label ellipsis">{e.label}</span>
                    <span class="num">
                      {#if e.dir !== "same"}<span class="faint">{e.was} →</span>{/if}
                      <span class:pos={e.dir === "up"} class:neg={e.dir === "down"}>
                        {e.now}{#if e.dir === "up"} ↑{:else if e.dir === "down"} ↓{/if}
                      </span>
                    </span>
                  </div>
                {/each}
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </section>

    {#if scenarios.value.length >= 2}
      <section>
        <h2 class="section-label">Side by side</h2>
        <ComparePanel columns={cmp.value ?? []} />
      </section>
    {/if}
  {/if}
</div>

{#if deleting}
  {@const s = deleting}
  <Alert
    title="Delete {s.name}?"
    message="Its hypothetical changes are deleted. Your real grades stay as they are."
    onconfirm={() => remove(s)}
    oncancel={() => (deleting = null)}
  />
{/if}

<style>
  .wi {
    flex-direction: column;
    align-items: stretch;
    gap: var(--s1);
    padding-top: var(--s3);
    padding-bottom: var(--s3);
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--s2) var(--s6);
    min-height: 28px;
  }
  .line.wrap {
    flex-wrap: wrap;
    align-items: flex-end;
  }
  .name {
    flex: 1;
    min-width: 0;
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }
  .changes {
    flex: 1 1 240px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .rename {
    flex: 1;
    max-width: 320px;
  }
  .effects {
    display: flex;
    gap: var(--s6);
    margin-left: auto;
  }
  .effect {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    max-width: 220px;
  }
  .faint {
    color: var(--label-3);
  }
  .actions {
    display: flex;
    gap: var(--s1);
    opacity: 0;
    transition: opacity var(--ease);
  }
  .wi:hover .actions,
  .wi:focus-within .actions {
    opacity: 1;
  }
  /* Touch screens have no hover: keep the actions visible. */
  @media (hover: none) {
    .actions {
      opacity: 1;
    }
  }
</style>
