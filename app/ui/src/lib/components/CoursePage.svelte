<script lang="ts">
  import { newId } from "../api";
  import { app, ask, mutate, query, toast } from "../app.svelte";
  import { dec2, parseNum, pct, plain } from "../format";
  import type { NeededScore } from "../generated/output";
  import CategoryCard from "./CategoryCard.svelte";
  import TargetStat from "./TargetStat.svelte";
  import TrendChart from "./TrendChart.svelte";

  let { courseId }: { courseId: string } = $props();

  let addingCategory = $state(false);
  let cat = $state({ name: "", weight: "", drop: "0" });

  const sid = $derived(app.scenarioId);
  /** `undefined` skips a "real data" query when no what-if is open. */
  const realSid = $derived(sid ? null : undefined);

  const course = query(() => {
    const cmd = { command: "show_course" as const, course_id: courseId, scenario_id: sid };
    return () => ask(cmd, "course").then((o) => o.course);
  });
  // The real course beside a what-if: the faint real score next to each what-if chip.
  const realCourse = query(() => {
    if (realSid === undefined) return null;
    return () => ask({ command: "show_course", course_id: courseId }, "course").then((o) => o.course);
  });

  const terms = query(() => () => ask({ command: "list_terms" }, "terms").then((o) => o.terms));
  const termName = $derived(terms.value?.find((t) => t.courses.some((c) => c.id === courseId))?.name ?? null);

  function gradeQuery(scenario: () => string | null | undefined) {
    return query(() => {
      const scenario_id = scenario();
      if (scenario_id === undefined) return null;
      const cmd = { command: "course_grade" as const, course_id: courseId, curve_mode: app.curveMode, scenario_id };
      return () => ask(cmd, "course_grade").then((o) => o.report);
    });
  }
  const grade = gradeQuery(() => sid);
  const realGrade = gradeQuery(() => realSid);

  function gpaQuery(scenario: () => string | null | undefined) {
    return query(() => {
      const scenario_id = scenario();
      if (scenario_id === undefined) return null;
      return () => ask({ command: "cumulative_gpa", scenario_id }, "cumulative_gpa").then((o) => o.report.gpa ?? null);
    });
  }
  const gpa = gpaQuery(() => sid ?? undefined);
  const realGpa = gpaQuery(() => realSid);

  // One solve feeds the target color and the per-assignment "needed".
  const target = $derived(course.value?.target ?? null);
  const solve = query(() => {
    if (target == null) return null;
    const cmd = {
      command: "solve_target" as const,
      course_id: courseId,
      locked: [],
      with_curve: app.solverCurve,
      scenario_id: sid,
    };
    return () => ask(cmd, "solve").then((o) => o.solve);
  });
  const needed = $derived(
    new Map<string, NeededScore>((solve.value?.needed_scores ?? []).map((n) => [n.assignment_id, n])),
  );

  function impactQuery(scenario: () => string | null | undefined) {
    return query(() => {
      const scenario_id = scenario();
      if (scenario_id === undefined) return null;
      const cmd = { command: "impact" as const, course_id: courseId, curve_mode: app.curveMode, scenario_id };
      return () => ask(cmd, "impact").then((o) => o.entries);
    });
  }
  const impact = impactQuery(() => sid);
  const realImpact = impactQuery(() => realSid);
  const moves = $derived(new Map((impact.value ?? []).map((e) => [e.assignment_id, e.delta ?? null])));

  const leverageQ = query(() => {
    const cmd = {
      command: "leverage" as const,
      course_id: courseId,
      baseline: null,
      curve_mode: app.curveMode,
      scenario_id: sid,
    };
    return () => ask(cmd, "leverage").then((o) => o.report.entries);
  });
  const leverage = $derived.by(() => {
    const entries = leverageQ.value ?? [];
    // Bar widths are layout only: each is drawn against the largest entry.
    const max = Math.max(0, ...entries.map((e) => e.leverage));
    return new Map(entries.map((e) => [e.assignment_id, { value: e.leverage, share: max > 0 ? e.leverage / max : 0 }]));
  });

  // Which rows the open what-if changes, and the override indices that undo each.
  const scenario = query(() => {
    if (!sid) return null;
    return () => ask({ command: "list_scenarios" }, "scenarios").then((o) => o.scenarios.find((s) => s.id === sid) ?? null);
  });
  const overrides = $derived.by(() => {
    const map = new Map<string, number[]>();
    const add = (id: string, i: number) => map.set(id, [...(map.get(id) ?? []), i]);
    (scenario.value?.overrides ?? []).forEach((o, i) => {
      if (o.type === "set_score" || o.type === "set_status" || o.type === "set_assignment_curves") add(o.assignment_id, i);
      if (o.type === "add_assignment") add(o.assignment.id, i);
    });
    return map;
  });
  const realScores = $derived.by(() => {
    if (!realCourse.value) return undefined;
    const map = new Map<string, number | null>();
    for (const k of realCourse.value.categories) for (const a of k.assignments) map.set(a.id, a.score ?? null);
    return map;
  });

  const meta = $derived.by(() => {
    const c = course.value;
    if (!c) return "";
    const out = [`${plain(c.credits)} ${c.credits === 1 ? "credit" : "credits"}`];
    if (c.code && c.code !== c.name) out.unshift(c.code);
    if (termName) out.push(termName);
    return out.join(" · ");
  });

  const fmtGrade = (r: { current?: number | null; current_letter?: string | null } | null) =>
    r ? `${pct(r.current)} ${r.current_letter ?? ""}`.trim() : "";
  const gradeMoved = $derived(!!sid && !!realGrade.value && fmtGrade(realGrade.value) !== fmtGrade(grade.value));
  const gpaMoved = $derived(!!sid && dec2(gpa.value) !== dec2(realGpa.value));

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
  {@const g = grade.value}
  <div class="page">
    <header>
      <h1 class="t-large ellipsis">{c.name}</h1>
      <p class="t-callout muted">{meta}</p>
    </header>

    <section class="summary" aria-label="Grade summary">
      <div class="hero">
        {#if gradeMoved && realGrade.value}
          <span class="was num" title="Real grade">{fmtGrade(realGrade.value)}</span>
          <span class="arrow" aria-hidden="true">→</span>
        {/if}
        {#if g?.current != null}
          <span class="t-hero num" class:accent={gradeMoved}>{pct(g.current)}</span>
          {#if g.current_letter}<span class="letter">{g.current_letter}</span>{/if}
        {:else}
          <span class="t-hero faint">{g ? "—" : " "}</span>
        {/if}
        {#if grade.error}<p class="error-text t-callout">{grade.error}</p>{/if}
      </div>

      <div class="stats">
        <TargetStat course={c} solve={solve.value} />
        <div class="stat" title="Your final grade if you score 0 on everything left">
          <span class="stat-label">Locked in</span>
          <span class="stat-value num">{pct(g?.projected)}</span>
        </div>
        {#if sid}
          <div class="stat" title="Cumulative GPA">
            <span class="stat-label">GPA</span>
            <span class="stat-value num">
              {#if gpaMoved}<span class="was-sm">{dec2(realGpa.value)} →</span>{/if}
              <span class:accent={gpaMoved}>{dec2(gpa.value)}</span>
            </span>
          </div>
        {/if}
      </div>
    </section>

    <section class="cols" aria-label="Assignments">
      {#each c.categories as category (category.id)}
        <CategoryCard
          {category}
          breakdown={g?.categories.find((b) => b.category_id === category.id)}
          needed={c.target != null ? needed : undefined}
          impact={moves}
          {leverage}
          {overrides}
          {realScores}
        />
      {/each}

      {#if addingCategory}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <form class="group" onsubmit={addCategory} onkeydown={(e) => e.key === "Escape" && (addingCategory = false)}>
          <div class="editor">
            <div class="editor-grid">
              <label class="field wide">
                <span>Category</span>
                <input use:focusOnMount bind:value={cat.name} placeholder="Homework" />
              </label>
              <label class="field">
                <span>Weight (%)</span>
                <input bind:value={cat.weight} inputmode="decimal" />
              </label>
              <label class="field">
                <span>Drop lowest</span>
                <input bind:value={cat.drop} inputmode="numeric" />
              </label>
            </div>
            <div class="editor-foot">
              <button type="button" class="btn" onclick={() => (addingCategory = false)}>Cancel</button>
              <button type="submit" class="btn prominent">Add</button>
            </div>
          </div>
        </form>
      {:else if !sid}
        <div class="add-cat">
          <button type="button" class="btn borderless" onclick={() => (addingCategory = true)}>Add category</button>
        </div>
      {/if}
    </section>

    <section class="trend" aria-label="Grade over time">
      <TrendChart entries={impact.value ?? []} real={sid ? (realImpact.value ?? []) : null} target={c.target ?? null} />
    </section>
  </div>
{:else if course.loading}
  <p class="muted">Loading...</p>
{/if}

<style>
  .summary {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--s4) var(--s8);
  }
  .hero {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 0 var(--s3);
  }
  .letter {
    font: var(--t-hero);
    font-weight: 400;
    color: var(--label-2);
  }
  .was {
    font: 500 22px/28px var(--font);
    color: var(--label-3);
  }
  .arrow {
    font: 400 22px/28px var(--font);
    color: var(--label-3);
  }
  .was-sm {
    color: var(--label-3);
    font-weight: 400;
  }
  .stats {
    display: flex;
    gap: var(--s8);
  }
  .add-cat {
    grid-column: 1 / -1;
  }
  .trend {
    padding-top: var(--s2);
  }
</style>
