<script lang="ts">
  let { keys, label = '' }: { keys: string; label?: string } = $props();
</script>

<span class="hint"><kbd>{keys}</kbd>{#if label}<span class="label">{label}</span>{/if}</span>

<style>
  .hint { white-space: nowrap; }
  .label { margin-left: 5px; }
  kbd {
    font: 11px var(--font-mono); color: var(--muted); padding: 1px 5px; border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong); background: var(--surface);
  }
</style>
