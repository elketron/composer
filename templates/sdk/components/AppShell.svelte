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
    <span class="site">{site}</span>
    {#if crumb}<span class="crumb">/ <b>{crumb}</b></span>{/if}
    <div class="right">
      <span class="chip" title={isConnected() ? 'connected' : 'reconnecting'}>
        <span class="dot" class:off={!isConnected()}></span>
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
    height: 44px; flex: none; display: flex; align-items: center; gap: 12px; padding: 0 14px;
    border-bottom: 1px solid var(--border); background: var(--surface); font-size: 13px;
  }
  .site { font: 600 13px var(--font-mono); }
  .crumb { font: 12px var(--font-mono); color: var(--faint); }
  .crumb b { color: var(--muted); font-weight: 400; }
  .right { margin-left: auto; display: flex; align-items: center; gap: 10px; }
  .chip {
    font: 12px var(--font-mono); color: var(--muted); border: 1px solid var(--border);
    border-radius: var(--radius-sm); padding: 2px 10px; display: inline-flex; gap: 6px; align-items: center;
  }
  .dot { width: 7px; height: 7px; border-radius: var(--radius-sm); background: var(--ok); }
  .dot.off { background: var(--faint); }
  main { flex: 1; min-height: 0; overflow: auto; }
  .foot {
    height: 28px; flex: none; display: flex; align-items: center; gap: 16px; padding: 0 14px;
    border-top: 1px solid var(--border); font-size: 12px; color: var(--faint);
  }
  .status { margin-left: auto; font-family: var(--font-mono); }
</style>
