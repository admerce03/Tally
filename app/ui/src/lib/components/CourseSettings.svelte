<script lang="ts">
  import { app, mutate, navigate, toast } from "../app.svelte";
  import { GPA_STATUSES, GPA_STATUS_LABEL, LETTERS, parseNum, parseOptNum } from "../format";
  import type { CourseView, GpaStatus, Letter, Override } from "../generated/output";
  import Alert from "./Alert.svelte";

  // Course pane of the Settings sheet. In a what-if, curve and GPA rows change the what-if only.
  let { course, onclose }: { course: CourseView; onclose: () => void } = $props();

  function init() {
    return {
      name: course.name,
      code: course.code ?? "",
      credits: String(course.credits),
      gpaStatus: (course.gpa_status ?? "normal") as GpaStatus,
      official: (course.official_letter ?? "") as Letter | "",
      addPoints: String(course.curve?.add_points ?? 0),
      shift: String(course.curve?.cutoff_shift ?? 0),
    };
  }
  let f = $state(init());
  let confirmDelete = $state(false);

  function cutoffText(letter: Letter): string {
    const hit = course.cutoffs.entries.find((e) => e.letter === letter);
    return hit ? String(hit.min) : "";
  }

  const inScenario = $derived(!!app.scenarioId);
  const LOCKED = "Real data only: finish the what-if to edit";

  async function override(o: Override): Promise<void> {
    if (app.scenarioId) await mutate({ command: "add_override", scenario_id: app.scenarioId, override: o });
  }

  async function commitName() {
    const name = f.name.trim();
    if (!name) {
      f.name = course.name;
      return toast("Name is required");
    }
    if (name !== course.name) await mutate({ command: "update_course", id: course.id, name });
  }

  async function commitCode() {
    const code = f.code.trim() || null;
    if (code !== (course.code ?? null)) await mutate({ command: "update_course", id: course.id, code });
  }

  async function commitCredits() {
    const credits = parseNum(f.credits);
    if (credits === undefined) {
      f.credits = String(course.credits);
      return toast("Credits must be a number");
    }
    if (credits !== course.credits) await mutate({ command: "update_course", id: course.id, credits });
  }

  async function commitCurve() {
    const add = parseNum(f.addPoints);
    const shift = parseNum(f.shift);
    if (add === undefined || shift === undefined) return toast("Curve values must be numbers");
    if (app.scenarioId)
      return override({ type: "set_course_curve", course_id: course.id, curve: { add_points: add, cutoff_shift: shift } });
    await mutate({ command: "set_course_curve", course_id: course.id, add_points: add, cutoff_shift: shift });
  }

  async function commitCutoff(letter: Letter, text: string) {
    if (app.scenarioId) return;
    const v = parseOptNum(text);
    if (v === undefined) return toast(`Cutoff for ${letter} must be a number`);
    await mutate({ command: "set_cutoff", course_id: course.id, letter, value: v });
  }

  async function commitGpaStatus() {
    if (app.scenarioId) return override({ type: "set_gpa_status", course_id: course.id, gpa_status: f.gpaStatus });
    await mutate({ command: "update_course", id: course.id, gpa_status: f.gpaStatus });
  }

  async function commitOfficial() {
    const letter = f.official || null;
    if (app.scenarioId) return override({ type: "set_official_letter", course_id: course.id, letter });
    await mutate({ command: "update_course", id: course.id, official_letter: letter });
  }

  async function removeCourse() {
    confirmDelete = false;
    if (await mutate({ command: "remove_course", id: course.id })) {
      onclose();
      navigate({ name: "home" });
    }
  }
</script>

<section>
  <div class="group">
    <label class="group-row">
      <span class="grow">Name</span>
      <input class="mid right" bind:value={f.name} disabled={inScenario} title={inScenario ? LOCKED : undefined} onchange={commitName} />
    </label>
    <label class="group-row">
      <span class="grow">Code</span>
      <input class="mid right" bind:value={f.code} placeholder="optional" disabled={inScenario} title={inScenario ? LOCKED : undefined} onchange={commitCode} />
    </label>
    <label class="group-row">
      <span class="grow">Credits</span>
      <input class="narrow right" bind:value={f.credits} inputmode="decimal" disabled={inScenario} title={inScenario ? LOCKED : undefined} onchange={commitCredits} />
    </label>
  </div>
</section>

<section>
  <h3 class="section-label">Curve</h3>
  <div class="group" class:hypo-group={inScenario}>
    <label class="group-row" title="Added to the final percent. A curve is an assumption, not data.">
      <span class="grow">Add points</span>
      <input class="narrow right" bind:value={f.addPoints} inputmode="decimal" onchange={commitCurve} />
    </label>
    <label class="group-row" title="Every letter cutoff moves down by this much.">
      <span class="grow">Lower cutoffs by</span>
      <input class="narrow right" bind:value={f.shift} inputmode="decimal" onchange={commitCurve} />
    </label>
  </div>
</section>

<section>
  <h3 class="section-label">Letter cutoffs</h3>
  <div class="group cutoffs">
    {#each LETTERS as l (l)}
      <label class="group-row" title="Minimum percent for {l}. Clear to remove the letter.">
        <span class="grow">{l}</span>
        <input
          class="narrow right"
          inputmode="decimal"
          aria-label="Minimum percent for {l}"
          value={cutoffText(l)}
          placeholder="off"
          disabled={inScenario}
          onchange={(e) => commitCutoff(l, e.currentTarget.value)}
        />
      </label>
    {/each}
  </div>
</section>

<section>
  <h3 class="section-label">GPA</h3>
  <div class="group" class:hypo-group={inScenario}>
    <label class="group-row">
      <span class="grow">Status</span>
      <select bind:value={f.gpaStatus} onchange={commitGpaStatus}>
        {#each GPA_STATUSES as s (s)}
          <option value={s}>{GPA_STATUS_LABEL[s]}</option>
        {/each}
      </select>
    </label>
    <label class="group-row">
      <span class="grow">Official letter</span>
      <select bind:value={f.official} onchange={commitOfficial}>
        <option value="">From current grade</option>
        {#each LETTERS as l (l)}
          <option value={l}>{l}</option>
        {/each}
      </select>
    </label>
  </div>
</section>

<section>
  <button
    type="button"
    class="btn destructive"
    disabled={inScenario}
    title={inScenario ? LOCKED : undefined}
    onclick={() => (confirmDelete = true)}
  >
    Delete course…
  </button>
</section>

{#if confirmDelete}
  <Alert
    title="Delete {course.name}?"
    message="This also deletes its categories and assignments. This can't be undone."
    onconfirm={removeCourse}
    oncancel={() => (confirmDelete = false)}
  />
{/if}

<style>
  /* Two columns of cutoffs keep twelve letters from becoming a long scroll. */
  .cutoffs {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .cutoffs .group-row:nth-child(2)::before {
    content: none;
  }
</style>
