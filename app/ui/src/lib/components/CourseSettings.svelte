<script lang="ts">
  import { mutate, toast } from "../app.svelte";
  import { GPA_STATUSES, GPA_STATUS_LABEL, LETTERS, parseNum, parseOptNum } from "../format";
  import type { CourseView, GpaStatus, Letter } from "../generated/output";
  import Drawer from "./Drawer.svelte";

  let { course, onclose }: { course: CourseView; onclose: () => void } = $props();

  function init() {
    return {
      name: course.name,
      code: course.code ?? "",
      credits: String(course.credits),
      target: course.target == null ? "" : String(course.target),
      gpaStatus: (course.gpa_status ?? "normal") as GpaStatus,
      official: (course.official_letter ?? "") as Letter | "",
      addPoints: String(course.curve?.add_points ?? 0),
      shift: String(course.curve?.cutoff_shift ?? 0),
    };
  }
  let f = $state(init());

  function cutoffText(letter: Letter): string {
    const hit = course.cutoffs.entries.find((e) => e.letter === letter);
    return hit ? String(hit.min) : "";
  }

  async function saveGeneral(e: Event) {
    e.preventDefault();
    const credits = parseNum(f.credits);
    const target = parseOptNum(f.target);
    if (!f.name.trim()) return toast("Name is required");
    if (credits === undefined) return toast("Credits must be a number");
    if (target === undefined) return toast("Target must be a number");
    await mutate({
      command: "update_course",
      id: course.id,
      name: f.name.trim(),
      code: f.code.trim() || null,
      credits,
      target,
      gpa_status: f.gpaStatus,
      official_letter: f.official || null,
    });
  }

  async function saveCurve(e: Event) {
    e.preventDefault();
    const add = parseNum(f.addPoints);
    const shift = parseNum(f.shift);
    if (add === undefined || shift === undefined) return toast("Curve values must be numbers");
    await mutate({ command: "set_course_curve", course_id: course.id, add_points: add, cutoff_shift: shift });
  }

  async function saveCutoff(letter: Letter, text: string) {
    const v = parseOptNum(text);
    if (v === undefined) return toast(`Cutoff for ${letter} must be a number`);
    await mutate({ command: "set_cutoff", course_id: course.id, letter, value: v });
  }
</script>

<Drawer title="Course settings" {onclose}>
  <form class="stack tight" onsubmit={saveGeneral}>
    <h3>General</h3>
    <label class="field">
      <span>Name</span>
      <input bind:value={f.name} />
    </label>
    <div class="cluster">
      <label class="field grow">
        <span>Code</span>
        <input bind:value={f.code} placeholder="optional" />
      </label>
      <label class="field">
        <span>Credits</span>
        <input class="narrow" bind:value={f.credits} inputmode="decimal" />
      </label>
      <label class="field">
        <span>Target (%)</span>
        <input class="narrow" bind:value={f.target} inputmode="decimal" placeholder="none" />
      </label>
    </div>
    <div class="cluster">
      <label class="field grow">
        <span>GPA status</span>
        <select bind:value={f.gpaStatus}>
          {#each GPA_STATUSES as s (s)}
            <option value={s}>{GPA_STATUS_LABEL[s]}</option>
          {/each}
        </select>
      </label>
      <label class="field grow">
        <span>Official letter</span>
        <select bind:value={f.official}>
          <option value="">Computed from grade</option>
          {#each LETTERS as l (l)}
            <option value={l}>{l}</option>
          {/each}
        </select>
      </label>
    </div>
    <div><button type="submit" class="btn primary">Save</button></div>
  </form>

  <form class="stack tight" onsubmit={saveCurve}>
    <h3>Course curve</h3>
    <p class="muted small">An assumption, not data: adds points to the final percent and lowers every cutoff.</p>
    <div class="cluster">
      <label class="field">
        <span>Add points</span>
        <input class="narrow" bind:value={f.addPoints} inputmode="decimal" />
      </label>
      <label class="field">
        <span>Cutoff shift</span>
        <input class="narrow" bind:value={f.shift} inputmode="decimal" />
      </label>
      <button type="submit" class="btn">Save curve</button>
    </div>
  </form>

  <div class="stack tight">
    <h3>Letter cutoffs</h3>
    <p class="muted small">Minimum percent per letter. Press Enter to save; clear a field to remove that letter.</p>
    <div class="grid">
      {#each LETTERS as l (l)}
        <label class="field">
          <span>{l}</span>
          <input
            class="narrow"
            inputmode="decimal"
            value={cutoffText(l)}
            onchange={(e) => saveCutoff(l, e.currentTarget.value)}
          />
        </label>
      {/each}
    </div>
  </div>
</Drawer>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .grid input {
    width: 100%;
  }
</style>
