<script lang="ts">
  import { newId } from "../api";
  import { app, ask, mutate, navigate, query, toast } from "../app.svelte";
  import { dec2, parseNum } from "../format";

  const terms = query(() => () => ask({ command: "list_terms" }, "terms").then((o) => o.terms));
  const cumulative = query(() => {
    const scenario_id = app.scenarioId;
    return () => ask({ command: "cumulative_gpa", scenario_id }, "cumulative_gpa").then((o) => o.report);
  });

  let termName = $state("");
  let courseName = $state("");
  let credits = $state("3");

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }

  async function onboard(e: Event) {
    e.preventDefault();
    const tn = termName.trim();
    const cn = courseName.trim();
    const cr = parseNum(credits);
    if (!tn) return toast("Give your first term a name");
    if (cn && cr === undefined) return toast("Credits must be a number");
    const termId = newId();
    if (!(await mutate({ command: "add_term", id: termId, name: tn }))) return;
    if (cn && cr !== undefined) {
      const id = newId();
      if (await mutate({ command: "add_course", term_id: termId, id, name: cn, credits: cr })) {
        navigate({ name: "course", id });
      }
    }
  }
</script>

{#if terms.error}
  <div class="empty">{terms.error}</div>
{:else if terms.value && terms.value.length === 0}
  <div class="card onboard stack">
    <h2>Welcome to Tally</h2>
    <p class="muted">Start by adding a term and your first course. You can add categories and assignments next.</p>
    <form class="stack tight" onsubmit={onboard}>
      <label class="field">
        <span>Term name</span>
        <input use:focusOnMount bind:value={termName} placeholder="e.g. Fall 2026" />
      </label>
      <div class="cluster">
        <label class="field grow">
          <span>First course (optional)</span>
          <input bind:value={courseName} placeholder="e.g. Calculus I" />
        </label>
        <label class="field">
          <span>Credits</span>
          <input class="narrow" bind:value={credits} inputmode="decimal" />
        </label>
      </div>
      <div><button type="submit" class="btn primary">Get started</button></div>
    </form>
  </div>
{:else if terms.value}
  <div class="stack">
    <h2>Overview</h2>
    <div class="card gpa">
      <span class="muted small">Cumulative GPA</span>
      <span class="big num">{dec2(cumulative.value?.gpa)}</span>
    </div>
    {#each terms.value as t (t.id)}
      <section class="card stack tight">
        <h3>{t.name}</h3>
        {#if t.courses.length === 0}
          <p class="muted">No courses yet. Use "+ Course" under this term in the sidebar.</p>
        {:else}
          <div class="cluster">
            {#each t.courses as c (c.id)}
              <button type="button" class="btn" onclick={() => navigate({ name: "course", id: c.id })}>{c.name}</button>
            {/each}
          </div>
        {/if}
      </section>
    {/each}
  </div>
{/if}

<style>
  .onboard {
    max-width: 520px;
    margin: 48px auto;
    padding: 24px;
  }
  p {
    margin: 0;
  }
  .gpa {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }
  .big {
    font-size: 32px;
    font-weight: 650;
    text-align: left;
  }
</style>
