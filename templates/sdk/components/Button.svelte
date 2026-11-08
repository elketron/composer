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
    font: 500 11px var(--font-mono); height: 26px; padding: 0 9px; border-radius: 0;
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text);
    display: inline-flex; align-items: center; justify-content: center; gap: 7px; cursor: pointer;
  }
  .btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); background: var(--accent-dim); }
  .btn:focus-visible { outline: 1px solid var(--accent); outline-offset: 1px; }
  .btn:disabled { opacity: 0.45; cursor: default; }
  .primary { background: var(--accent); border-color: var(--accent); color: var(--bg); }
  .primary:hover:not(:disabled) { background: #a6c4f0; border-color: #a6c4f0; color: var(--bg); }
  .ghost { background: none; border-color: transparent; color: var(--muted); }
  .danger { color: var(--err); }
  .danger:hover:not(:disabled) { background: #e07c7c14; border-color: var(--err); color: var(--err); }
  kbd {
    font: 9.5px var(--font-mono); padding: 0 4px; border-radius: 0;
    border: 1px solid var(--border-strong); background: var(--bg); color: var(--muted);
  }
  .primary kbd { color: var(--bg); border-color: #0003; background: #0001; }
</style>
