<script lang="ts">
  import { onDestroy } from "svelte";

  let {
    label,
    confirmLabel = "Confirm?",
    onconfirm,
    title,
  }: { label: string; confirmLabel?: string; onconfirm: () => void; title?: string } = $props();

  let armed = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function click() {
    if (armed) {
      clearTimeout(timer);
      armed = false;
      onconfirm();
      return;
    }
    armed = true;
    timer = setTimeout(() => (armed = false), 3000);
  }

  onDestroy(() => clearTimeout(timer));
</script>

<button
  type="button"
  class="btn sm ghost danger"
  class:armed
  {title}
  onclick={click}
  onblur={() => (armed = false)}
>
  {armed ? confirmLabel : label}
</button>
