<script lang="ts">
  import { app, dismissToast } from "../app.svelte";
  import Icon from "./Icon.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast" role="status">
      {#if t.kind === "error"}<span class="dot" aria-hidden="true"></span>{/if}
      <span class="grow">{t.text}</span>
      <button type="button" class="btn quiet icon" onclick={() => dismissToast(t.id)} aria-label="Dismiss">
        <Icon name="close" size={14} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: var(--s4);
    bottom: var(--s4);
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    z-index: 70;
    max-width: min(420px, calc(100vw - 32px));
  }
  .toast {
    display: flex;
    align-items: center;
    gap: var(--s2);
    background: var(--bg);
    border-radius: var(--r-group);
    padding: var(--s2) var(--s2) var(--s2) var(--s3);
    box-shadow: var(--shadow);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--red);
    flex: none;
  }
</style>
