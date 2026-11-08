<script lang="ts">
  let { keys, label = '' }: { keys: string; label?: string } = $props();
</script>

<span class="hint"><kbd>{keys}</kbd>{#if label}<span class="label">{label}</span>{/if}</span>

<style>
  .hint { white-space: nowrap; }
  .label { margin-left: 4px; }
  kbd {
    font: 9.5px var(--font-mono); color: var(--muted); padding: 0 4px; border-radius: 0;
    border: 1px solid var(--border-strong); background: var(--surface);
  }
</style>
