<script lang="ts">
  import { onMount } from "svelte";
  import { execute, type OutputOf } from "./lib/api";

  let terms = $state<OutputOf<"terms">["terms"]>([]);
  let error = $state<string | null>(null);

  async function refresh() {
    try {
      const out = await execute({ command: "list_terms" });
      if (out.result === "terms") terms = out.terms;
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  onMount(refresh);
</script>

<div class="shell">
  <aside class="sidebar" aria-label="Terms">
    <h1>Tally</h1>
    {#if error}
      <p class="error">{error}</p>
    {:else if terms.length === 0}
      <p class="muted">No terms yet.</p>
    {:else}
      <ul>
        {#each terms as term (term.id)}
          <li>
            <button type="button" class="term">{term.name}</button>
            <ul>
              {#each term.courses as course (course.id)}
                <li><button type="button" class="course">{course.name}</button></li>
              {/each}
            </ul>
          </li>
        {/each}
      </ul>
    {/if}
  </aside>
  <main class="main">
    <p class="muted">Select a course.</p>
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 260px 1fr;
    height: 100%;
  }
  .sidebar {
    background: var(--bg-side);
    border-right: 1px solid var(--border);
    padding: 16px 12px;
    overflow-y: auto;
  }
  .main {
    padding: 24px 32px;
    overflow-y: auto;
  }
  h1 {
    font-size: 15px;
    font-weight: 600;
    margin: 0 8px 16px;
    letter-spacing: 0.02em;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  ul ul {
    padding-left: 12px;
  }
  button {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-radius: var(--radius);
    padding: 6px 8px;
    cursor: pointer;
  }
  button:hover {
    background: var(--hover);
  }
  .term {
    font-weight: 600;
    color: var(--fg-muted);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-top: 8px;
  }
  .muted {
    color: var(--fg-muted);
    margin: 0 8px;
  }
  .error {
    color: #d33;
    margin: 0 8px;
  }
</style>
