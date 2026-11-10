<script lang="ts">
  import { app, ask, query } from "./lib/app.svelte";
  import CoursePage from "./lib/components/CoursePage.svelte";
  import GpaPage from "./lib/components/GpaPage.svelte";
  import HomePage from "./lib/components/HomePage.svelte";
  import ScenariosPage from "./lib/components/ScenariosPage.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Toasts from "./lib/components/Toasts.svelte";

  const scenarios = query(
    () => () => ask({ command: "list_scenarios" }, "scenarios").then((o) => o.scenarios),
  );
  const active = $derived(scenarios.value?.find((s) => s.id === app.scenarioId) ?? null);
</script>

<div class="shell">
  <Sidebar />
  <main class="main">
    {#if active}
      <div class="banner" role="status">
        <span>
          Viewing through scenario <strong>{active.name}</strong>. Grades and GPA are hypothetical. Score, status, curve
          and new-assignment edits are saved as overrides in this scenario; real data is not changed. Course and
          category settings still edit real data.
        </span>
        <button type="button" class="btn sm" onclick={() => (app.scenarioId = null)}>Back to real data</button>
      </div>
    {/if}
    <div class="content">
      {#if app.route.name === "course"}
        {#key app.route.id}
          <CoursePage courseId={app.route.id} />
        {/key}
      {:else if app.route.name === "gpa"}
        <GpaPage />
      {:else if app.route.name === "scenarios"}
        <ScenariosPage />
      {:else}
        <HomePage />
      {/if}
    </div>
  </main>
</div>
<Toasts />

<style>
  .shell {
    display: grid;
    grid-template-columns: 260px 1fr;
    height: 100%;
  }
  .main {
    overflow-y: auto;
    padding: 24px 32px 48px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .content {
    width: 100%;
    max-width: 960px;
    margin: 0 auto;
  }
  @media (max-width: 720px) {
    .shell {
      grid-template-columns: 1fr;
      grid-template-rows: auto 1fr;
    }
    .main {
      padding: 16px;
    }
  }
</style>
