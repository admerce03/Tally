<script lang="ts">
  import { ask, query } from "../app.svelte";
  import { dec2, pct } from "../format";
  import type { CompareCourse, CompareTerm } from "../generated/output";

  // Empty scenario_ids = compare real data with every scenario.
  const cmp = query(
    () => () => ask({ command: "compare_scenarios", scenario_ids: [] }, "comparison").then((o) => o.columns),
  );

  // Row keys: union across columns, first-seen order (layout only).
  const courseRows = $derived.by(() => {
    const seen = new Map<string, string>();
    for (const col of cmp.value ?? []) for (const c of col.courses) if (!seen.has(c.course_id)) seen.set(c.course_id, c.name);
    return [...seen];
  });
  const termRows = $derived.by(() => {
    const seen = new Map<string, string>();
    for (const col of cmp.value ?? []) for (const t of col.terms) if (!seen.has(t.term_id)) seen.set(t.term_id, t.name);
    return [...seen];
  });

  function course(cols: CompareCourse[], id: string): CompareCourse | undefined {
    return cols.find((c) => c.course_id === id);
  }
  function term(ts: CompareTerm[], id: string): CompareTerm | undefined {
    return ts.find((t) => t.term_id === id);
  }
</script>

<section class="card flush">
  <div class="card-head"><h3>Compare side by side</h3></div>
  {#if cmp.error}
    <p class="error-text card-body">{cmp.error}</p>
  {:else if cmp.value && cmp.value.length > 0}
    <div class="scroll">
      <table>
        <thead>
          <tr>
            <th></th>
            {#each cmp.value as col, i (col.scenario_id ?? `real-${i}`)}
              <th class="num">{col.scenario_id ? col.name : "Real data"}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#if cmp.value.some((c) => c.error)}
            <tr>
              <td class="muted">Errors</td>
              {#each cmp.value as col, i (col.scenario_id ?? `real-${i}`)}
                <td class="num error-text wrap">{col.error ?? ""}</td>
              {/each}
            </tr>
          {/if}
          <tr class="group"><td colspan={cmp.value.length + 1}>Courses (current / projected)</td></tr>
          {#each courseRows as [id, name] (id)}
            <tr>
              <td>{name}</td>
              {#each cmp.value as col, i (col.scenario_id ?? `real-${i}`)}
                {@const c = course(col.courses, id)}
                <td class="num">
                  {#if c}
                    <div>{pct(c.current)} {c.current_letter ?? ""}</div>
                    <div class="muted small">{pct(c.projected)} {c.projected_letter ?? ""}</div>
                  {:else}
                    <span class="muted">{"—"}</span>
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
          <tr class="group"><td colspan={cmp.value.length + 1}>Term GPA</td></tr>
          {#each termRows as [id, name] (id)}
            <tr>
              <td>{name}</td>
              {#each cmp.value as col, i (col.scenario_id ?? `real-${i}`)}
                <td class="num">{dec2(term(col.terms, id)?.gpa)}</td>
              {/each}
            </tr>
          {/each}
          <tr class="group total">
            <td>Cumulative GPA</td>
            {#each cmp.value as col, i (col.scenario_id ?? `real-${i}`)}
              <td class="num">{dec2(col.cumulative_gpa)}</td>
            {/each}
          </tr>
        </tbody>
      </table>
    </div>
  {:else}
    <p class="muted card-body">Nothing to compare yet.</p>
  {/if}
</section>

<style>
  .scroll {
    overflow-x: auto;
  }
  .group td {
    font-size: 12px;
    color: var(--fg-muted);
    background: var(--bg-side);
  }
  .total td {
    font-weight: 600;
    color: var(--fg);
  }
  .wrap {
    white-space: normal;
    max-width: 220px;
  }
</style>
