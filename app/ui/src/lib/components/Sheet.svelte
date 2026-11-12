<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    children,
  }: { title: string; onclose: () => void; children: Snippet } = $props();

  let panel: HTMLElement | undefined = $state();

  $effect(() => {
    panel?.querySelector<HTMLElement>("input:not([disabled]), select:not([disabled])")?.focus();
  });

  function key(e: KeyboardEvent) {
    // An alert or an open popover above the sheet handles Esc itself.
    if (e.key === "Escape" && !e.defaultPrevented && !document.querySelector("[data-alert], .popover")) {
      e.preventDefault();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" onpointerdown={onclose} role="presentation"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-label={title} bind:this={panel}>
  <header class="sheet-head">
    <h2 class="t-title">{title}</h2>
    <button type="button" class="btn prominent" onclick={onclose}>Done</button>
  </header>
  <div class="sheet-body">{@render children()}</div>
</div>
