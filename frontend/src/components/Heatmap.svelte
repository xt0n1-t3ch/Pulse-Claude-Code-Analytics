<script lang="ts">
  import type { HourlyActivity } from "../lib/api";

  let { data = [] }: { data: HourlyActivity[] } = $props();

  const CELL = 18;
  const HOURS = Array.from({ length: 24 }, (_, i) => i);

  let maxCount = $derived(Math.max(...data.map(d => d.session_count), 1));

  function intensity(hour: number): number {
    const entry = data.find(d => d.hour === hour);
    if (!entry) return 0;
    return entry.session_count / maxCount;
  }

  function cellColor(val: number): string {
    if (val === 0) return "var(--bg-elevated)";
    if (val < 0.25) return "color-mix(in srgb, var(--accent) 25%, transparent)";
    if (val < 0.5) return "color-mix(in srgb, var(--accent) 50%, transparent)";
    if (val < 0.75) return "color-mix(in srgb, var(--accent) 75%, transparent)";
    return "var(--accent)";
  }

  function hourLabel(h: number): string {
    if (h === 0) return "12a";
    if (h < 12) return h + "a";
    if (h === 12) return "12p";
    return (h - 12) + "p";
  }

  function sessionCount(hour: number): number {
    return data.find(d => d.hour === hour)?.session_count ?? 0;
  }
</script>

<div class="heatmap">
  <div class="heatmap-grid">
    {#each HOURS as h}
      <div
        class="heatmap-cell"
        style="background:{cellColor(intensity(h))};height:{CELL}px"
        title="{hourLabel(h)}: {sessionCount(h)} sessions"
      ></div>
    {/each}
  </div>
  <div class="heatmap-labels">
    {#each [0, 6, 12, 18] as h}
      <span class="heatmap-label">{hourLabel(h)}</span>
    {/each}
  </div>
  <div class="heatmap-legend">
    <span class="legend-text">Less</span>
    {#each [0, 0.25, 0.5, 0.75, 1] as v}
      <div class="legend-cell" style="background:{cellColor(v)}"></div>
    {/each}
    <span class="legend-text">More</span>
  </div>
</div>

<style>
  .heatmap { display: flex; flex-direction: column; gap: 6px; max-width: 100%; min-width: 0; }
  .heatmap-grid { display: grid; grid-template-columns: repeat(24, minmax(4px, 1fr)); gap: 2px; }
  .heatmap-cell { border-radius: 3px; transition: background 0.2s ease; cursor: default; }
  .heatmap-labels { display: flex; justify-content: space-between; height: 14px; }
  .heatmap-label { font-size: 9px; color: var(--text-muted); font-weight: 500; }
  .heatmap-legend { display: flex; align-items: center; gap: 3px; margin-top: 4px; }
  .legend-text { font-size: 9px; color: var(--text-muted); }
  .legend-cell { width: 10px; height: 10px; border-radius: 2px; }
</style>
