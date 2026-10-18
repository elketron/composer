<!--
  Ticket board for site://orchestration: one column per status, live.
  Tickets are pages; this app only reads them. New tickets are asked of the agent,
  which creates them with the site's `new-ticket` action.
-->
<script lang="ts">
  import { AppShell, Badge, Button, Kbd, onChange, page, search, send, session, type Page } from '$sdk';

  type Status = 'open' | 'claimed' | 'done' | 'failed';
  const columns: { status: Status; tone: 'muted' | 'warn' | 'ok' | 'err' }[] = [
    { status: 'open', tone: 'muted' },
    { status: 'claimed', tone: 'warn' },
    { status: 'done', tone: 'ok' },
    { status: 'failed', tone: 'err' },
  ];

  let tickets: Page[] = $state([]);
  let column = $state(0);
  let row = $state(0);
  let detail = $state(false);
  let asking = $state(false);
  let title = $state('');
  let notice = $state('');
  let input: HTMLInputElement | undefined = $state();

  const field = (t: Page, key: string) => String(t.frontmatter[key] ?? '');
  const inColumn = (status: Status) =>
    tickets.filter((t) => (field(t, 'status') || 'open') === status).sort((a, b) => field(a, 'id').localeCompare(field(b, 'id')));
  const selected = $derived(inColumn(columns[column].status)[row]);

  async function load() {
    const hits = await search('type:ticket site:orchestration');
    tickets = await Promise.all(hits.map((h) => page(h.url)));
    row = Math.min(row, Math.max(inColumn(columns[column].status).length - 1, 0));
  }

  $effect(() => {
    load();
    return onChange(['site://orchestration/tickets/**'], load);
  });

  async function askForTicket() {
    const text = title.trim();
    if (!text) return;
    try {
      await send(`Create a ticket titled "${text}" with the new-ticket action, then fill in its Task, Context and Done when.`);
      notice = 'asked the agent to create the ticket';
      title = '';
      asking = false;
    } catch (error) {
      notice = String(error);
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (asking) {
      if (event.key === 'Enter') askForTicket();
      if (event.key === 'Escape') asking = false;
      return;
    }
    const count = inColumn(columns[column].status).length;
    switch (event.key) {
      case 'h': column = Math.max(column - 1, 0); row = 0; break;
      case 'l': column = Math.min(column + 1, columns.length - 1); row = 0; break;
      case 'j': row = Math.min(row + 1, Math.max(count - 1, 0)); break;
      case 'k': row = Math.max(row - 1, 0); break;
      case 'Enter': detail = !detail; break;
      case 'Escape': detail = false; break;
      case 'n':
        event.preventDefault();
        asking = true;
        queueMicrotask(() => input?.focus());
        break;
    }
  }
</script>

<svelte:window {onkeydown} />

<AppShell crumb="tickets" status={selected?.url ?? ''}>
  {#snippet actions()}
    <Button kbd="n" onclick={() => { asking = true; queueMicrotask(() => input?.focus()); }} disabled={!session}>New ticket</Button>
  {/snippet}

  {#if asking}
    <div class="ask">
      <input bind:this={input} bind:value={title} placeholder="ticket title · enter asks the agent to create it · esc cancels" />
    </div>
  {/if}
  {#if notice}<p class="notice">{notice}</p>{/if}

  <div class="board" class:with-detail={detail && selected}>
    {#each columns as col, c (col.status)}
      <section class="col">
        <h3><Badge tone={col.tone}>{col.status}</Badge><span class="n">{inColumn(col.status).length}</span></h3>
        {#each inColumn(col.status) as ticket, r (ticket.url)}
          <button class="card" class:sel={c === column && r === row} onclick={() => { column = c; row = r; detail = true; }}>
            <span class="id">{field(ticket, 'id')}</span>
            <span class="t">{ticket.title}</span>
            <span class="meta">
              {#if col.status === 'claimed'}<span class="pulse"></span>{/if}
              {field(ticket, 'agent') || 'worker'}
              {#if field(ticket, 'plan')}<span class="plan">{field(ticket, 'plan').replace('site://', '')}</span>{/if}
            </span>
          </button>
        {:else}
          <div class="empty">no {col.status} tickets</div>
        {/each}
      </section>
    {/each}
    {#if detail && selected}
      <aside class="detail"><pre>{selected.raw}</pre></aside>
    {/if}
  </div>

  {#snippet footer()}
    <Kbd keys="h/l" label="column" />
    <Kbd keys="j/k" label="ticket" />
    <Kbd keys="enter" label="details" />
    <Kbd keys="n" label="new ticket" />
  {/snippet}
</AppShell>

<style>
  .board { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; padding: 14px; align-items: start; }
  .board.with-detail { grid-template-columns: repeat(4, minmax(0, 1fr)) minmax(280px, 1.4fr); }
  .col h3 { font: 500 12px var(--font-mono); margin: 0 0 10px; display: flex; gap: 8px; align-items: center; }
  .n { color: var(--faint); }
  .card {
    width: 100%; text-align: left; display: flex; flex-direction: column; gap: 2px; cursor: pointer;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius);
    padding: 10px 12px; margin-bottom: 8px; color: var(--text); font: 13px var(--font-ui);
  }
  .card.sel { border-color: var(--accent); }
  .id { font: 11px var(--font-mono); color: var(--faint); }
  .t { margin-bottom: 6px; }
  .meta { display: flex; gap: 8px; align-items: center; font: 11px var(--font-mono); color: var(--faint); }
  .plan { margin-left: auto; color: var(--muted); }
  .pulse { width: 6px; height: 6px; border-radius: var(--radius-sm); background: var(--warn); box-shadow: 0 0 0 3px #d6b06a22; }
  .empty { font-size: 13px; color: var(--faint); border: 1px dashed var(--border); border-radius: var(--radius); padding: 16px; text-align: center; }
  .detail { border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); padding: 12px 14px; overflow: auto; max-height: calc(100vh - 120px); }
  pre { margin: 0; font: 12px/1.6 var(--font-mono); white-space: pre-wrap; }
  .ask { padding: 14px 14px 0; }
  .ask input {
    width: 100%; font: 13px var(--font-ui); height: 32px; padding: 0 10px; border-radius: var(--radius);
    border: 1px solid var(--accent); background: var(--bg); color: var(--text);
  }
  .notice { margin: 8px 14px 0; font-size: 12px; color: var(--muted); }
</style>
