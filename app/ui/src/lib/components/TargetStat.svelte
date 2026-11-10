<script lang="ts">
  import { app, mutate, toast } from "../app.svelte";
  import { ceil1, parseOptNum, pct } from "../format";
  import type { CourseView, SolveResult } from "../generated/output";
  import Popover from "./Popover.svelte";

  // The target as a stat. Its color is the whole message: green secured, red out of reach.
  let { course, solve }: { course: CourseView; solve: SolveResult | null } = $props();

  let open = $state(false);
  let text = $state("");

  const inScenario = $derived(!!app.scenarioId);

  const tone = $derived.by(() => {
    const s = solve?.status;
    if (!s) return "";
    if (s.state === "secured") return "pos";
    if (s.state === "unreachable") return "neg";
    return "";
  });

  const tip = $derived.by(() => {
    if (inScenario) return "Edit the target outside a what-if";
    const s = solve?.status;
    if (!s) return "Edit target";
    if (s.state === "secured") return "Secured, even with 0 on what's left";
    if (s.state === "on_track") return `${ceil1(s.needed)}% needed on what's left`;
    return s.needed == null ? "Out of reach" : `Out of reach: ${ceil1(s.needed)}% needed on what's left`;
  });

  function show() {
    text = course.target == null ? "" : String(course.target);
    open = true;
  }

  async function commit(value: number | null | undefined) {
    if (value === undefined) return toast("Target must be a number");
    open = false;
    if ((value ?? null) !== (course.target ?? null))
      await mutate({ command: "set_target", course_id: course.id, target: value });
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commit(parseOptNum(text));
    }
  }
</script>

<div class="stat">
  <span class="stat-label">Target</span>
  <span class="anchor">
    {#if course.target == null}
      <button type="button" class="value-btn stat-value accent" disabled={inScenario} onclick={show}>Set</button>
    {:else}
      <button type="button" class="value-btn stat-value num {tone}" title={tip} disabled={inScenario} onclick={show}>
        {pct(course.target)}
      </button>
    {/if}
    {#if open}
      <Popover end onclose={() => (open = false)}>
        <div class="stack tight">
          <label class="field">
            <span>Target grade (%)</span>
            <input bind:value={text} inputmode="decimal" placeholder="e.g. 90" onkeydown={key} />
          </label>
          <div class="editor-foot">
            {#if course.target != null}
              <button type="button" class="btn destructive lead" onclick={() => commit(null)}>Remove</button>
            {/if}
            <button type="button" class="btn prominent" onclick={() => commit(parseOptNum(text))}>Done</button>
          </div>
        </div>
      </Popover>
    {/if}
  </span>
</div>
