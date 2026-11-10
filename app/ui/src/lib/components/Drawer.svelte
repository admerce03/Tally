<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    children,
  }: { title: string; onclose: () => void; children: Snippet } = $props();

  let panel: HTMLElement | undefined = $state();

  $effect(() => {
    panel?.querySelector<HTMLElement>("input, select, button")?.focus();
  });

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={key} />

<div class="backdrop" onpointerdown={onclose} role="presentation"></div>
<div class="drawer" role="dialog" aria-modal="true" aria-label={title} bind:this={panel}>
  <header class="spread">
    <h3>{title}</h3>
    <button type="button" class="btn ghost sm" onclick={onclose} aria-label="Close">Close (Esc)</button>
  </header>
  <div class="stack">{@render children()}</div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    z-index: 20;
  }
  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(440px, 100vw);
    background: var(--bg);
    border-left: 1px solid var(--border);
    padding: 20px;
    overflow-y: auto;
    z-index: 21;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
</style>
