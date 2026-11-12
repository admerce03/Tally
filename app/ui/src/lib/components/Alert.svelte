<script lang="ts">
  // Confirmation for deleting things that cannot be undone.
  let {
    title,
    message = "",
    confirmLabel = "Delete",
    onconfirm,
    oncancel,
  }: { title: string; message?: string; confirmLabel?: string; onconfirm: () => void; oncancel: () => void } =
    $props();

  let box: HTMLElement | undefined = $state();

  $effect(() => {
    box?.querySelector<HTMLElement>("button")?.focus();
  });

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      oncancel();
    }
  }
</script>

<svelte:window onkeydown={key} />

<div class="scrim" style="z-index: 60" onpointerdown={oncancel} role="presentation"></div>
<div class="alert" role="alertdialog" aria-modal="true" aria-label={title} data-alert bind:this={box}>
  <div class="stack tight">
    <h2 class="t-headline">{title}</h2>
    {#if message}<p class="t-callout muted">{message}</p>{/if}
  </div>
  <div class="alert-actions">
    <button type="button" class="btn" onclick={oncancel}>Cancel</button>
    <button type="button" class="btn destructive" onclick={onconfirm}>{confirmLabel}</button>
  </div>
</div>
