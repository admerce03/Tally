<script lang="ts">
  import { dec2, pct } from "../format";
  import type { ScenarioColumn } from "../generated/output";

  // Real data and every what-if side by side, only the rows where some what-if differs from real.
  // Columns come from core compare_scenarios; a cell that differs from real is accent.
  let { columns }: { columns: ScenarioColumn[] } = $props();

  type Row = { key: string; label: string; cell: (col: ScenarioColumn) => string; total?: boolean };

  const real = $derived(columns.find((c) => c.scenario_id == null) ?? null);
  const cols = $derived(real ? [real, ...columns.filter((c) => c !== real)] : columns);

  // Row keys: union across columns, first-seen order (layout only).
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const seen = new Set<string>();
    for (const col of cols)
      for (const c of col.courses)
        if (!seen.has(c.course_id)) {
          seen.add(c.course_id);
          out.push({
            key: c.course_id,
            label: c.name,
            cell: (k) => {
              const x = k.courses.find((y) => y.course_id === c.course_id);
              return x ? `${pct(x.current)} ${x.current_letter ?? ""}`.trim() : "—";
            },
          });
        }
    for (const col of cols)
      for (const t of col.terms)
        if (!seen.has(t.term_id)) {
          seen.add(t.term_id);
          out.push({
            key: t.term_id,
            label: `${t.name} GPA`,
            cell: (k) => dec2(k.terms.find((y) => y.term_id === t.term_id)?.gpa),
          });
        }
    out.push({ key: "cumulative", label: "Cumulative GPA", cell: (k) => dec2(k.cumulative_gpa), total: true });
    if (!real) return out;
    return out.filter((r) => r.total || cols.some((c) => c !== real && !c.error && r.cell(c) !== r.cell(real)));
  });
</script>

{#if cols.length > 1}
  <div class="scroll">
    <table>
      <thead>
        <tr>
          <th></th>
          {#each cols as col, i (col.scenario_id ?? `real-${i}`)}
            <th class="num">{col.scenario_id ? col.name : "Real"}</th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each rows as r (r.key)}
          {@const base = r.cell(cols[0])}
          <tr class:total={r.total}>
            <td>{r.label}</td>
            {#each cols as col, i (col.scenario_id ?? `real-${i}`)}
              {@const v = col.error ? "" : r.cell(col)}
              <td class="num" class:hypo-val={i > 0 && v !== base}>{v}</td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .scroll {
    overflow-x: auto;
  }
  table {
    width: auto;
    min-width: min(100%, 560px);
  }
  th.num {
    white-space: normal;
    max-width: 140px;
    vertical-align: bottom;
  }
  .total td {
    font-weight: 600;
  }
</style>
