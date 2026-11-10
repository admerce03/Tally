<script lang="ts">
  import { app, ask, mutate, navigate, query, toast } from "../app.svelte";
  import { EXCLUSION_LABEL, LETTERS, dec2, entriesOf, parseNum, plain } from "../format";

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

  const scale = query(() => () => ask({ command: "show_gpa_scale" }, "gpa_scale").then((o) => o.scale));

  function scaleText(letter: (typeof LETTERS)[number]): string {
    const hit = entriesOf(scale.value?.points).find(([l]) => l === letter);
    return hit ? String(hit[1]) : "";
  }

  async function saveScale(letter: (typeof LETTERS)[number], text: string) {
    const points = parseNum(text);
    if (points === undefined) return toast(`Grade points for ${letter} must be a number`);
    await mutate({ command: "set_gpa_scale_entry", letter, points });
  }
</script>

<div class="stack">
  <header class="spread">
    <h2>GPA</h2>
    {#if report.value}
      <div class="cumulative">
        <span class="muted small">Cumulative GPA</span>
        <span class="big num">{dec2(report.value.cumulative.gpa)}</span>
        <span class="muted small">{plain(report.value.cumulative.credits)} credits</span>
      </div>
    {/if}
  </header>

  {#if report.error}
    <div class="empty">{report.error}</div>
  {:else if report.value && report.value.termReports.length === 0}
    <div class="empty">No terms yet. Add a term and courses from the sidebar to see your GPA.</div>
  {:else if report.value}
    {#each report.value.termReports as t (t.term_id)}
      <section class="card flush">
        <div class="card-head">
          <h3>{t.name}</h3>
          <span class="muted">Term GPA <strong class="num">{dec2(t.gpa)}</strong> &middot; {plain(t.credits)} credits</span>
        </div>
        {#if t.courses.length === 0}
          <p class="muted card-body">No courses in this term.</p>
        {:else}
          <table>
            <thead>
              <tr>
                <th>Course</th>
                <th class="num">Credits</th>
                <th class="num">Letter</th>
                <th class="num">Points</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each t.courses as c (c.course_id)}
                <tr class:excluded={!c.included}>
                  <td>
                    <button type="button" class="link" onclick={() => navigate({ name: "course", id: c.course_id })}>
                      {c.name}
                    </button>
                  </td>
                  <td class="num">{plain(c.credits)}</td>
                  <td class="num">{c.letter ?? "—"}</td>
                  <td class="num">{dec2(c.points)}</td>
                  <td>
                    {#if c.included}
                      <span class="pill accent">Included</span>
                    {:else}
                      <span class="pill">Excluded{c.exclusion ? `: ${EXCLUSION_LABEL[c.exclusion]}` : ""}</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </section>
    {/each}
  {/if}

  <section class="card stack tight">
    <h3>GPA scale</h3>
    <p class="muted small">Grade points per letter. Changes apply to every GPA. Press Enter or leave the field to save.</p>
    <div class="grid">
      {#each LETTERS as l (l)}
        <label class="field">
          <span>{l}</span>
          <input
            inputmode="decimal"
            value={scaleText(l)}
            onchange={(e) => saveScale(l, e.currentTarget.value)}
          />
        </label>
      {/each}
    </div>
  </section>
</div>

<style>
  .cumulative {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }
  .big {
    font-size: 32px;
    font-weight: 650;
    line-height: 1.1;
  }
  .excluded td {
    color: var(--fg-muted);
  }
  .link {
    background: none;
    border: 0;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
    text-align: left;
  }
  .link:hover {
    text-decoration: underline;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(72px, 1fr));
    gap: 10px;
  }
  .grid input {
    width: 100%;
  }
  strong {
    font-variant-numeric: tabular-nums;
  }
</style>
