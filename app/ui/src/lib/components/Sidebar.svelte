<script lang="ts">
  import { newId, syncAvailable } from "../api";
  import { app, ask, mutate, navigate, query, stored, toast } from "../app.svelte";
  import { dec2, guessYear, parseNum } from "../format";
  import type { TermSummary } from "../generated/output";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";
  import SyncControl from "./SyncControl.svelte";

  const terms = query(() => () => ask({ command: "list_terms" }, "terms").then((o) => o.terms));

  const cumulative = query(() => {
    const scenario_id = app.scenarioId;
    return () => ask({ command: "cumulative_gpa", scenario_id }, "cumulative_gpa").then((o) => o.report);
  });

  // Current letter per course (one core query each), keyed by a joined id string so it only re-runs when ids change.
  const courseKey = $derived((terms.value ?? []).flatMap((t) => t.courses.map((c) => c.id)).join(","));
  const letters = query(() => {
    const ids = courseKey ? courseKey.split(",") : [];
    const scenario_id = app.scenarioId;
    const curve_mode = app.curveMode;
    return () =>
      Promise.all(
        ids.map((course_id) =>
          ask({ command: "course_grade", course_id, curve_mode, scenario_id }, "course_grade").then(
            (o) => [course_id, o.report.current_letter ?? null] as const,
          ),
        ),
      ).then((entries) => new Map<string, string | null>(entries));
  });

  // Term GPA per term (core term_gpa, through the open what-if), keyed so it re-runs only when ids change.
  const termKey = $derived((terms.value ?? []).map((t) => t.id).join(","));
  const termGpas = query(() => {
    const ids = termKey ? termKey.split(",") : [];
    const scenario_id = app.scenarioId;
    return () =>
      Promise.all(
        ids.map((term_id) =>
          ask({ command: "term_gpa", term_id, scenario_id }, "term_gpa").then((o) => [term_id, o.report.gpa ?? null] as const),
        ),
      ).then((entries) => new Map<string, number | null>(entries));
  });

  // Collapsed terms ("t:<id>") and years ("y:<year>"), remembered per device.
  const COLLAPSED_KEY = "tally.sidebarCollapsed";
  function loadCollapsed(): string[] {
    try {
      const v: unknown = JSON.parse(stored.load(COLLAPSED_KEY) ?? "[]");
      return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
    } catch {
      return [];
    }
  }
  let collapsed = $state<string[]>(loadCollapsed());
  function toggle(key: string) {
    collapsed = collapsed.includes(key) ? collapsed.filter((k) => k !== key) : [...collapsed, key];
    stored.save(COLLAPSED_KEY, JSON.stringify(collapsed));
  }

  // Grouped view: years newest first ("No year" last); within a year, the latest-added term first.
  const years = $derived.by(() => {
    if (!app.groupByYear) return null;
    const byYear = new Map<number | null, TermSummary[]>();
    for (const t of [...(terms.value ?? [])].reverse()) {
      const y = t.year ?? null;
      byYear.set(y, [...(byYear.get(y) ?? []), t]);
    }
    return [...byYear.entries()].sort(([a], [b]) => (a == null ? 1 : b == null ? -1 : b - a));
  });

  let menuFor = $state<string | null>(null);
  let plusOpen = $state(false);
  let editing = $state<{ id: string; field: "name" | "year" } | null>(null);
  let editText = $state("");
  let deleting = $state<TermSummary | null>(null);

  let adding = $state<"course" | "term" | null>(null);
  let newName = $state("");
  let newYear = $state("");
  let yearTouched = $state(false);
  let newCredits = $state("3");
  let newTermId = $state("");

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    // bind:value fills the field after this action runs (and resets the caret); select after it has.
    setTimeout(() => node.select(), 0);
  }

  function startAdd(kind: "course" | "term") {
    adding = kind;
    newName = "";
    newYear = "";
    yearTouched = false;
    newCredits = "3";
    const list = terms.value ?? [];
    newTermId = list[list.length - 1]?.id ?? "";
  }

  /** "" -> null (no year); a whole number -> it; else undefined (invalid). */
  function parseYear(s: string): number | null | undefined {
    const t = s.trim();
    if (!t) return null;
    const n = Number(t);
    return Number.isInteger(n) ? n : undefined;
  }

  async function submitAdd(e: Event) {
    e.preventDefault();
    const name = newName.trim();
    if (!name) return;
    if (adding === "term") {
      const year = parseYear(newYear);
      if (year === undefined) return toast("Year must be a whole number");
      const id = newId();
      if (!(await mutate({ command: "add_term", id, name }))) return;
      if (year != null) await mutate({ command: "set_term_year", id, year });
      adding = null;
      return;
    }
    const credits = parseNum(newCredits);
    if (credits === undefined) return toast("Credits must be a number");
    if (!newTermId) return toast("Pick a term");
    const id = newId();
    if (await mutate({ command: "add_course", term_id: newTermId, id, name, credits })) {
      adding = null;
      navigate({ name: "course", id });
    }
  }

  function startEdit(term: TermSummary, field: "name" | "year") {
    editing = { id: term.id, field };
    editText =
      field === "name" ? term.name : String(term.year ?? guessYear(term.name) ?? "");
  }

  async function commitEdit(term: TermSummary) {
    const e = editing;
    if (!e) return;
    if (e.field === "name") {
      const name = editText.trim();
      if (name && name !== term.name && !(await mutate({ command: "rename_term", id: term.id, name }))) return;
    } else {
      const year = parseYear(editText);
      if (year === undefined) return toast("Year must be a whole number");
      if (year !== (term.year ?? null) && !(await mutate({ command: "set_term_year", id: term.id, year }))) return;
    }
    editing = null;
  }

  async function removeTerm(term: TermSummary) {
    deleting = null;
    const route = app.route;
    if (await mutate({ command: "remove_term", id: term.id })) {
      if (route.name === "course" && term.courses.some((c) => c.id === route.id)) navigate({ name: "home" });
    }
  }

  function escape(e: KeyboardEvent, cancel: () => void) {
    if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    }
  }
</script>

{#snippet termSection(term: TermSummary)}
  {@const open = !collapsed.includes(`t:${term.id}`)}
  <section class="term">
    <div class="term-head" class:menu-open={menuFor === term.id}>
      {#if editing?.id === term.id}
        <form
          class="grow"
          onsubmit={(e) => {
            e.preventDefault();
            commitEdit(term);
          }}
        >
          <input
            use:focusSelect
            class="rename"
            class:year={editing.field === "year"}
            bind:value={editText}
            inputmode={editing.field === "year" ? "numeric" : undefined}
            placeholder={editing.field === "year" ? "Year, e.g. 2026" : "Term name"}
            aria-label={editing.field === "year" ? `Year of ${term.name}` : "Term name"}
            onkeydown={(e) => escape(e, () => (editing = null))}
            onblur={() => (editing = null)}
          />
        </form>
      {:else}
        <button type="button" class="disclosure grow" aria-expanded={open} onclick={() => toggle(`t:${term.id}`)}>
          <span class="chev"><Icon name="chevron" size={10} /></span>
          <span class="t-caption ellipsis">{term.name}</span>
        </button>
        <!-- Term GPA trails the row like the course letters; on hover it gives way to the ⋯ menu. -->
        {@const tg = termGpas.value?.get(term.id)}
        <span class="trail num term-gpa" title="Term GPA">{tg != null ? dec2(tg) : ""}</span>
        <span class="anchor more-slot">
          <button
            type="button"
            class="btn quiet icon more"
            aria-label="Actions for {term.name}"
            aria-haspopup="menu"
            onclick={() => (menuFor = menuFor === term.id ? null : term.id)}
          >
            <Icon name="more" />
          </button>
          {#if menuFor === term.id}
            <Menu
              end
              onclose={() => (menuFor = null)}
              items={[
                { label: "Rename", onselect: () => startEdit(term, "name") },
                { label: term.year ? `Year: ${term.year}…` : "Set year…", onselect: () => startEdit(term, "year") },
                { label: "Delete…", destructive: true, separator: true, onselect: () => (deleting = term) },
              ]}
            />
          {/if}
        </span>
      {/if}
    </div>

    {#if open}
      {#each term.courses as course (course.id)}
        <button
          type="button"
          class="src"
          aria-current={app.route.name === "course" && app.route.id === course.id ? "page" : undefined}
          onclick={() => navigate({ name: "course", id: course.id })}
        >
          <span class="grow ellipsis">{course.name}</span>
          <span class="trail">{letters.value?.get(course.id) ?? ""}</span>
        </button>
      {/each}
    {/if}
  </section>
{/snippet}

<aside class="sidebar" aria-label="Navigation">
  <!-- Title-bar strip: drag to move the window (room for the macOS traffic lights). -->
  <div class="titlebar" data-tauri-drag-region></div>

  <div class="scroll">
    <nav class="top" aria-label="Pages">
      <button
        type="button"
        class="src"
        aria-current={app.route.name === "gpa" ? "page" : undefined}
        onclick={() => navigate({ name: "gpa" })}
      >
        <span class="grow ellipsis">GPA</span>
        <span class="trail num">{cumulative.value?.gpa != null ? dec2(cumulative.value.gpa) : ""}</span>
      </button>
      <button
        type="button"
        class="src"
        aria-current={app.route.name === "scenarios" ? "page" : undefined}
        onclick={() => navigate({ name: "scenarios" })}
      >
        <span class="grow ellipsis">What-if</span>
      </button>
    </nav>

    {#if terms.error}
      <p class="error-text t-callout pad">{terms.error}</p>
    {:else if terms.value && terms.value.length === 0}
      <p class="muted t-callout pad">No terms yet</p>
    {/if}

    {#if years}
      {#each years as [year, list] (year ?? "none")}
        {@const key = `y:${year ?? "none"}`}
        {@const open = !collapsed.includes(key)}
        <section class="year">
          <button type="button" class="disclosure year-head" aria-expanded={open} onclick={() => toggle(key)}>
            <span class="chev"><Icon name="chevron" size={12} /></span>
            <span class="num">{year ?? "No year"}</span>
          </button>
          {#if open}
            <div class="year-body">
              {#each list as term (term.id)}{@render termSection(term)}{/each}
            </div>
          {/if}
        </section>
      {/each}
    {:else}
      {#each terms.value ?? [] as term (term.id)}{@render termSection(term)}{/each}
    {/if}
  </div>

  {#if adding}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <form class="editor add" onsubmit={submitAdd} onkeydown={(e) => escape(e, () => (adding = null))}>
      {#if adding === "term"}
        <div class="cluster nowrap">
          <input
            use:focusSelect
            class="grow"
            bind:value={newName}
            placeholder="Term name"
            aria-label="New term name"
            oninput={() => {
              if (!yearTouched) newYear = String(guessYear(newName) ?? "");
            }}
          />
          <input
            class="yr right"
            bind:value={newYear}
            inputmode="numeric"
            placeholder="Year"
            aria-label="Year"
            oninput={() => (yearTouched = true)}
          />
        </div>
      {:else}
        <input use:focusSelect bind:value={newName} placeholder="Course name" aria-label="New course name" />
        <div class="cluster nowrap">
          <select class="grow" bind:value={newTermId} aria-label="Term">
            {#each terms.value ?? [] as t (t.id)}
              <option value={t.id}>{t.name}</option>
            {/each}
          </select>
          <input class="yr right" bind:value={newCredits} inputmode="decimal" aria-label="Credits" title="Credits" />
        </div>
      {/if}
      <div class="editor-foot">
        <button type="button" class="btn" onclick={() => (adding = null)}>Cancel</button>
        <button type="submit" class="btn prominent">Add</button>
      </div>
    </form>
  {/if}

  <footer class="foot">
    <span class="anchor">
      <button
        type="button"
        class="btn quiet icon"
        aria-label="Add"
        title="New course or term"
        aria-haspopup="menu"
        onclick={() => (plusOpen = !plusOpen)}
      >
        <Icon name="plus" />
      </button>
      {#if plusOpen}
        <Menu
          up
          onclose={() => (plusOpen = false)}
          items={[
            {
              label: "New course…",
              disabled: (terms.value ?? []).length === 0,
              onselect: () => startAdd("course"),
            },
            { label: "New term…", onselect: () => startAdd("term") },
          ]}
        />
      {/if}
    </span>
    <span class="grow"></span>
    {#if syncAvailable}<SyncControl />{/if}
    <button
      type="button"
      class="btn quiet icon"
      aria-label="Settings"
      title="Settings (Ctrl+,)"
      onclick={() => (app.settingsOpen = true)}
    >
      <Icon name="gear" />
    </button>
  </footer>
</aside>

{#if deleting}
  {@const t = deleting}
  <Alert
    title="Delete {t.name}?"
    message={t.courses.length
      ? `This also deletes its ${t.courses.length} ${t.courses.length === 1 ? "course" : "courses"} and all their grades. This can't be undone.`
      : "This can't be undone."}
    onconfirm={() => removeTerm(t)}
    oncancel={() => (deleting = null)}
  />
{/if}

<style>
  .sidebar {
    background: var(--bg-side);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .titlebar {
    flex: none;
    height: var(--bar-h);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--s2) var(--s2);
    display: flex;
    flex-direction: column;
    gap: var(--s4);
  }
  .top,
  .term,
  .year,
  .year-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .year-body {
    gap: var(--s3);
    padding-top: var(--s1);
  }
  .pad {
    padding: 0 var(--s2);
  }
  .src {
    display: flex;
    align-items: center;
    gap: var(--s2);
    width: 100%;
    min-height: 30px;
    padding: 4px var(--s2);
    text-align: left;
    background: none;
    border: 0;
    border-radius: var(--r-control);
    cursor: pointer;
  }
  .src:hover {
    background: var(--bg-hover);
  }
  .src[aria-current="page"] {
    background: var(--accent-tint);
  }
  .trail {
    color: var(--label-2);
    font: var(--t-callout);
  }
  .disclosure {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    background: none;
    border: 0;
    padding: 2px 0;
    cursor: pointer;
    text-align: left;
    color: var(--label-2);
  }
  .disclosure .chev {
    display: inline-flex;
    flex: none;
    transition: transform var(--ease);
    color: var(--label-3);
  }
  .disclosure[aria-expanded="true"] .chev {
    transform: rotate(90deg);
  }
  .year-head {
    padding: 2px var(--s2);
    font: var(--t-headline);
    color: var(--label);
  }
  .term-head {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--s1);
    min-height: 24px;
    padding: 0 var(--s2);
  }
  /* GPA and the ⋯ button share the trailing slot: GPA at rest, ⋯ on hover/focus/open. */
  .more-slot {
    position: absolute;
    right: 2px;
    top: 50%;
    transform: translateY(-50%);
  }
  .more {
    opacity: 0;
    min-height: 22px;
    padding: 2px;
  }
  .term-head:hover .more,
  .term-head:focus-within .more,
  .term-head.menu-open .more {
    opacity: 1;
  }
  .term-head:hover .term-gpa,
  .term-head:focus-within .term-gpa,
  .term-head.menu-open .term-gpa {
    visibility: hidden;
  }
  .rename {
    width: 100%;
    min-height: 24px;
    padding: 2px 6px;
  }
  .add {
    margin: 0 var(--s2) var(--s2);
    gap: var(--s2);
  }
  .nowrap {
    flex-wrap: nowrap;
  }
  .yr {
    width: 64px;
    flex: none;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--s1);
    padding: var(--s2);
    border-top: 1px solid var(--separator);
  }
</style>
