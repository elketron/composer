<!--
  The frame every composer app shares: top bar (site / page, session, actions),
  content, and a footer with keyboard hints.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { isConnected, session, site } from '../client.svelte';

  let {
    crumb = '',
    actions,
    footer,
    status = '',
    children,
  }: {
    /** Shown after the site name, e.g. `tickets` or `mockups / login`. */
    crumb?: string;
    /** Right side of the top bar (buttons). */
    actions?: Snippet;
    /** Left side of the footer (keyboard hints). */
    footer?: Snippet;
    /** Right side of the footer (e.g. the URL of what is selected). */
    status?: string;
    children: Snippet;
  } = $props();
</script>

<div class="shell">
  <header class="bar">
    <span class="brand">composer</span>
    <span class="site">{site}</span>
    {#if crumb}<span class="crumb">/ <b>{crumb}</b></span>{/if}
    <div class="right">
      <span class="chip" class:off={!isConnected()} title={isConnected() ? 'connected' : 'reconnecting'}>
        <span class="dot"></span>
        {session ? `#${session}` : 'no session'}
      </span>
      {@render actions?.()}
    </div>
  </header>
  <main>{@render children()}</main>
  <footer class="foot">
    {@render footer?.()}
    <span class="status">{status}</span>
  </footer>
</div>

<style>
  .shell { height: 100%; display: flex; flex-direction: column; }
  .bar {
    height: 34px; flex: none; display: flex; align-items: center; gap: 8px; padding: 0 12px;
    border-bottom: 1px solid var(--border); background: var(--surface);
  }
  .brand { font: 700 12px var(--font-mono); letter-spacing: .06em; color: var(--text); }
  .site { font: 11.5px var(--font-mono); color: var(--accent); }
  .crumb { font: 11px var(--font-mono); color: var(--faint); }
  .crumb b { color: var(--muted); font-weight: 400; }
  .right { margin-left: auto; display: flex; align-items: center; gap: 8px; }
  .chip {
    font: 10.5px var(--font-mono); color: var(--muted); border: 0; padding: 0;
    display: inline-flex; gap: 6px; align-items: center;
  }
  .chip.off { color: var(--faint); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--ok); }
  .chip.off .dot { background: var(--faint); }
  main { flex: 1; min-height: 0; overflow: auto; }
  .foot {
    height: 24px; flex: none; display: flex; align-items: center; gap: 14px; padding: 0 12px;
    border-top: 1px solid var(--border); background: var(--surface);
    font-size: 10.5px; color: var(--faint);
  }
  .status { margin-left: auto; font-family: var(--font-mono); }
</style>
