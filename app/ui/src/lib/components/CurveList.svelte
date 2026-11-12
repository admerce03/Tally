<script lang="ts">
  import { CURVE_LABEL, type CurveRow } from "../format";
  import Icon from "./Icon.svelte";

  let { rows = $bindable() }: { rows: CurveRow[] } = $props();
  const kinds = Object.keys(CURVE_LABEL) as CurveRow["kind"][];
</script>

<div class="stack tight">
  {#each rows as row, i (i)}
    <div class="cluster">
      <select bind:value={row.kind} aria-label="Curve type">
        {#each kinds as k (k)}
          <option value={k}>{CURVE_LABEL[k]}</option>
        {/each}
      </select>
      <input class="narrow right" type="text" inputmode="decimal" bind:value={row.value} aria-label="Curve value" />
      <button type="button" class="btn quiet icon" aria-label="Remove curve" onclick={() => rows.splice(i, 1)}>
        <Icon name="close" />
      </button>
    </div>
  {/each}
  <div>
    <button type="button" class="btn borderless" onclick={() => rows.push({ kind: "add_points", value: "0" })}>
      Add curve
    </button>
  </div>
</div>
