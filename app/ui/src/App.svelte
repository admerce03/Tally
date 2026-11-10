<script lang="ts">
  import { onMount } from "svelte";
  import { syncAvailable } from "./lib/api";
  import { app, ask, query } from "./lib/app.svelte";
  import { startSync } from "./lib/sync.svelte";
  import CoursePage from "./lib/components/CoursePage.svelte";
  import GpaPage from "./lib/components/GpaPage.svelte";
  import HomePage from "./lib/components/HomePage.svelte";
  import ScenariosPage from "./lib/components/ScenariosPage.svelte";
  import Settings from "./lib/components/Settings.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import Toolbar from "./lib/components/Toolbar.svelte";
  import { chrome } from "./lib/platform";

  const scenarios = query(
    () => () => ask({ command: "list_scenarios" }, "scenarios").then((o) => o.scenarios),
  );
  const active = $derived(scenarios.value?.find((s) => s.id === app.scenarioId) ?? null);

  // A deleted scenario (e.g. removed on another device) can no longer be the active one. Wait for a
  // fresh list: one just created is not in the list that was loaded before it.
  $effect(() => {
    const list = scenarios.value;
    if (scenarios.loading || !list || !app.scenarioId) return;
    if (!list.some((s) => s.id === app.scenarioId)) app.scenarioId = null;
  });

  onMount(() => (syncAvailable ? startSync() : undefined));

  function key(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === ",") {
      e.preventDefault();
      app.settingsOpen = !app.settingsOpen;
    }
  }
</script>

<svelte:window onkeydown={key} />

<div class="shell" data-chrome={chrome}>
  <Sidebar />
  <main class="main">
    <Toolbar scenarios={scenarios.value ?? []} {active} />
    <div class="scroller">
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
    </div>
  </main>
</div>

{#if app.settingsOpen}
  <Settings />
{/if}
<Toasts />

<style>
  .shell {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100%;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .scroller {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    container: main / inline-size;
  }
  .content {
    max-width: var(--content-max);
    margin: 0 auto;
    padding: var(--s2) var(--gutter) 64px;
  }
  @media (max-width: 720px) {
    .shell {
      grid-template-columns: 1fr;
      grid-template-rows: auto 1fr;
    }
  }
</style>
