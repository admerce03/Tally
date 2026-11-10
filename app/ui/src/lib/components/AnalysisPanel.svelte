<script lang="ts">
  import { app, ask, query } from "../app.svelte";
  import { dec2, parseOptNum, pct, signClass, signed } from "../format";
  import TrendChart from "./TrendChart.svelte";

  let { courseId }: { courseId: string } = $props();

  let baselineText = $state("");
  const baseline = $derived(parseOptNum(baselineText));

  const impact = query(() => {
    const cmd = {
      command: "impact" as const,
      course_id: courseId,
      curve_mode: app.curveMode,
      scenario_id: app.scenarioId,
    };
    return () => ask(cmd, "impact").then((o) => o.entries);
  });

  const trends = query(() => {
    const cmd = {
      command: "trends" as const,
      course_id: courseId,
      curve_mode: app.curveMode,
      scenario_id: app.scenarioId,
    };
    return () => ask(cmd, "trends").then((o) => o.points);
  });

  const leverage = query(() => {
    if (baseline === undefined) return null;
    const cmd = {
      command: "leverage" as const,
      course_id: courseId,
      baseline,
      curve_mode: app.curveMode,
      scenario_id: app.scenarioId,
    };
    return () => ask(cmd, "leverage").then((o) => o.report);
  });

  // Bar widths are layout only: each bar is scaled against the largest entry.
  const maxLeverage = $derived(Math.max(0, ...(leverage.value?.entries ?? []).map((e) => e.leverage)));
</script>

<div class="stack">
  <section class="card stack tight">
    <h3>Trend</h3>
    {#if trends.error}
      <p class="error-text">{trends.error}</p>
    {:else if trends.value && trends.value.length > 0}
      <TrendChart points={trends.value} />
    {:else}
      <p class="muted">The trend appears once there is graded work.</p>
    {/if}
  </section>

  <section class="card flush">
    <div class="card-head"><h3>Impact of each graded assignment</h3></div>
    {#if impact.error}
      <p class="error-text card-body">{impact.error}</p>
    {:else if impact.value && impact.value.length > 0}
      <table>
        <thead>
          <tr>
            <th>Date</th>
            <th>Assignment</th>
            <th class="num">Before</th>
            <th class="num">After</th>
            <th class="num">Change</th>
          </tr>
        </thead>
        <tbody>
          {#each impact.value as e (e.assignment_id)}
            <tr>
              <td class="muted num" style="text-align:left">{e.due ?? "—"}</td>
              <td>{e.name}</td>
              <td class="num">{pct(e.grade_before)}</td>
              <td class="num">{pct(e.grade_after)}</td>
              <td class="num {signClass(e.delta)}">{signed(e.delta)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="muted card-body">No graded assignments yet.</p>
    {/if}
  </section>

  <section class="card stack tight">
    <div class="spread">
      <h3>Leverage of remaining work</h3>
      <label class="cluster">
        <span class="muted small">Baseline for other work (%)</span>
        <input
          class="narrow"
          bind:value={baselineText}
          inputmode="decimal"
          placeholder={leverage.value ? dec2(leverage.value.baseline_used) : "default"}
          aria-label="Baseline percent"
        />
      </label>
    </div>
    <p class="muted small">
      Final-grade points gained per 10% scored on each remaining assignment
      {#if leverage.value}(baseline {pct(leverage.value.baseline_used)}){/if}.
    </p>
    {#if baseline === undefined}
      <p class="error-text">Baseline must be a number.</p>
    {:else if leverage.error}
      <p class="error-text">{leverage.error}</p>
    {:else if leverage.value && leverage.value.entries.length > 0}
      <ul class="bars">
        {#each leverage.value.entries as e (e.assignment_id)}
          <li>
            <span class="name">{e.name}</span>
            <span class="track">
              <span class="bar" style:width="{maxLeverage > 0 ? (e.leverage / maxLeverage) * 100 : 0}%"></span>
            </span>
            <span class="num val">{dec2(e.leverage)}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">No remaining assignments.</p>
    {/if}
  </section>
</div>

<style>
  .bars {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    display: grid;
    grid-template-columns: minmax(80px, 200px) 1fr 64px;
    gap: 12px;
    align-items: center;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track {
    background: var(--hover);
    border-radius: 999px;
    height: 10px;
    overflow: hidden;
  }
  .bar {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
  }
  .val {
    font-weight: 600;
  }
</style>
