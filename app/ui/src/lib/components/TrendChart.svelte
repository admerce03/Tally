<script lang="ts">
  import { pct } from "../format";
  import type { TrendPoint } from "../generated/output";

  let { points }: { points: TrendPoint[] } = $props();

  // Layout geometry only (viewBox units). The SVG scales to its container width.
  const W = 640;
  const H = 280;
  const M = { l: 48, r: 16, t: 16, b: 40 };

  // Axis range: padded around the data, rounded to steps of 5. Presentation only.
  const range = $derived.by(() => {
    const vals = points.map((p) => p.grade);
    const lo = Math.max(0, Math.floor((Math.min(...vals) - 5) / 5) * 5);
    const hi = Math.max(lo + 10, Math.ceil((Math.max(...vals) + 5) / 5) * 5);
    return { lo, hi };
  });

  const ticks = $derived.by(() => {
    const out: number[] = [];
    const step = (range.hi - range.lo) / 4;
    for (let i = 0; i <= 4; i++) out.push(range.lo + step * i);
    return out;
  });

  const px = (i: number) =>
    points.length === 1 ? M.l + (W - M.l - M.r) / 2 : M.l + (i * (W - M.l - M.r)) / (points.length - 1);
  const py = (g: number) => M.t + ((range.hi - g) * (H - M.t - M.b)) / (range.hi - range.lo);

  const path = $derived(points.map((p, i) => `${i === 0 ? "M" : "L"}${px(i)},${py(p.grade)}`).join(" "));

  let hover = $state<number | null>(null);
  let svg: SVGSVGElement | undefined = $state();

  function move(e: PointerEvent) {
    if (!svg || points.length === 0) return;
    const rect = svg.getBoundingClientRect();
    const x = ((e.clientX - rect.left) / rect.width) * W;
    let best = 0;
    for (let i = 1; i < points.length; i++) {
      if (Math.abs(px(i) - x) < Math.abs(px(best) - x)) best = i;
    }
    hover = best;
  }

  function label(p: TrendPoint): string {
    return p.date ?? "Undated";
  }

  // Keep the tooltip inside the plot.
  const tipX = $derived(hover === null ? 0 : Math.min(Math.max(px(hover), M.l + 60), W - M.r - 60));
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<svg
  bind:this={svg}
  viewBox="0 0 {W} {H}"
  role="img"
  aria-label="Course grade over time"
  onpointermove={move}
  onpointerleave={() => (hover = null)}
>
  {#each ticks as t (t)}
    <line class="grid" x1={M.l} x2={W - M.r} y1={py(t)} y2={py(t)} />
    <text class="axis" x={M.l - 8} y={py(t) + 4} text-anchor="end">{t.toFixed(0)}%</text>
  {/each}
  <path class="line" d={path} />
  {#each points as p, i (i)}
    <circle class="dot" class:active={hover === i} cx={px(i)} cy={py(p.grade)} r={hover === i ? 5 : 3.5} />
    {#if points.length <= 8 || i % Math.ceil(points.length / 8) === 0}
      <text class="axis" x={px(i)} y={H - M.b + 18} text-anchor="middle">{label(p)}</text>
    {/if}
  {/each}
  <text class="axis" x={M.l - 8} y={M.t - 4} text-anchor="end">Grade</text>
  {#if hover !== null}
    {@const p = points[hover]}
    <line class="cursor" x1={px(hover)} x2={px(hover)} y1={M.t} y2={H - M.b} />
    <g transform="translate({tipX},{Math.max(py(p.grade) - 44, 2)})">
      <rect class="tip" x="-60" width="120" height="36" rx="6" />
      <text class="tip-text" y="15" text-anchor="middle">{label(p)}</text>
      <text class="tip-text strong" y="30" text-anchor="middle">{pct(p.grade)}</text>
    </g>
  {/if}
</svg>

<style>
  svg {
    width: 100%;
    height: auto;
    display: block;
    touch-action: none;
  }
  .grid {
    stroke: var(--border);
    stroke-width: 1;
  }
  .axis {
    fill: var(--fg-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2.5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .dot {
    fill: var(--bg);
    stroke: var(--accent);
    stroke-width: 2;
  }
  .dot.active {
    fill: var(--accent);
  }
  .cursor {
    stroke: var(--border);
    stroke-dasharray: 3 3;
  }
  .tip {
    fill: var(--bg-card);
    stroke: var(--border);
  }
  .tip-text {
    fill: var(--fg-muted);
    font-size: 11px;
  }
  .tip-text.strong {
    fill: var(--fg);
    font-size: 13px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
</style>
