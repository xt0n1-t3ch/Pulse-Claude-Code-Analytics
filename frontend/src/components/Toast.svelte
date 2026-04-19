<script lang="ts">
  import { toasts } from "../lib/stores";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
</script>

<div class="toast-container">
  {#each $toasts as toast (toast.id)}
    <div
      class="toast {toast.type}"
      in:fly={{ x: 300, duration: 300 }}
      out:fly={{ x: 300, duration: 200 }}
      animate:flip={{ duration: 200 }}
    >
      <span class="toast-icon">
        {#if toast.type === "danger"}⚠{:else if toast.type === "warning"}⚡{:else if toast.type === "success"}✓{:else}ℹ{/if}
      </span>
      <span class="toast-msg">{toast.message}</span>
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    bottom: 20px;
    right: 20px;
    display: flex;
    flex-direction: column-reverse;
    gap: 8px;
    z-index: 9999;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 18px;
    border-radius: var(--radius-md);
    background: var(--bg-card);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-lg);
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
    pointer-events: auto;
    min-width: 260px;
    max-width: 380px;
    backdrop-filter: blur(12px);
  }

  .toast.danger {
    border-color: rgba(239, 83, 80, 0.3);
    background: linear-gradient(135deg, var(--bg-card), rgba(239, 83, 80, 0.08));
  }

  .toast.warning {
    border-color: rgba(255, 183, 77, 0.3);
    background: linear-gradient(135deg, var(--bg-card), rgba(255, 183, 77, 0.08));
  }

  .toast.success {
    border-color: rgba(76, 175, 80, 0.3);
    background: linear-gradient(135deg, var(--bg-card), rgba(76, 175, 80, 0.08));
  }

  .toast-icon {
    font-size: 16px;
    flex-shrink: 0;
  }

  .toast.danger .toast-icon { color: var(--danger); }
  .toast.warning .toast-icon { color: var(--warning); }
  .toast.success .toast-icon { color: var(--success); }

  .toast-msg {
    flex: 1;
  }
</style>
