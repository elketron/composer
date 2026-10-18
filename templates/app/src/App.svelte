<!--
  Starter app: this site's pages (live), a preview, and a message box to the agent.
  Replace it with what the site needs; keep AppShell for the shared look.
-->
<script lang="ts">
  import { AppShell, Button, Kbd, onChange, page, search, send, session, site, type Page, type SearchHit } from '$sdk';

  let pages: SearchHit[] = $state([]);
  let selected = $state(0);
  let preview: Page | null = $state(null);
  let message = $state('');
  let notice = $state('');
  let input: HTMLTextAreaElement | undefined = $state();

  async function loadPages() {
    pages = (await search(`site:${site}`)).sort((a, b) => a.url.localeCompare(b.url));
    selected = Math.min(selected, Math.max(pages.length - 1, 0));
  }

  $effect(() => {
    loadPages();
    return onChange([`site://${site}/**`], loadPages);
  });

  $effect(() => {
    const hit = pages[selected];
    if (hit) page(hit.url).then((p) => (preview = p));
  });

  async function submit() {
    const text = message.trim();
    if (!text) return;
    try {
      const current = pages[selected];
      await send(text, { pages: current ? [current.url] : [] });
      message = '';
      notice = 'sent to the agent';
    } catch (error) {
      notice = String(error);
    }
  }

  function onkeydown(event: KeyboardEvent) {
    const typing = event.target instanceof HTMLTextAreaElement;
    if (typing) {
      if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) submit();
      if (event.key === 'Escape') input?.blur();
      return;
    }
    if (event.key === 'j') selected = Math.min(selected + 1, pages.length - 1);
    if (event.key === 'k') selected = Math.max(selected - 1, 0);
    if (event.key === 'i') {
      event.preventDefault();
      input?.focus();
    }
  }
</script>

<svelte:window {onkeydown} />

<AppShell crumb="pages" status={pages[selected]?.url ?? ''}>
  {#snippet actions()}
    <Button variant="primary" kbd="⌘↵" onclick={submit} disabled={!session || !message.trim()}>Send to agent</Button>
  {/snippet}

  <div class="layout">
    <ul class="list">
      {#each pages as hit, i (hit.url)}
        <li class:sel={i === selected}>
          <button onclick={() => (selected = i)}>
            {hit.title}
            <span>{hit.url.replace(`site://${site}/`, '')}</span>
          </button>
        </li>
      {:else}
        <li class="empty">no pages yet</li>
      {/each}
    </ul>

    <section class="preview">
      {#if preview}
        <pre>{preview.raw}</pre>
      {/if}
    </section>

    <aside class="compose">
      <div class="label">message to the agent</div>
      <textarea bind:this={input} bind:value={message} rows="6" placeholder="i to write · ⌘↵ sends, with the selected page attached"></textarea>
      {#if notice}<p class="notice">{notice}</p>{/if}
      {#if !session}<p class="notice">open this app from the TUI (a) to talk to a session</p>{/if}
    </aside>
  </div>

  {#snippet footer()}
    <Kbd keys="j/k" label="page" />
    <Kbd keys="i" label="write" />
    <Kbd keys="⌘↵" label="send" />
  {/snippet}
</AppShell>

<style>
  .layout { display: grid; grid-template-columns: 260px 1fr 300px; height: 100%; }
  .list { list-style: none; margin: 0; padding: 8px; border-right: 1px solid var(--border); overflow: auto; }
  .list button {
    width: 100%; text-align: left; background: none; border: 0; color: var(--text); font: 13px var(--font-ui);
    padding: 7px 10px; border-radius: var(--radius); cursor: pointer; display: flex; flex-direction: column; gap: 2px;
  }
  .list button span { font: 11px var(--font-mono); color: var(--faint); }
  .list .sel button { background: var(--raised); box-shadow: inset 2px 0 var(--accent); }
  .empty { color: var(--faint); padding: 10px; font-size: 13px; }
  .preview { overflow: auto; padding: 16px 20px; }
  pre { margin: 0; font: 13px/1.6 var(--font-mono); white-space: pre-wrap; color: var(--text); }
  .compose { border-left: 1px solid var(--border); padding: 12px; display: flex; flex-direction: column; gap: 8px; }
  .label { font: 11px var(--font-mono); color: var(--faint); text-transform: uppercase; letter-spacing: 0.06em; }
  textarea {
    font: 13px/1.5 var(--font-ui); background: var(--bg); color: var(--text); resize: none;
    border: 1px solid var(--border-strong); border-radius: var(--radius); padding: 8px 10px;
  }
  textarea:focus { outline: 2px solid var(--accent); outline-offset: 1px; }
  .notice { margin: 0; font-size: 12px; color: var(--muted); }
</style>
