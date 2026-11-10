<script lang="ts">
  import { app, ask, navigate, query } from "../app.svelte";
  import { EXCLUSION_LABEL, dec2, plain } from "../format";

  const report = query(() => {
    const scenario_id = app.scenarioId;
    return async () => {
      const terms = (await ask({ command: "list_terms" }, "terms")).terms;
      const termReports = await Promise.all(
        terms.map((t) => ask({ command: "term_gpa", term_id: t.id, scenario_id }, "term_gpa").then((o) => o.report)),
      );
      const cumulative = (await ask({ command: "cumulative_gpa", scenario_id }, "cumulative_gpa")).report;
      return { termReports, cumulative };
    };
  });
</script>

<div class="page">
  <header>
    <h1 class="t-large">GPA</h1>
  </header>

  {#if report.error}
    <div class="empty">{report.error}</div>
  {:else if report.value}
    <section class="stats" aria-label="Cumulative GPA">
      <div class="stat">
        <span class="stat-label">Cumulative</span>
        <span class="t-hero num">{dec2(report.value.cumulative.gpa)}</span>
      </div>
      <div class="stat">
        <span class="stat-label">Credits</span>
        <span class="stat-value num">{plain(report.value.cumulative.credits)}</span>
      </div>
    </section>

    {#if report.value.termReports.length === 0}
      <div class="empty">No terms yet</div>
    {/if}

    <div class="cols">
      {#each report.value.termReports as t (t.term_id)}
        <section>
          <div class="section-label spread">
            <h2 class="t-headline">{t.name}</h2>
            <span class="t-headline num" title="Term GPA · {plain(t.credits)} credits">{dec2(t.gpa)}</span>
          </div>
          <div class="group">
            {#if t.courses.length === 0}
              <div class="group-row muted">No courses in this term.</div>
            {/if}
            {#each t.courses as c (c.course_id)}
              <button
                type="button"
                class="group-row clickable course"
                onclick={() => navigate({ name: "course", id: c.course_id })}
              >
                <span class="grow">
                  <span class:muted={!c.included}>{c.name}</span>
                  {#if !c.included}
                    <span class="t-callout muted block">
                      Excluded{c.exclusion ? ` · ${EXCLUSION_LABEL[c.exclusion]}` : ""}
                    </span>
                  {/if}
                </span>
                <span class="t-callout muted num">{plain(c.credits)} cr</span>
                <span class="letter num">{c.letter ?? "—"}</span>
                <span class="pts num muted">{dec2(c.points)}</span>
              </button>
            {/each}
          </div>
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .course {
    width: 100%;
    background: none;
    border: 0;
    text-align: left;
    color: inherit;
  }
  .stats {
    display: flex;
    align-items: flex-end;
    gap: var(--s8);
  }
  .block {
    display: block;
  }
  .letter {
    min-width: 32px;
    text-align: right;
  }
  .pts {
    min-width: 40px;
    text-align: right;
  }
</style>
