<script lang="ts">
  let { config, updateData }: { config?: unknown; updateData?: unknown } = $props();
</script>

<div class="chart-stub" data-config={config ? "set" : "empty"} data-update={updateData ? "set" : "empty"}></div>
