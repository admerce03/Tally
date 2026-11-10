<script lang="ts">
  import { newId } from "../api";
  import { app, ask, mutate, navigate, query, toast } from "../app.svelte";
  import { parseNum } from "../format";
  import ConfirmButton from "./ConfirmButton.svelte";

  const terms = query(() => () => ask({ command: "list_terms" }, "terms").then((o) => o.terms));
  const scenarios = query(
    () => () => ask({ command: "list_scenarios" }, "scenarios").then((o) => o.scenarios),
  );

  // A deleted scenario can no longer be the active one.
  $effect(() => {
    const list = scenarios.value;
    if (app.scenarioId && list && !list.some((s) => s.id === app.scenarioId)) app.scenarioId = null;
  });

  let addingTerm = $state(false);
  let newTerm = $state("");
  let renaming = $state<string | null>(null);
  let renameText = $state("");
  let addingCourseFor = $state<string | null>(null);
  let courseName = $state("");
  let courseCredits = $state("3");

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function addTerm() {
    const name = newTerm.trim();
    if (!name) return;
    const out = await mutate({ command: "add_term", id: newId(), name });
    if (out) {
      newTerm = "";
      addingTerm = false;
    }
  }

  async function renameTerm(id: string) {
    const name = renameText.trim();
    if (!name) return;
    if (await mutate({ command: "rename_term", id, name })) renaming = null;
  }

  async function removeTerm(id: string, courseIds: string[]) {
    if (await mutate({ command: "remove_term", id })) {
      if (app.route.name === "course" && courseIds.includes(app.route.id)) navigate({ name: "home" });
    }
  }

  async function addCourse(termId: string) {
    const name = courseName.trim();
    const credits = parseNum(courseCredits);
    if (!name) return;
    if (credits === undefined) {
      toast("Credits must be a number");
      return;
    }
    const id = newId();
    if (await mutate({ command: "add_course", term_id: termId, id, name, credits })) {
      courseName = "";
      addingCourseFor = null;
      navigate({ name: "course", id });
    }
  }

  async function removeCourse(id: string) {
    if (await mutate({ command: "remove_course", id })) {
      if (app.route.name === "course" && app.route.id === id) navigate({ name: "home" });
    }
  }

  function onKey(e: KeyboardEvent, cancel: () => void) {
    if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    }
  }
</script>

<aside class="sidebar" aria-label="Navigation">
  <a class="brand" href="#/">Tally</a>

  <div class="block">
    <label class="field">
      <span>View</span>
      <select
        value={app.scenarioId ?? ""}
        onchange={(e) => (app.scenarioId = e.currentTarget.value || null)}
      >
        <option value="">Real data</option>
        {#each scenarios.value ?? [] as s (s.id)}
          <option value={s.id}>Scenario: {s.name}</option>
        {/each}
      </select>
    </label>
  </div>

  <nav class="block" aria-label="Pages">
    <button
      type="button"
      class="nav"
      aria-current={app.route.name === "gpa" ? "page" : undefined}
      onclick={() => navigate({ name: "gpa" })}>GPA</button
    >
    <button
      type="button"
      class="nav"
      aria-current={app.route.name === "scenarios" ? "page" : undefined}
      onclick={() => navigate({ name: "scenarios" })}>Scenarios</button
    >
  </nav>

  <div class="block terms">
    <div class="spread head">
      <span class="label">Terms</span>
      <button type="button" class="btn ghost sm" onclick={() => (addingTerm = !addingTerm)}>+ Term</button>
    </div>

    {#if addingTerm}
      <form
        class="inline-add"
        onsubmit={(e) => {
          e.preventDefault();
          addTerm();
        }}
      >
        <input
          use:focusOnMount
          bind:value={newTerm}
          placeholder="Term name"
          aria-label="New term name"
          onkeydown={(e) => onKey(e, () => (addingTerm = false))}
        />
      </form>
    {/if}

    {#if terms.error}
      <p class="error-text small">{terms.error}</p>
    {:else if terms.value && terms.value.length === 0 && !addingTerm}
      <p class="muted small">No terms yet.</p>
    {/if}

    {#each terms.value ?? [] as term (term.id)}
      <section>
        <div class="term-row">
          {#if renaming === term.id}
            <form
              class="grow"
              onsubmit={(e) => {
                e.preventDefault();
                renameTerm(term.id);
              }}
            >
              <input
                use:focusOnMount
                bind:value={renameText}
                aria-label="Term name"
                onkeydown={(e) => onKey(e, () => (renaming = null))}
              />
            </form>
          {:else}
            <span class="term-name grow">{term.name}</span>
            <span class="row-actions">
              <button
                type="button"
                class="btn ghost sm"
                title="Rename term"
                onclick={() => {
                  renaming = term.id;
                  renameText = term.name;
                }}>Rename</button
              >
              <ConfirmButton
                label="Delete"
                title="Delete term and its courses"
                onconfirm={() =>
                  removeTerm(
                    term.id,
                    term.courses.map((c) => c.id),
                  )}
              />
            </span>
          {/if}
        </div>

        {#each term.courses as course (course.id)}
          <div class="course-row">
            <button
              type="button"
              class="nav grow"
              aria-current={app.route.name === "course" && app.route.id === course.id ? "page" : undefined}
              onclick={() => navigate({ name: "course", id: course.id })}
            >
              {course.name}
            </button>
            <span class="row-actions">
              <ConfirmButton label="x" title="Delete course" onconfirm={() => removeCourse(course.id)} />
            </span>
          </div>
        {/each}

        {#if addingCourseFor === term.id}
          <form
            class="inline-add course-form"
            onsubmit={(e) => {
              e.preventDefault();
              addCourse(term.id);
            }}
          >
            <input
              use:focusOnMount
              bind:value={courseName}
              placeholder="Course name"
              aria-label="New course name"
              onkeydown={(e) => onKey(e, () => (addingCourseFor = null))}
            />
            <input
              class="credits"
              bind:value={courseCredits}
              inputmode="decimal"
              aria-label="Credits"
              onkeydown={(e) => onKey(e, () => (addingCourseFor = null))}
            />
          </form>
        {:else}
          <button
            type="button"
            class="btn ghost sm add-course"
            onclick={() => {
              addingCourseFor = term.id;
              courseName = "";
            }}>+ Course</button
          >
        {/if}
      </section>
    {/each}
  </div>
</aside>

<style>
  .sidebar {
    background: var(--bg-side);
    border-right: 1px solid var(--border);
    padding: 16px 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .brand {
    font-size: 15px;
    font-weight: 650;
    margin: 0 8px;
    letter-spacing: 0.02em;
    color: var(--fg);
    text-decoration: none;
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
    font-weight: 600;
    padding-left: 8px;
  }
  .nav {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-radius: var(--radius);
    padding: 6px 8px;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .nav:hover {
    background: var(--hover);
  }
  .nav[aria-current="page"] {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .head {
    margin-bottom: 4px;
  }
  .term-row,
  .course-row {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 30px;
  }
  .term-name {
    padding: 4px 8px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .course-row {
    padding-left: 8px;
  }
  .row-actions {
    display: none;
  }
  .term-row:hover .row-actions,
  .term-row:focus-within .row-actions,
  .course-row:hover .row-actions,
  .course-row:focus-within .row-actions {
    display: inline-flex;
  }
  section {
    margin-bottom: 8px;
  }
  .inline-add {
    display: flex;
    gap: 6px;
    margin: 4px 0;
  }
  .inline-add input {
    flex: 1;
    min-width: 0;
  }
  .inline-add input.credits {
    flex: 0 0 52px;
  }
  .course-form {
    padding-left: 8px;
  }
  .add-course {
    margin-left: 8px;
  }
</style>
