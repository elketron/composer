<!--
  The phone app: sessions, and a chat with the selected one. Check in, read along,
  send short messages, answer recipe approvals. Talks to the WebSocket API directly.
-->
<script lang="ts">
  import { Badge, isConnected, onMessage, request } from '$sdk';

  type Session = {
    id: string; agent: string; parent: string | null; ticket: string | null;
    state: 'idle' | 'waiting' | 'running' | 'awaiting_approval'; preview: string | null; updated: string;
  };
  type Item =
    | { kind: 'you'; text: string; source: string }
    | { kind: 'agent'; text: string }
    | { kind: 'thinking'; words: number }
    | { kind: 'tool'; id: string; name: string; summary: string; failed: boolean; done: boolean }
    | { kind: 'note'; text: string };
  type Approval = { request: string; session: string; site: string; recipe: string; args: string[]; body: string };

  let sessions: Session[] = $state([]);
  let current: string | null = $state(null);
  let items: Item[] = $state([]);
  let streaming = $state('');
  let thinking = $state(false);
  let lastSeq = 0;
  let approvals: Approval[] = $state([]);
  let draft = $state('');
  let error = $state('');
  let scroller: HTMLElement | undefined = $state();

  const session = $derived(sessions.find((s) => s.id === current));
  const tree = $derived(
    sessions
      .filter((s) => !s.parent)
      .flatMap((top) => [top, ...sessions.filter((s) => s.parent === top.id)]),
  );
  const parentAgent = (s: Session) => sessions.find((p) => p.id === s.parent)?.agent;

  function upsert(info: Session) {
    const i = sessions.findIndex((s) => s.id === info.id);
    if (i >= 0) sessions[i] = info;
    else sessions = [info, ...sessions];
  }

  async function loadSessions() {
    try {
      const result = await request({ type: 'session.list' });
      sessions = result.sessions;
    } catch (e) {
      error = String(e);
    }
  }

  async function open(id: string) {
    current = id;
    items = [];
    streaming = '';
    thinking = false;
    lastSeq = 0;
    location.hash = id;
    await request({ type: 'session.attach', session: id, since: null });
    scrollDown();
  }

  function back() {
    if (current) request({ type: 'session.detach', session: current }).catch(() => {});
    current = null;
    history.replaceState(null, '', location.pathname + location.search);
  }

  async function newSession() {
    const result = await request({ type: 'session.create', agent: 'default', project: null });
    upsert(result.session);
    await open(result.session.id);
  }

  async function sendDraft() {
    const text = draft.trim();
    if (!text || !current) return;
    draft = '';
    await request({ type: 'message.send', session: current, text, pages: [], images: [] }).catch((e) => (error = String(e)));
  }

  async function answer(approval: Approval, decision: 'approve' | 'deny') {
    approvals = approvals.filter((a) => a.request !== approval.request);
    await request({ type: 'approval.respond', request: approval.request, decision }).catch((e) => (error = String(e)));
  }

  function summary(args: any): string {
    if (args?.site && args?.recipe) return `${args.site} › ${args.recipe} ${(args.args ?? []).join(' ')}`;
    return args?.url ?? args?.query ?? args?.ticket ?? args?.status ?? args?.site ?? '';
  }

  /** Agent text: escaped first, then `code` and **bold** (enough for a phone). */
  function inline(text: string): string {
    const escaped = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    return escaped.replace(/`([^`\n]+)`/g, '<code>$1</code>').replace(/\*\*([^*\n]+)\*\*/g, '<b>$1</b>');
  }

  function scrollDown() {
    queueMicrotask(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  }

  // One handler for everything the server pushes.
  $effect(() =>
    onMessage((m) => {
      if (m.type === 'session.updated') return upsert(m.session);
      if (m.type === 'approval.request') {
        if (!approvals.some((a) => a.request === m.request)) approvals = [...approvals, m];
        return;
      }
      if (m.type === 'approval.resolved') {
        approvals = approvals.filter((a) => a.request !== m.request);
        return;
      }
      if (m.type === 'reminder.due') {
        items.push({ kind: 'note', text: `⏰ ${m.title} · ${m.late ? 'was due' : 'due'} ${m.due}` });
        scrollDown();
        return;
      }
      if (m.session !== current || typeof m.seq !== 'number' || m.seq <= lastSeq) return;
      lastSeq = m.seq;
      switch (m.type) {
        case 'thinking.delta': thinking = true; break;
        case 'message.delta': streaming += m.text; break;
        case 'message.done': {
          const msg = m.message;
          if (msg.role === 'user') {
            const source = msg.source?.kind === 'app' ? `${msg.source.site} app` : msg.source?.kind === 'worker' ? `worker #${msg.source.session}` : msg.source?.kind === 'reminder' ? 'reminder' : '';
            items.push({ kind: 'you', text: msg.content, source });
          } else if (msg.role === 'assistant') {
            streaming = '';
            thinking = false;
            if (msg.thinking) items.push({ kind: 'thinking', words: msg.thinking.split(/\s+/).length });
            if (msg.content.trim()) items.push({ kind: 'agent', text: msg.content });
          } else {
            items.push({ kind: 'note', text: msg.content });
          }
          break;
        }
        case 'tool.call': items.push({ kind: 'tool', id: m.call_id, name: m.name, summary: summary(m.args), failed: false, done: false }); break;
        case 'tool.result': {
          const tool = items.findLast((i) => i.kind === 'tool' && i.id === m.call_id);
          if (tool && tool.kind === 'tool') { tool.done = true; tool.failed = m.is_error; }
          break;
        }
        case 'session.compacted': items.push({ kind: 'note', text: `compacted ${m.messages} messages` }); break;
        case 'turn.done': streaming = ''; thinking = false; if (m.cancelled) items.push({ kind: 'note', text: 'cancelled' }); break;
      }
      scrollDown();
    }),
  );

  // (Re)load on connect; re-attach the open session after a reconnect.
  $effect(() => {
    if (!isConnected()) return;
    loadSessions().then(() => {
      const fromHash = location.hash.slice(1);
      if (current) open(current);
      else if (fromHash) open(fromHash);
    });
  });

  const stateLabel: Record<Session['state'], string> = { idle: '', waiting: 'waiting', running: 'running', awaiting_approval: 'needs approval' };
</script>

<div class="screen">
  {#if !session}
    <header>
      <span class="title">chat</span>
      <span class="dot" class:off={!isConnected()}></span>
      <button class="new" onclick={newSession}>+ new</button>
    </header>
    <main class="list">
      {#if error}<p class="error">{error}</p>{/if}
      {#each tree as s (s.id)}
        <button class="session" class:child={!!s.parent} onclick={() => open(s.id)}>
          <span class="row">
            <span>{s.agent} <span class="id">#{s.id}</span></span>
            {#if s.state !== 'idle'}<Badge tone="warn">{stateLabel[s.state]}</Badge>{:else}<span class="when">{s.updated.slice(5, 16).replace('T', ' ')}</span>{/if}
          </span>
          {#if s.preview}<span class="preview">{s.preview}</span>{/if}
        </button>
      {:else}
        <p class="empty">no sessions yet</p>
      {/each}
      {#each approvals as a (a.request)}
        <div class="approval">
          <div><span class="warn">Run recipe?</span> <span class="mono">{a.site} › {a.recipe} {a.args.join(' ')}</span> <span class="id">#{a.session}</span></div>
          <pre>{a.body}</pre>
          <div class="buttons"><button class="primary" onclick={() => answer(a, 'approve')}>Approve</button><button onclick={() => answer(a, 'deny')}>Deny</button></div>
        </div>
      {/each}
    </main>
  {:else}
    <header>
      <button class="back" onclick={back} aria-label="back">‹</button>
      <span class="title">{#if session.parent}<span class="muted">{parentAgent(session)} › </span>{/if}{session.agent}</span>
      <span class="id">#{session.id}</span>
      {#if session.state !== 'idle'}<Badge tone="warn">{stateLabel[session.state]}</Badge>{/if}
      <span class="dot" class:off={!isConnected()}></span>
    </header>
    <main class="chat" bind:this={scroller}>
      {#each items as item, i (i)}
        {#if item.kind === 'you'}
          <div class="you">{#if item.source}<div class="who">{item.source}</div>{/if}{item.text}</div>
        {:else if item.kind === 'agent'}
          <div class="agent">{@html inline(item.text)}</div>
        {:else if item.kind === 'thinking'}
          <div class="tool">▸ thinking · {item.words} words</div>
        {:else if item.kind === 'tool'}
          <div class="tool">▸ <b>{item.name}</b> {item.summary}{#if !item.done} …{:else if item.failed} <span class="err">✗</span>{/if}</div>
        {:else}
          <div class="note">{item.text}</div>
        {/if}
      {/each}
      {#if thinking && !streaming}<div class="tool">▸ thinking …</div>{/if}
      {#if streaming}<div class="agent">{streaming}<span class="caret">▍</span></div>{/if}
      {#each approvals.filter((a) => a.session === session.id) as a (a.request)}
        <div class="approval">
          <div><span class="warn">Run recipe?</span> <span class="mono">{a.site} › {a.recipe} {a.args.join(' ')}</span></div>
          <pre>{a.body}</pre>
          <div class="buttons"><button class="primary" onclick={() => answer(a, 'approve')}>Approve</button><button onclick={() => answer(a, 'deny')}>Deny</button></div>
        </div>
      {/each}
    </main>
    <form class="compose" onsubmit={(e) => { e.preventDefault(); sendDraft(); }}>
      <textarea bind:value={draft} rows="1" placeholder={session.state === 'idle' ? 'Message' : 'Message (after this turn)'}
        onkeydown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) sendDraft(); }}></textarea>
      <button class="send" type="submit" disabled={!draft.trim()} aria-label="send">↑</button>
    </form>
  {/if}
</div>

<style>
  .screen { height: 100dvh; display: flex; flex-direction: column; max-width: 720px; margin: 0 auto; }
  header {
    flex: none; display: flex; align-items: center; gap: 10px; padding: 12px 14px;
    padding-top: max(12px, env(safe-area-inset-top)); border-bottom: 1px solid var(--border); background: var(--surface);
  }
  .title { font: 600 15px var(--font-mono); }
  .muted { color: var(--faint); font-weight: 400; }
  .id { font: 12px var(--font-mono); color: var(--faint); }
  .dot { width: 8px; height: 8px; border-radius: var(--radius-sm); background: var(--ok); margin-left: auto; }
  .dot.off { background: var(--faint); }
  button { font: inherit; color: var(--text); background: none; border: 0; cursor: pointer; }
  .new { font: 13px var(--font-mono); color: var(--accent); border: 1px solid var(--border-strong); border-radius: var(--radius); padding: 4px 10px; }
  .back { font-size: 24px; line-height: 1; padding: 0 6px 0 0; color: var(--muted); }
  main { flex: 1; overflow-y: auto; -webkit-overflow-scrolling: touch; }
  .list { padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
  .session {
    text-align: left; display: flex; flex-direction: column; gap: 3px; padding: 11px 12px;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius);
  }
  .session.child { margin-left: 18px; }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 8px; font-size: 15px; }
  .when { font-size: 12px; color: var(--faint); }
  .preview { font-size: 13px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .empty, .error { color: var(--faint); font-size: 14px; }
  .error { color: var(--err); }
  .chat { padding: 14px; display: flex; flex-direction: column; gap: 10px; font-size: 15px; }
  .you {
    align-self: flex-end; max-width: 85%; white-space: pre-wrap; background: var(--you);
    border: 1px solid #8fb3e826; border-radius: var(--radius) var(--radius) var(--radius-sm) var(--radius); padding: 8px 12px;
  }
  .who { font: 11px var(--font-mono); color: var(--faint); margin-bottom: 2px; }
  .agent { white-space: pre-wrap; line-height: 1.5; }
  .agent :global(code) { font: 13px var(--font-mono); color: var(--muted); }
  .agent :global(b) { font-weight: 600; }
  .tool { font: 12px var(--font-mono); color: var(--faint); }
  .tool b { color: var(--info); font-weight: 400; }
  .err { color: var(--err); }
  .note { font: 12px var(--font-mono); color: var(--warn); }
  .caret { color: var(--accent); }
  .approval { border: 1px solid var(--warn); border-radius: var(--radius); padding: 12px; background: var(--surface); font-size: 14px; }
  .warn { color: var(--warn); }
  .mono { font: 13px var(--font-mono); color: var(--muted); }
  pre {
    font: 12px var(--font-mono); background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius);
    padding: 8px; margin: 8px 0 10px; white-space: pre-wrap; max-height: 40vh; overflow: auto;
  }
  .buttons { display: flex; gap: 8px; }
  .buttons button { flex: 1; height: 40px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--raised); }
  .buttons .primary { background: var(--accent); border-color: var(--accent); color: var(--bg); font-weight: 600; }
  .compose {
    flex: none; display: flex; gap: 8px; padding: 10px; padding-bottom: max(10px, env(safe-area-inset-bottom));
    border-top: 1px solid var(--border); background: var(--surface);
  }
  textarea {
    flex: 1; resize: none; font: 16px/1.4 var(--font-ui); color: var(--text); background: var(--bg);
    border: 1px solid var(--border-strong); border-radius: var(--radius); padding: 9px 12px; max-height: 30vh; field-sizing: content;
  }
  textarea:focus { outline: 2px solid var(--accent); outline-offset: 1px; }
  .send { width: 44px; border-radius: var(--radius); background: var(--accent); color: var(--bg); font-size: 18px; font-weight: 700; }
  .send:disabled { opacity: 0.4; }
</style>
