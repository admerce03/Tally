<script module lang="ts">
  export type MenuItem = {
    label: string;
    onselect: () => void;
    destructive?: boolean;
    disabled?: boolean;
    /** Draw a hairline above this item (starts a new section). */
    separator?: boolean;
    /** Show a check mark (the current choice). */
    checked?: boolean;
  };
</script>

<script lang="ts">
  import Popover from "./Popover.svelte";

  let {
    items,
    onclose,
    up = false,
    end = false,
  }: { items: MenuItem[]; onclose: () => void; up?: boolean; end?: boolean } = $props();

  const checks = $derived(items.some((i) => i.checked !== undefined));
</script>

<Popover {onclose} {up} {end}>
  <div class="menu" role="menu">
    {#each items as item, i (i)}
      {#if item.separator}<div class="menu-sep" role="separator"></div>{/if}
      <button
        type="button"
        role={checks ? "menuitemradio" : "menuitem"}
        aria-checked={checks ? !!item.checked : undefined}
        class="menu-item"
        class:destructive={item.destructive}
        disabled={item.disabled}
        onclick={() => {
          onclose();
          item.onselect();
        }}
      >
        {#if checks}<span class="check" aria-hidden="true">{item.checked ? "✓" : ""}</span>{/if}
        <span class="ellipsis">{item.label}</span>
      </button>
    {/each}
  </div>
</Popover>

<style>
  :global(.popover:has(> .menu)) {
    padding: 0;
  }
  .menu-sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--separator);
  }
  .menu-item {
    display: flex;
    gap: 6px;
    max-width: 320px;
  }
  .check {
    width: 14px;
    flex: none;
  }
</style>
