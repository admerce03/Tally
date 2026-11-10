<script lang="ts">
  import { app, ask, query } from "../app.svelte";
  import { LETTERS, pct, parseNum, plain } from "../format";
  import type { Command } from "../api";
  import type { Course, Letter } from "../generated/output";

  let { course }: { course: Course } = $props();

  type Mode = "course" | "percent" | "letter";
  let mode = $state<Mode>("percent");
  let percentText = $state("90");
  let letter = $state<Letter>("A");
  let withCurve = $state(false);
  let locks = $state<{ assignment_id: string; points: string }[]>([]);

  // Default to the course's own target when it has one.
  let seeded = false;
  $effect(() => {
    if (!seeded) {
      seeded = true;
      if (course.target != null) mode = "course";
    }
  });

  const ungraded = $derived(
    (course.categories ?? []).flatMap((c) =>
      (c.assignments ?? []).filter((a) => a.score == null).map((a) => ({ id: a.id, name: a.name, cat: c.name })),
    ),
  );

  function buildCommand(): Extract<Command, { command: "solve_target" }> | null {
    const locked = [];
    for (const l of locks) {
      const p = parseNum(l.points);
      if (!l.assignment_id || p === undefined) continue;
      locked.push({ assignment_id: l.assignment_id, points: p });
    }
    let target: Extract<Command, { command: "solve_target" }>["target"] = null;
    if (mode === "percent") {
      const p = parseNum(percentText);
      if (p === undefined) return null;
      target = { percent: p };
    } else if (mode === "letter") {
      target = { letter };
    }
    return {
      command: "solve_target",
      course_id: course.id,
      target,
      locked,
      with_curve: withCurve,
      scenario_id: app.scenarioId,
    };
  }

  const solve = query(() => {
    const cmd = buildCommand();
    return cmd ? () => ask(cmd, "solve").then((o) => o.solve) : null;
  });

  function addLock() {
    const first = ungraded.find((a) => !locks.some((l) => l.assignment_id === a.id));
    locks.push({ assignment_id: first?.id ?? "", points: "" });
  }
</script>

<div class="stack">
  <div class="card stack">
    <h3>Target grade</h3>
    <div class="cluster">
      <div class="seg" role="group" aria-label="Target type">
        {#if course.target != null}
          <button type="button" aria-pressed={mode === "course"} onclick={() => (mode = "course")}>
            Course target ({plain(course.target)}%)
          </button>
        {/if}
        <button type="button" aria-pressed={mode === "percent"} onclick={() => (mode = "percent")}>Percent</button>
        <button type="button" aria-pressed={mode === "letter"} onclick={() => (mode = "letter")}>Letter</button>
      </div>
      {#if mode === "percent"}
        <input class="narrow" bind:value={percentText} inputmode="decimal" aria-label="Target percent" />
      {:else if mode === "letter"}
        <select bind:value={letter} aria-label="Target letter">
          {#each LETTERS as l (l)}
            <option value={l}>{l}</option>
          {/each}
        </select>
      {/if}
      <label class="cluster">
        <input type="checkbox" bind:checked={withCurve} />
        <span>Include course curve</span>
      </label>
    </div>

    <div class="stack tight">
      <div class="spread">
        <span class="muted small">Locked expected scores (points) on specific assignments</span>
        <button type="button" class="btn sm" onclick={addLock} disabled={ungraded.length === 0}>+ Lock</button>
      </div>
      {#each locks as l, i (i)}
        <div class="cluster">
          <select bind:value={l.assignment_id} aria-label="Assignment to lock">
            {#each ungraded as a (a.id)}
              <option value={a.id}>{a.cat}: {a.name}</option>
            {/each}
          </select>
          <input class="narrow" bind:value={l.points} inputmode="decimal" placeholder="points" aria-label="Expected points" />
          <button type="button" class="btn ghost sm" onclick={() => locks.splice(i, 1)}>Remove</button>
        </div>
      {/each}
    </div>
  </div>

  <div class="card">
    {#if solve.error}
      <p class="error-text">{solve.error}</p>
    {:else if solve.value}
      {@const s = solve.value}
      {#if s.status.state === "secured"}
        <p class="result pos">Secured</p>
        <p class="muted">
          The target of {pct(s.target_percent)} is met even with 0% on the remaining {s.remaining_count}
          {s.remaining_count === 1 ? "assignment" : "assignments"}.
        </p>
      {:else if s.status.state === "on_track"}
        <p class="result">On track: need {pct(s.status.needed)}</p>
        <p class="muted">
          on the remaining {s.remaining_count}
          {s.remaining_count === 1 ? "assignment" : "assignments"} to reach {pct(s.target_percent)}.
        </p>
      {:else if s.status.needed != null}
        <p class="result neg">Unreachable</p>
        <p class="muted">
          Reaching {pct(s.target_percent)} would take {pct(s.status.needed)} on the remaining
          {s.remaining_count} {s.remaining_count === 1 ? "assignment" : "assignments"}, more than 100%.
        </p>
      {:else}
        <p class="result neg">Unreachable</p>
        <p class="muted">
          {s.remaining_count === 0
            ? "No remaining work: nothing left that can raise the grade."
            : `Reaching ${pct(s.target_percent)} is not possible with the remaining work.`}
        </p>
      {/if}
      {#if s.grade_at_needed != null}
        <p class="small muted">Resulting course grade: {pct(s.grade_at_needed)}</p>
      {/if}
    {:else if solve.loading}
      <p class="muted">Solving...</p>
    {:else}
      <p class="muted">Enter a valid target.</p>
    {/if}
  </div>
</div>

<style>
  .result {
    font-size: 20px;
    font-weight: 650;
    margin: 0 0 4px;
    font-variant-numeric: tabular-nums;
  }
  .card p {
    margin: 0;
  }
</style>
