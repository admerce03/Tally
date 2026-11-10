<script lang="ts">
  import { newId } from "../api";
  import { app, ask, mutate, query, toast } from "../app.svelte";
  import { parseNum, pct } from "../format";
  import AnalysisPanel from "./AnalysisPanel.svelte";
  import CategoryCard from "./CategoryCard.svelte";
  import CourseSettings from "./CourseSettings.svelte";
  import SolverPanel from "./SolverPanel.svelte";

  let { courseId }: { courseId: string } = $props();

  type Tab = "grades" | "target" | "analysis";
  let tab = $state<Tab>("grades");
  let settingsOpen = $state(false);
  let addingCategory = $state(false);
  let cat = $state({ name: "", weight: "", drop: "0" });

  // Switching course resets view state.
  $effect(() => {
    void courseId;
    tab = "grades";
    settingsOpen = false;
    addingCategory = false;
  });

  const course = query(() => {
    const cmd = { command: "show_course" as const, course_id: courseId, scenario_id: app.scenarioId };
    return () => ask(cmd, "course").then((o) => o.course);
  });

  const grade = query(() => {
    const cmd = {
      command: "course_grade" as const,
      course_id: courseId,
      curve_mode: app.curveMode,
      scenario_id: app.scenarioId,
    };
    return () => ask(cmd, "course_grade").then((o) => o.report);
  });

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }

  async function addCategory(e: Event) {
    e.preventDefault();
    const weight = parseNum(cat.weight);
    const drop = parseNum(cat.drop);
    if (!cat.name.trim()) return toast("Name is required");
    if (weight === undefined) return toast("Weight must be a number");
    if (drop === undefined || !Number.isInteger(drop) || drop < 0) return toast("Drop lowest must be a whole number");
    const out = await mutate({
      command: "add_category",
      course_id: courseId,
      id: newId(),
      name: cat.name.trim(),
      weight,
      drop_lowest: drop,
    });
    if (out) {
      cat = { name: "", weight: "", drop: "0" };
      addingCategory = false;
    }
  }
</script>

{#if course.error && !course.value}
  <div class="empty">{course.error}</div>
{:else if course.value}
  {@const c = course.value}
  <div class="stack">
    <header class="spread top">
      <div>
        <h2>{c.name}</h2>
        <p class="muted small">
          {#if c.code}{c.code} &middot;{/if}
          {c.credits} credits
          {#if c.target != null}&middot; target {c.target}%{/if}
          {#if c.gpa_status && c.gpa_status !== "normal"}&middot; {c.gpa_status.replace("_", "/")}{/if}
        </p>
      </div>
      <div class="cluster">
        <div class="seg" role="group" aria-label="Curve">
          <button type="button" aria-pressed={app.curveMode === "apply"} onclick={() => (app.curveMode = "apply")}>
            Curve: apply
          </button>
          <button type="button" aria-pressed={app.curveMode === "ignore"} onclick={() => (app.curveMode = "ignore")}>
            Ignore
          </button>
        </div>
        <button type="button" class="btn" onclick={() => (settingsOpen = true)}>Settings</button>
      </div>
    </header>

    <section class="stats" aria-label="Grade summary">
      <div class="card stat">
        <span class="muted small">Current</span>
        <span class="big num">{pct(grade.value?.current)}</span>
        <span class="pill accent">{grade.value?.current_letter ?? "—"}</span>
      </div>
      <div class="card stat">
        <span class="muted small">Projected (full weights)</span>
        <span class="big num">{pct(grade.value?.projected)}</span>
        <span class="pill">{grade.value?.projected_letter ?? "—"}</span>
      </div>
    </section>
    {#if grade.error}
      <p class="error-text small">{grade.error}</p>
    {/if}

    <div class="tabs" role="tablist">
      <button type="button" role="tab" aria-selected={tab === "grades"} onclick={() => (tab = "grades")}>Grades</button>
      <button type="button" role="tab" aria-selected={tab === "target"} onclick={() => (tab = "target")}>Target</button>
      <button type="button" role="tab" aria-selected={tab === "analysis"} onclick={() => (tab = "analysis")}>Analysis</button>
    </div>

    {#if tab === "grades"}
      <div class="stack">
        {#if c.categories.length === 0 && !addingCategory}
          <div class="empty">
            <p>No categories yet. Add one (for example Homework or Exams) to start entering grades.</p>
            <button type="button" class="btn primary" disabled={!!app.scenarioId} onclick={() => (addingCategory = true)}>
              Add category
            </button>
          </div>
        {/if}
        {#each c.categories as category (category.id)}
          <CategoryCard
            {category}
            breakdown={grade.value?.categories.find((b) => b.category_id === category.id)}
          />
        {/each}
        {#if addingCategory}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <form
            class="card inline"
            onsubmit={addCategory}
            onkeydown={(e) => e.key === "Escape" && (addingCategory = false)}
          >
            <label class="field">
              <span>Category name</span>
              <input use:focusOnMount bind:value={cat.name} />
            </label>
            <label class="field">
              <span>Weight</span>
              <input class="narrow" bind:value={cat.weight} inputmode="decimal" />
            </label>
            <label class="field">
              <span>Drop lowest</span>
              <input class="narrow" bind:value={cat.drop} inputmode="numeric" />
            </label>
            <button type="submit" class="btn primary">Add category</button>
            <button type="button" class="btn ghost" onclick={() => (addingCategory = false)}>Cancel</button>
          </form>
        {:else if c.categories.length > 0}
          <div>
            <button type="button" class="btn" disabled={!!app.scenarioId} onclick={() => (addingCategory = true)}>
              + Category
            </button>
          </div>
        {/if}
      </div>
    {:else if tab === "target"}
      <SolverPanel course={c} />
    {:else}
      <AnalysisPanel {courseId} />
    {/if}
  </div>

  {#if settingsOpen}
    <CourseSettings course={c} onclose={() => (settingsOpen = false)} />
  {/if}
{:else if course.loading}
  <p class="muted">Loading...</p>
{/if}

<style>
  .top h2 {
    margin-bottom: 2px;
  }
  p {
    margin: 0;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 12px;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-start;
  }
  .big {
    font-size: 28px;
    font-weight: 650;
    letter-spacing: -0.01em;
    text-align: left;
  }
  .empty p {
    margin-bottom: 12px;
  }
</style>
