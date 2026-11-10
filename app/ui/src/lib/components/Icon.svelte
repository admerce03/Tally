<script lang="ts">
  let { name, size = 16 }: { name: "gear" | "plus" | "more" | "chevron" | "close"; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.3"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === "gear"}
    <!-- Eight square teeth around a hub: reads as a gear, never as a sun. -->
    <path
      d="M6.63 2.88L6.89 0.99L9.11 0.99L9.37 2.88L10.65 3.41L12.17 2.26L13.74 3.83L12.59 5.35L13.12 6.63L15.01 6.89L15.01 9.11L13.12 9.37L12.59 10.65L13.74 12.17L12.17 13.74L10.65 12.59L9.37 13.12L9.11 15.01L6.89 15.01L6.63 13.12L5.35 12.59L3.83 13.74L2.26 12.17L3.41 10.65L2.88 9.37L0.99 9.11L0.99 6.89L2.88 6.63L3.41 5.35L2.26 3.83L3.83 2.26L5.35 3.41Z"
    />
    <circle cx="8" cy="8" r="2.2" />
  {:else if name === "plus"}
    <path d="M8 3v10M3 8h10" stroke-width="1.5" />
  {:else if name === "more"}
    <circle cx="3.5" cy="8" r="0.6" fill="currentColor" />
    <circle cx="8" cy="8" r="0.6" fill="currentColor" />
    <circle cx="12.5" cy="8" r="0.6" fill="currentColor" />
  {:else if name === "chevron"}
    <path d="M6 3.5 10.5 8 6 12.5" stroke-width="1.5" />
  {:else}
    <path d="M4 4l8 8M12 4l-8 8" stroke-width="1.5" />
  {/if}
</svg>
