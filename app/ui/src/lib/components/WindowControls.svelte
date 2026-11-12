<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  // Minimize / maximize / close for the frameless window on Windows (rendered only there).
  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const sync = () => win.isMaximized().then((m) => (maximized = m));
    sync();
    let off: (() => void) | undefined;
    let gone = false;
    win.onResized(sync).then((u) => (gone ? u() : (off = u)));
    return () => {
      gone = true;
      off?.();
    };
  });
</script>

<div class="controls">
  <button type="button" tabindex="-1" aria-label="Minimize" onclick={() => win.minimize()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
  </button>
  <button
    type="button"
    tabindex="-1"
    aria-label={maximized ? "Restore" : "Maximize"}
    onclick={() => win.toggleMaximize()}
  >
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      {#if maximized}
        <path d="M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z" />
      {:else}
        <path d="M.5.5h9v9h-9z" />
      {/if}
    </svg>
  </button>
  <button type="button" class="close" tabindex="-1" aria-label="Close" onclick={() => win.close()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5l9 9M9.5.5l-9 9" /></svg>
  </button>
</div>

<style>
  .controls {
    display: flex;
    align-self: stretch;
    margin-left: var(--s2);
  }
  button {
    width: 46px;
    display: grid;
    place-items: center;
    background: none;
    border: 0;
    color: var(--label);
    cursor: default;
  }
  button:hover {
    background: var(--bg-hover);
  }
  button:active {
    background: var(--separator);
  }
  .close:hover {
    background: var(--win-close);
    color: var(--on-accent);
  }
  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
</style>
