<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    tone = 'muted',
    children,
  }: { tone?: 'muted' | 'ok' | 'warn' | 'err' | 'info'; children: Snippet } = $props();
</script>

<span class="badge {tone}">{@render children()}</span>

<style>
  .badge {
    font: 11px var(--font-mono); padding: 1px 7px; border-radius: var(--radius-sm);
    border: 1px solid currentColor; display: inline-block; line-height: 16px;
  }
  .muted { color: var(--muted); }
  .ok { color: var(--ok); }
  .warn { color: var(--warn); }
  .err { color: var(--err); }
  .info { color: var(--info); }
</style>
