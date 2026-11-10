<script lang="ts">
  import { newId } from "../api";
  import { app, ask, mutate, navigate, query, toast } from "../app.svelte";
  import { dec2, parseNum, plain } from "../format";

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
  <form class="stack onboard" onsubmit={onboard}>
    <header>
      <h1 class="t-large">Welcome to Tally</h1>
    </header>
    <div class="group">
      <label class="group-row">
        <span class="grow">Term</span>
        <input use:focusOnMount class="mid right" bind:value={termName} placeholder="e.g. Fall 2026" />
      </label>
      <label class="group-row">
        <span class="grow">First course</span>
        <input class="mid right" bind:value={courseName} placeholder="optional" />
      </label>
      <label class="group-row">
        <span class="grow">Credits</span>
        <input class="narrow right" bind:value={credits} inputmode="decimal" />
      </label>
    </div>
    <div><button type="submit" class="btn prominent">Get started</button></div>
  </form>
{:else if terms.value}
  <div class="page">
    <header>
      <h1 class="t-large">Overview</h1>
    </header>
    <section class="stat">
      <span class="stat-label">GPA</span>
      <span class="t-hero num">{dec2(cumulative.value?.gpa)}</span>
    </section>
    <div class="cols">
      {#each terms.value as t (t.id)}
        <section>
          <h2 class="section-label">{t.name}</h2>
          <div class="group">
            {#if t.courses.length === 0}
              <div class="group-row muted">No courses yet</div>
            {/if}
            {#each t.courses as c (c.id)}
              <button
                type="button"
                class="group-row clickable course"
                onclick={() => navigate({ name: "course", id: c.id })}
              >
                <span class="grow">{c.name}</span>
                <span class="t-callout muted num">{plain(c.credits)} cr</span>
              </button>
            {/each}
          </div>
        </section>
      {/each}
    </div>
  </div>
{/if}

<style>
  .onboard {
    max-width: 480px;
    margin: var(--s8) auto;
  }
  .course {
    width: 100%;
    background: none;
    border: 0;
    text-align: left;
    color: inherit;
  }
</style>
