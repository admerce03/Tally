<script lang="ts">
  import { syncAvailable } from "../api";
  import { app, ask, mutate, query, setCurveMode, setGroupByYear, setSolverCurve, toast } from "../app.svelte";
  import { LETTERS, parseNum } from "../format";
  import SyncSettings from "./SyncSettings.svelte";

  // App pane of the Settings sheet: settings that apply to every course.
  const scale = query(() => () => ask({ command: "show_gpa_scale" }, "gpa_scale").then((o) => o.scale));

  function scaleText(letter: (typeof LETTERS)[number]): string {
    const hit = scale.value?.points.find((e) => e.letter === letter);
    return hit ? String(hit.points) : "";
  }

  async function saveScale(letter: (typeof LETTERS)[number], text: string) {
    const points = parseNum(text);
    if (points === undefined) return toast(`Grade points for ${letter} must be a number`);
    await mutate({ command: "set_gpa_scale_entry", letter, points });
  }
</script>

<section>
  <h3 class="section-label">Curves</h3>
  <div class="group">
    <label class="group-row" title="Curves are assumptions. Turn this off to see grades as they stand.">
      <span class="grow">Show grades with curves</span>
      <input
        type="checkbox"
        checked={app.curveMode === "apply"}
        onchange={(e) => setCurveMode(e.currentTarget.checked ? "apply" : "ignore")}
      />
    </label>
    <label class="group-row" title="Needed scores ignore the expected course curve unless this is on.">
      <span class="grow">Count curves in needed scores</span>
      <input type="checkbox" checked={app.solverCurve} onchange={(e) => setSolverCurve(e.currentTarget.checked)} />
    </label>
  </div>
</section>

<section>
  <h3 class="section-label">Sidebar</h3>
  <div class="group">
    <label class="group-row" title="Set a term's year from its ⋯ menu in the sidebar.">
      <span class="grow">Group terms by year</span>
      <input type="checkbox" checked={app.groupByYear} onchange={(e) => setGroupByYear(e.currentTarget.checked)} />
    </label>
  </div>
</section>

<section>
  <h3 class="section-label">GPA scale</h3>
  <div class="group scale">
    {#each LETTERS as l (l)}
      <label class="group-row">
        <span class="grow">{l}</span>
        <input
          class="narrow right"
          inputmode="decimal"
          aria-label="Grade points for {l}"
          value={scaleText(l)}
          onchange={(e) => saveScale(l, e.currentTarget.value)}
        />
      </label>
    {/each}
  </div>
</section>

{#if syncAvailable}
  <SyncSettings />
{/if}

<style>
  .scale {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .scale .group-row:nth-child(2)::before {
    content: none;
  }
</style>
