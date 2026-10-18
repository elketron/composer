<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    variant = 'default',
    kbd = '',
    disabled = false,
    onclick,
    children,
  }: {
    variant?: 'default' | 'primary' | 'ghost' | 'danger';
    /** Key hint shown inside the button, e.g. `n` or `⌘↵`. */
    kbd?: string;
    disabled?: boolean;
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

<button class="btn {variant}" {disabled} {onclick}>
  {@render children()}
  {#if kbd}<kbd>{kbd}</kbd>{/if}
</button>

<style>
  .btn {
    font: 500 13px var(--font-ui); height: 30px; padding: 0 12px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--raised); color: var(--text);
    display: inline-flex; align-items: center; justify-content: center; gap: 8px; cursor: pointer;
  }
  .btn:hover:not(:disabled) { border-color: var(--faint); }
  .btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .primary { background: var(--accent); border-color: var(--accent); color: var(--bg); }
  .ghost { background: none; border-color: transparent; color: var(--muted); }
  .danger { color: var(--err); }
  kbd {
    font: 11px var(--font-mono); padding: 1px 5px; border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--muted);
  }
  .primary kbd { color: var(--bg); border-color: #0003; background: #0001; }
</style>
