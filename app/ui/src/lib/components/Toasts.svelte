<script lang="ts">
  import { app, dismissToast } from "../app.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast" class:err={t.kind === "error"} role="status">
      <span>{t.text}</span>
      <button type="button" class="btn ghost sm" onclick={() => dismissToast(t.id)} aria-label="Dismiss">x</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 40;
    max-width: min(420px, calc(100vw - 32px));
  }
  .toast {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  }
  .toast.err {
    border-color: var(--neg);
  }
</style>
