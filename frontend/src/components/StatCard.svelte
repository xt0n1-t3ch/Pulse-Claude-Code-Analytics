<script lang="ts">
  import type { Snippet } from "svelte";

  let { label, value, extra }: { label: string; value: string; extra?: Snippet } = $props();
</script>

<div class="stat-card">
  <span class="stat-label">{label}</span>
  <div class="stat-row">
    <span class="stat-value">{value}</span>
    {#if extra}
      <div class="stat-extra">{@render extra()}</div>
    {/if}
  </div>
</div>

<style>
  .stat-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    transition: all 0.2s var(--ease);
  }

  .stat-card:hover {
    border-color: var(--border-hover);
    background: var(--bg-card-hover);
    box-shadow: var(--shadow-sm);
  }

  .stat-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .stat-row {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 8px;
  }

  .stat-value {
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.03em;
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }

  .stat-extra {
    display: flex;
    align-items: flex-end;
    flex-shrink: 0;
  }
</style>
