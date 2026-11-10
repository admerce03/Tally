<script lang="ts">
  import type { Snippet } from "svelte";

  // Place inside an `.anchor` element next to its trigger. The panel itself is moved to <body> and
  // positioned against the anchor with `position: fixed`, so no container can clip or cover it.
  // It opens below (or above with `up`), aligned to the anchor's start (or end with `end`), flips
  // when there is no room, stays inside the window, and closes on Esc or a press outside.
  let {
    onclose,
    up = false,
    end = false,
    children,
  }: { onclose: () => void; up?: boolean; end?: boolean; children: Snippet } = $props();

  const GAP = 6;
  const MARGIN = 8;

  let node: HTMLElement | undefined = $state();
  let anchor: HTMLElement | null = null;
  let panel: HTMLElement | null = null;
  let pos = $state<{ top: number; left: number } | null>(null);

  function place() {
    if (!panel || !anchor) return;
    const a = anchor.getBoundingClientRect();
    const w = panel.offsetWidth;
    const h = panel.offsetHeight;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const below = a.bottom + GAP;
    const above = a.top - GAP - h;
    let top = up ? above : below;
    if (!up && below + h > vh - MARGIN && above >= MARGIN) top = above;
    if (up && above < MARGIN && below + h <= vh - MARGIN) top = below;
    let left = end ? a.right - w : a.left;
    left = Math.min(Math.max(left, MARGIN), vw - w - MARGIN);
    top = Math.min(Math.max(top, MARGIN), vh - h - MARGIN);
    pos = { top, left };
  }

  /** Move the panel to <body>, remembering the anchor it belongs to. */
  function portal(el: HTMLElement) {
    anchor = el.parentElement;
    panel = el;
    document.body.appendChild(el);
    place();
    // Again once layout has settled (fonts, late content).
    requestAnimationFrame(place);
    return {
      destroy() {
        el.remove();
      },
    };
  }

  function down(e: PointerEvent) {
    const t = e.target;
    if (!(t instanceof Node)) return;
    if (node?.contains(t) || anchor?.contains(t)) return;
    onclose();
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }

  $effect(() => {
    const first = node?.querySelector<HTMLElement>("input, select, button:not(:disabled)");
    first?.focus({ preventScroll: true });
    // Select a prefilled text field so typing replaces it (after bind:value has filled it).
    if (first instanceof HTMLInputElement) setTimeout(() => first.select(), 0);
  });
</script>

<svelte:window onpointerdown={down} onkeydown={key} onresize={place} onscrollcapture={place} />

<div
  class="popover"
  role="presentation"
  use:portal
  bind:this={node}
  style:top="{pos?.top ?? 0}px"
  style:left="{pos?.left ?? 0}px"
  style:opacity={pos ? null : 0}
>
  {@render children()}
</div>
