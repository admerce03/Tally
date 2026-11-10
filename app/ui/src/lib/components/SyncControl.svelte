<script lang="ts">
  import { runSync, sync } from "../sync.svelte";

  // Sidebar status: a dot and one word, only while sync is on (set up in Settings). Click to sync now.
  const view = $derived.by(() => {
    const s = sync.status;
    if (!s) return { word: "", tone: "off", tip: "" };
    if (sync.busy || s.running) return { word: "Syncing", tone: "busy", tip: "Syncing" };
    if (s.last?.error) return { word: "Failed", tone: "fail", tip: `${s.last.error}. Click to retry.` };
    return { word: "Synced", tone: "ok", tip: "Click to sync now" };
  });
</script>

{#if sync.status?.configured}
  <button
    type="button"
    class="line t-callout muted"
    title={view.tip}
    disabled={sync.busy || sync.status.running}
    onclick={runSync}
  >
    <span class="dot {view.tone}" aria-hidden="true"></span>
    <span>{view.word}</span>
  </button>
{/if}

<style>
  .line {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: 0;
    padding: 4px 6px;
    border-radius: var(--r-control);
    cursor: pointer;
  }
  .line:hover:not(:disabled) {
    background: var(--bg-hover);
  }
  .line:disabled {
    cursor: default;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--label-3);
  }
  .dot.ok {
    background: var(--label-2);
  }
  .dot.busy {
    background: var(--accent);
  }
  .dot.fail {
    background: var(--red);
  }
</style>
