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
    font: 9.5px var(--font-mono); padding: 0 5px; border-radius: 0; letter-spacing: .08em;
    text-transform: uppercase; border: 1px solid currentColor; display: inline-block; line-height: 15px;
  }
  .muted { color: var(--muted); }
  .ok { color: var(--ok); }
  .warn { color: var(--warn); }
  .err { color: var(--err); }
  .info { color: var(--info); }
</style>
