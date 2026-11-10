<script lang="ts">
  import { fmtDate, pct } from "../format";
  import type { ImpactEntry } from "../generated/output";

  // Running grade after each graded assignment (core `impact`), one point per assignment in
  // chronological order. In a what-if, `real` is drawn as a quiet line under the what-if one.
  let {
    entries,
    real = null,
    target = null,
  }: { entries: ImpactEntry[]; real?: ImpactEntry[] | null; target?: number | null } = $props();

  type Slot = { id: string; name: string; due: string | null; now: number | null; was: number | null };

  // Layout only: merge both series into one ordered list of x slots (what-if order, with
  // real-only items placed after the real item before them).
  const slots = $derived.by((): Slot[] => {
    const out: Slot[] = entries.map((e) => ({
      id: e.assignment_id,
      name: e.name,
      due: e.due ?? null,
      now: e.grade_after ?? null,
      was: null,
    }));
    if (real) {
      let last = -1;
      for (const e of real) {
        const at = out.findIndex((s) => s.id === e.assignment_id);
        if (at >= 0) {
          out[at].was = e.grade_after ?? null;
          last = at;
        } else {
          out.splice(++last, 0, { id: e.assignment_id, name: e.name, due: e.due ?? null, now: null, was: e.grade_after ?? null });
        }
      }
    }
    return out;
  });

  let width = $state(640);
  const H = 220;
  const M = { l: 40, t: 16, b: 28 };
  // Right gutter: room for the target's label beside its line, outside the plot.
  const R = $derived(target != null ? 84 : 16);
  const plotW = $derived(Math.max(width - M.l - R, 40));
  const plotH = H - M.t - M.b;

  // Axis range: padded around the data and the target, in steps of 10 (5 when tight).
  const range = $derived.by(() => {
    const vals = slots.flatMap((s) => [s.now, s.was]).filter((v): v is number => v != null);
    if (target != null) vals.push(target);
    if (vals.length === 0) return { lo: 0, hi: 100, step: 20 };
    const lo = Math.max(0, Math.floor((Math.min(...vals) - 4) / 10) * 10);
    const hi = Math.max(lo + 20, Math.ceil((Math.max(...vals) + 2) / 10) * 10);
    const span = hi - lo;
    return { lo, hi, step: span <= 20 ? 5 : span <= 60 ? 10 : 20 };
  });
  const ticks = $derived.by(() => {
    const out: number[] = [];
    for (let v = range.lo; v <= range.hi + 1e-9; v += range.step) out.push(v);
    return out;
  });

  const PAD = 12;
  const px = (i: number) =>
    slots.length <= 1 ? M.l + plotW / 2 : M.l + PAD + (i * (plotW - 2 * PAD)) / (slots.length - 1);
  const py = (g: number) => M.t + ((range.hi - g) * plotH) / (range.hi - range.lo);

  function path(key: "now" | "was"): string {
    let d = "";
    let pen = false;
    slots.forEach((s, i) => {
      const v = s[key];
      if (v == null) return;
      d += `${pen ? "L" : "M"}${px(i).toFixed(1)},${py(v).toFixed(1)}`;
      pen = true;
    });
    return d;
  }

  // Thin the date labels so they never collide (~72px each).
  // Dates label the x axis only when every point has one; otherwise the axis carries ticks alone
  // and the hover readout names each point (consistent, never half-labelled).
  const allDated = $derived(slots.length > 0 && slots.every((s) => s.due));
  const every = $derived(Math.max(1, Math.ceil(slots.length / Math.max(1, Math.floor(plotW / 72)))));

  let hover = $state<number | null>(null);
  function move(e: PointerEvent) {
    if (slots.length === 0) return;
    const rect = (e.currentTarget as SVGElement).getBoundingClientRect();
    const x = e.clientX - rect.left;
    let best = 0;
    for (let i = 1; i < slots.length; i++) if (Math.abs(px(i) - x) < Math.abs(px(best) - x)) best = i;
    hover = best;
  }

  const whatIf = $derived(real != null);
</script>

<figure class="chart" bind:clientWidth={width}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <svg
    {width}
    height={H}
    role="img"
    aria-label="Grade after each graded assignment"
    onpointermove={move}
    onpointerleave={() => (hover = null)}
  >
    {#each ticks as t (t)}
      <line class="grid" x1={M.l} x2={width - R} y1={py(t)} y2={py(t)} />
      <text class="axis" x={M.l - 8} y={py(t) + 4} text-anchor="end">{t}%</text>
    {/each}

    {#if target != null}
      <line class="target" x1={M.l} x2={width - R} y1={py(target)} y2={py(target)} />
      <text class="target-label" x={width - R + 6} y={py(target) + 4} text-anchor="start">Target {pct(target)}</text>
    {/if}

    {#each slots as s, i (s.id)}
      <line class="tick" x1={px(i)} x2={px(i)} y1={H - M.b} y2={H - M.b + 4} />
      {#if allDated && i % every === 0}
        <text class="axis" x={px(i)} y={H - 8} text-anchor="middle">{fmtDate(s.due)}</text>
      {/if}
    {/each}

    {#if hover !== null}
      <line class="cursor" x1={px(hover)} x2={px(hover)} y1={M.t} y2={H - M.b} />
    {/if}

    {#if whatIf}
      <path class="line was" d={path("was")} />
    {/if}
    <path class="line now" class:dashed={whatIf} d={path("now")} />

    {#each slots as s, i (s.id)}
      {#if whatIf && s.was != null}
        <circle class="dot was" cx={px(i)} cy={py(s.was)} r="2.5" />
      {/if}
      {#if s.now != null}
        <circle class="dot now" class:active={hover === i} cx={px(i)} cy={py(s.now)} r={hover === i ? 4.5 : 3} />
      {/if}
    {/each}

    {#if slots.length === 0}
      <text class="hint" x={M.l + plotW / 2} y={M.t + plotH / 2} text-anchor="middle">No graded work yet</text>
    {/if}
  </svg>

  {#if hover !== null}
    {@const s = slots[hover]}
    <div class="tip" style:left="{Math.min(Math.max(px(hover), 90), width - 90)}px" style:top="{py(s.now ?? s.was ?? range.lo) - 10}px">
      <div class="t-headline ellipsis">{s.name}</div>
      <div class="t-callout muted num">
        {#if s.due}{fmtDate(s.due)} · {/if}
        {#if whatIf && s.was != null && s.now != null && pct(s.was) !== pct(s.now)}
          {pct(s.was)} → <span class="accent">{pct(s.now)}</span>
        {:else}
          {pct(s.now ?? s.was)}
        {/if}
      </div>
    </div>
  {/if}
</figure>

<style>
  .chart {
    position: relative;
    margin: 0;
    min-width: 0;
  }
  svg {
    display: block;
    touch-action: none;
  }
  .grid {
    stroke: var(--separator);
  }
  .axis {
    fill: var(--label-2);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .target {
    stroke: var(--label-3);
    stroke-dasharray: 4 4;
  }
  .tick {
    stroke: var(--label-3);
  }
  .target-label {
    fill: var(--label-2);
    font-size: 11px;
    font-weight: 600;
    /* A halo in the background color keeps the label readable over the line. */
    paint-order: stroke;
    stroke: var(--bg);
    stroke-width: 4px;
    stroke-linejoin: round;
  }
  .line {
    fill: none;
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .line.now {
    stroke: var(--accent);
  }
  .line.now.dashed {
    stroke-dasharray: 6 4;
  }
  .line.was {
    stroke: var(--label-3);
    stroke-width: 1.5;
  }
  .dot.now {
    fill: var(--bg);
    stroke: var(--accent);
    stroke-width: 2;
  }
  .dot.now.active {
    fill: var(--accent);
  }
  .dot.was {
    fill: var(--label-3);
  }
  .cursor {
    stroke: var(--label-3);
  }
  .hint {
    fill: var(--label-3);
    font-size: 13px;
  }
  .tip {
    position: absolute;
    transform: translate(-50%, -100%);
    max-width: 200px;
    padding: 6px 10px;
    background: var(--bg);
    border-radius: var(--r-control);
    box-shadow: var(--shadow);
    pointer-events: none;
    text-align: center;
  }
</style>
