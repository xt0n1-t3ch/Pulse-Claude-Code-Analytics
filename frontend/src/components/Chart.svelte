<script lang="ts">
  import { onMount } from "svelte";
  import { Chart, type ChartConfiguration } from "chart.js/auto";

  let {
    config,
    updateData,
  }: {
    config: ChartConfiguration;
    updateData?: (chart: Chart) => void;
  } = $props();

  let canvas: HTMLCanvasElement;
  let chart: Chart | null = null;

  onMount(() => {
    Chart.defaults.color = "#9b9696";
    Chart.defaults.borderColor = "rgba(74,70,70,0.3)";
    chart = new Chart(canvas, config);
    return () => chart?.destroy();
  });

  $effect(() => {
    if (chart && updateData) {
      updateData(chart);
      chart.update("none");
    }
  });
</script>

<div class="chart-wrap">
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .chart-wrap {
    position: relative;
    width: 100%;
    height: 100%;
  }
</style>
