<script lang="ts">
  // LIF-240 — hero chart: issues created vs closed per week. Hand-rolled
  // SVG (no chart library) using a Catmull-Rom smoothing spline (see
  // ./curve.ts). Responsive via viewBox; a plain positioned-div overlay
  // handles hover (per-column invisible strips + a small floating card),
  // deliberately not the shared Tooltip.svelte — that component's
  // shrink-to-fit trigger doesn't suit N equal-width hover columns.

  import type { WeekPoint } from "../api";
  import { smoothPath, smoothAreaPath, niceTicks, type Point } from "./curve";

  let { created, closed }: { created: WeekPoint[]; closed: WeekPoint[] } = $props();

  const VB_W = 680;
  const VB_H = 220;
  const PAD_L = 28;
  const PAD_R = 8;
  const PAD_T = 14;
  const PAD_B = 22;
  const plotW = VB_W - PAD_L - PAD_R;
  const plotH = VB_H - PAD_T - PAD_B;

  let n = $derived(created.length);

  let domainMax = $derived(
    Math.max(0, ...created.map((p) => p.count), ...closed.map((p) => p.count)),
  );
  let ticks = $derived(niceTicks(domainMax));
  let axisMax = $derived(ticks[ticks.length - 1] || 1);

  function xAt(i: number): number {
    return n <= 1 ? PAD_L + plotW / 2 : PAD_L + (plotW * i) / (n - 1);
  }
  function yAt(v: number): number {
    return PAD_T + plotH - (v / axisMax) * plotH;
  }

  let createdPts = $derived<Point[]>(created.map((p, i) => ({ x: xAt(i), y: yAt(p.count) })));
  let closedPts = $derived<Point[]>(closed.map((p, i) => ({ x: xAt(i), y: yAt(p.count) })));

  let createdLine = $derived(smoothPath(createdPts));
  let closedLine = $derived(smoothPath(closedPts));
  let createdArea = $derived(smoothAreaPath(createdPts, PAD_T + plotH));
  let closedArea = $derived(smoothAreaPath(closedPts, PAD_T + plotH));

  function weekLabel(iso: string): string {
    if (!iso) return "";
    const d = new Date(`${iso}T00:00:00Z`);
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric", timeZone: "UTC" });
  }

  // First / (up to 3 interior) / last, so the axis stays legible from a
  // 4-week window up to a 52-week one.
  let xLabelIndices = $derived.by(() => {
    if (n <= 1) return [0];
    if (n <= 6) return created.map((_, i) => i);
    const mid1 = Math.round((n - 1) / 3);
    const mid2 = Math.round(((n - 1) * 2) / 3);
    return [...new Set([0, mid1, mid2, n - 1])];
  });

  let hoverIndex = $state<number | null>(null);

  let hasAnyData = $derived(domainMax > 0);
</script>

<div class="relative select-none">
  <svg
    viewBox="0 0 {VB_W} {VB_H}"
    class="w-full h-auto block"
    role="img"
    aria-label="Issues created vs closed per week"
  >
    <!-- gridlines + y labels -->
    {#each ticks as t (t)}
      <line
        x1={PAD_L}
        x2={VB_W - PAD_R}
        y1={yAt(t)}
        y2={yAt(t)}
        stroke="var(--border)"
        stroke-width="1"
        stroke-dasharray={t === 0 ? undefined : "2 3"}
      />
      <text x={PAD_L - 6} y={yAt(t) + 3} text-anchor="end" class="fill-[var(--text-faint)]" font-size="9">
        {t}
      </text>
    {/each}

    <!-- x labels -->
    {#each xLabelIndices as i (i)}
      <text
        x={xAt(i)}
        y={VB_H - 6}
        text-anchor="middle"
        class="fill-[var(--text-faint)]"
        font-size="9"
      >
        {weekLabel(created[i]?.week_start ?? "")}
      </text>
    {/each}

    {#if hasAnyData}
      <!-- areas -->
      <path d={createdArea} fill="var(--accent)" opacity="0.10" />
      <path d={closedArea} fill="var(--success)" opacity="0.10" />

      <!-- lines -->
      <path d={createdLine} fill="none" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
      <path d={closedLine} fill="none" stroke="var(--success)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />

      {#if hoverIndex !== null}
        <line
          x1={xAt(hoverIndex)}
          x2={xAt(hoverIndex)}
          y1={PAD_T}
          y2={PAD_T + plotH}
          stroke="var(--text-faint)"
          stroke-width="1"
          stroke-dasharray="2 3"
        />
      {/if}

      <!-- dots -->
      {#each createdPts as p, i (`c${i}`)}
        <circle cx={p.x} cy={p.y} r={hoverIndex === i ? 3.5 : 2} fill="var(--accent)" />
      {/each}
      {#each closedPts as p, i (`d${i}`)}
        <circle cx={p.x} cy={p.y} r={hoverIndex === i ? 3.5 : 2} fill="var(--success)" />
      {/each}
    {/if}
  </svg>

  {#if hasAnyData}
    <!-- hover overlay: one invisible column per week -->
    <div
      class="absolute inset-0 flex"
      style="left: {(PAD_L / VB_W) * 100}%; right: {(PAD_R / VB_W) * 100}%;"
    >
      {#each created as pt, i (pt.week_start)}
        <div
          class="flex-1 h-full cursor-default"
          role="presentation"
          onmouseenter={() => (hoverIndex = i)}
          onmouseleave={() => (hoverIndex = null)}
        ></div>
      {/each}
    </div>

    {#if hoverIndex !== null}
      {@const pt = created[hoverIndex]}
      {@const cpt = closed[hoverIndex]}
      <div
        class="absolute z-10 pointer-events-none px-2.5 py-1.5 rounded-md
               bg-[var(--surface)] border border-[var(--border)]
               shadow-[0_4px_12px_rgba(0,0,0,0.18)] whitespace-nowrap
               -translate-x-1/2"
        style="left: {Math.min(92, Math.max(8, (xAt(hoverIndex) / VB_W) * 100))}%; top: 2px;"
      >
        <p class="text-caption font-medium text-[var(--text)] mb-0.5">{weekLabel(pt.week_start)}</p>
        <p class="text-micro text-[var(--accent)] m-0">
          Created <span class="tabular-nums font-semibold">{pt.count}</span>
        </p>
        <p class="text-micro text-[var(--success)] m-0">
          Closed <span class="tabular-nums font-semibold">{cpt?.count ?? 0}</span>
        </p>
      </div>
    {/if}
  {/if}
</div>

<!-- legend -->
<div class="flex items-center gap-4 mt-1 px-1">
  <span class="flex items-center gap-1.5 text-caption text-[var(--text-muted)]">
    <span class="size-2 rounded-full shrink-0" style="background: var(--accent)"></span>
    Created
  </span>
  <span class="flex items-center gap-1.5 text-caption text-[var(--text-muted)]">
    <span class="size-2 rounded-full shrink-0" style="background: var(--success)"></span>
    Closed
  </span>
</div>
