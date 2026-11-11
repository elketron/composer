<!--
  The comment shell: threads anchored to the things a view shows (docs/workflows.md, "Threads";
  what the agent does with them: site://home/threads.md). A view marks its anchors with
  `data-anchor="<id>" data-anchor-label="<label>"` and mounts this once:

    <Threads bind:this={shell} bind:picking bind:open bind:threads attach={(anchor) => [...]} />

  `picking` is the comment mode (hover highlights anchors, a click picks one); `open` shows the
  panel; `threads` is every thread under `root` (`<root>/threads/*.md`), kept live. Pins with the
  open-thread count sit on anchored elements; an id counts its sub-anchors (`a` counts `a/...`).
  `shell.comment({ id, label })` starts a thread on an anchor the view picked itself (a region).
-->
<script lang="ts">
  import { onChange, page, putPage, search, send, session, site } from '../client.svelte';
  import {
    agentMessage,
    appendMessage,
    formatThread,
    openCount,
    parseThread,
    setStatus,
    threadTime,
    threadUrl,
    type Anchor,
    type Thread,
    type ThreadMessage,
  } from '../threads';
  import Button from './Button.svelte';

  let {
    root = `site://${site}`,
    threads = $bindable([]),
    picking = $bindable(false),
    open = $bindable(false),
    attach = () => [],
    onopen,
  }: {
    /** The site or work folder whose `threads/` holds the threads. */
    root?: string;
    threads?: Thread[];
    picking?: boolean;
    open?: boolean;
    /** Pages sent to the agent with a comment on `anchor`, besides the thread. */
    attach?: (anchor: string) => string[];
    /** A thread was opened in the panel (the view may bring its anchor into sight). */
    onopen?: (thread: Thread) => void;
  } = $props();

  const folder = $derived(`${root.replace(/\/$/, '')}/threads/`);
  let shown: string | null = $state(null);
  let draft: Anchor | null = $state(null);
  let filter: string | null = $state(null);
  let resolvedToo = $state(false);
  let text = $state('');
  let busy = $state(false);
  let notice = $state('');
  let box: HTMLTextAreaElement | undefined = $state();
  type Box = { left: number; top: number; width: number; height: number };
  let pins: { id: string; left: number; top: number; count: number }[] = $state([]);
  let hover: (Box & { label: string }) | null = $state(null);
  let focus: Box[] = $state([]);

  const thread = $derived(threads.find((t) => t.url === shown) ?? null);
  const listed = $derived(
    threads.filter(
      (t) =>
        (resolvedToo || t.status === 'open') && (!filter || t.anchor === filter || t.anchor.startsWith(`${filter}/`)),
    ),
  );
  const openTotal = $derived(threads.filter((t) => t.status === 'open').length);

  const order = (a: Thread, b: Thread) =>
    a.status === b.status ? b.updated.localeCompare(a.updated) : a.status === 'open' ? -1 : 1;

  function upsert(next: Thread) {
    threads = [...threads.filter((t) => t.url !== next.url), next].sort(order);
  }

  async function reload(url: string) {
    try {
      upsert(parseThread((await page(url)).raw, url));
    } catch {
      threads = threads.filter((t) => t.url !== url);
    }
  }

  $effect(() => {
    const dir = folder;
    const siteName = dir.replace(/^site:\/\//, '').split('/')[0];
    search(`site:${siteName} type:thread`).then(async (hits) => {
      const urls = hits.map((h) => h.url).filter((u) => u.startsWith(dir) && !u.slice(dir.length).includes('/'));
      const read = await Promise.all(urls.map((u) => page(u).then((p) => parseThread(p.raw, u)).catch(() => null)));
      threads = read.filter((t): t is Thread => !!t).sort(order);
    });
    return onChange([`${dir}*`], (change) => reload(change.url));
  });

  /** Reads, changes and writes a thread page with its hash; a concurrent write (the agent's
   * reply) makes the write fail, and the change is applied again to the new text. */
  async function change(url: string, edit: (raw: string) => string) {
    for (let attempt = 0; ; attempt++) {
      const current = await page(url);
      try {
        await putPage(url, edit(current.raw), current.hash);
        return reload(url);
      } catch (error) {
        if (attempt >= 3) throw error;
      }
    }
  }

  async function post(toAgent: boolean) {
    const body = text.trim();
    if (!body || busy) return;
    busy = true;
    notice = '';
    try {
      const message: ThreadMessage = { author: 'you', at: threadTime(), text: body };
      let target: Pick<Thread, 'url' | 'anchor' | 'anchor_label'>;
      if (draft) {
        const url = threadUrl(root, draft, message.at);
        await putPage(url, formatThread(draft, message));
        await reload(url);
        target = { url, anchor: draft.id, anchor_label: draft.label };
        draft = null;
        shown = url;
      } else if (thread) {
        target = thread;
        // A reply reopens a resolved thread.
        await change(thread.url, (raw) => setStatus(appendMessage(raw, message), 'open', message.at));
      } else return;
      text = '';
      if (toAgent) {
        await send(agentMessage(target, body), { pages: [target.url, ...attach(target.anchor)] });
        notice = 'sent to the agent';
      }
    } catch (error) {
      notice = String(error);
    } finally {
      busy = false;
    }
  }

  async function resolve(status: 'open' | 'resolved') {
    if (!thread) return;
    try {
      await change(thread.url, (raw) => setStatus(raw, status));
    } catch (error) {
      notice = String(error);
    }
  }

  /** Starts a thread on an anchor (the view's own picking, e.g. a dragged region). */
  export function comment(anchor: Anchor) {
    draft = anchor;
    shown = null;
    picking = false;
    open = true;
    notice = '';
    queueMicrotask(() => box?.focus());
  }

  /** Opens a thread, or the list of threads on an anchor. */
  export function show(target: { thread?: string; anchor?: string }) {
    draft = null;
    open = true;
    if (target.thread) {
      shown = target.thread;
      const t = threads.find((x) => x.url === target.thread);
      if (t) onopen?.(t);
    } else {
      const on = threads.filter((t) => t.status === 'open' && (t.anchor === target.anchor || t.anchor.startsWith(`${target.anchor}/`)));
      if (on.length === 1) return show({ thread: on[0].url });
      shown = null;
      filter = target.anchor ?? null;
    }
  }

  function back() {
    draft = null;
    shown = null;
    filter = null;
    notice = '';
  }

  // ---- anchors on the page: pins, the hover highlight, the open thread's anchor ----

  /** An element's box clipped by its scrolling ancestors; null when nothing of it shows. */
  function visible(el: Element): Box | null {
    const r = el.getBoundingClientRect();
    let [left, top, right, bottom] = [r.left, r.top, r.right, r.bottom];
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      if (getComputedStyle(p).overflow === 'visible') continue;
      const q = p.getBoundingClientRect();
      [left, top, right, bottom] = [Math.max(left, q.left), Math.max(top, q.top), Math.min(right, q.right), Math.min(bottom, q.bottom)];
    }
    return right - left < 2 || bottom - top < 2 ? null : { left, top, width: right - left, height: bottom - top };
  }

  const anchors = () =>
    [...document.querySelectorAll<HTMLElement>('[data-anchor]')].filter((el) => !el.closest('.threads-ui'));

  function layout() {
    const next: typeof pins = [];
    const lit: Box[] = [];
    for (const el of anchors()) {
      const id = el.dataset.anchor!;
      const count = openCount(threads, id);
      const seen = count || (thread && thread.anchor === id) ? visible(el) : null;
      if (!seen) continue;
      if (count) next.push({ id, left: Math.round(seen.left + seen.width), top: Math.round(seen.top), count });
      if (open && thread?.anchor === id) lit.push(seen);
    }
    if (JSON.stringify(next) !== JSON.stringify(pins)) pins = next;
    if (JSON.stringify(lit) !== JSON.stringify(focus)) focus = lit;
  }

  let frame = 0;
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), layout()));
  };

  $effect(() => {
    // Again when the threads, the open thread or the panel change.
    void [threads, shown, open];
    schedule();
  });

  $effect(() => {
    const observer = new MutationObserver((records) => {
      if (records.some((r) => !(r.target instanceof Element && r.target.closest('.threads-ui')))) schedule();
    });
    observer.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['data-anchor', 'style', 'class'] });
    addEventListener('scroll', schedule, true);
    addEventListener('resize', schedule);
    return () => {
      observer.disconnect();
      removeEventListener('scroll', schedule, true);
      removeEventListener('resize', schedule);
    };
  });

  // Comment mode: hover highlights an anchor, a click picks it, Escape leaves.
  $effect(() => {
    if (!picking) {
      hover = null;
      return;
    }
    const target = (event: Event) => {
      const el = (event.target as Element | null)?.closest?.('[data-anchor]') as HTMLElement | null;
      return el && !el.closest('.threads-ui') ? el : null;
    };
    const move = (event: PointerEvent) => {
      const el = target(event);
      const seen = el && visible(el);
      hover = el && seen ? { ...seen, label: el.dataset.anchorLabel ?? el.dataset.anchor! } : null;
    };
    const click = (event: MouseEvent) => {
      const el = target(event);
      if (!el) return;
      event.preventDefault();
      event.stopPropagation();
      comment({ id: el.dataset.anchor!, label: el.dataset.anchorLabel ?? el.dataset.anchor! });
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') picking = false;
    };
    addEventListener('pointermove', move, true);
    addEventListener('click', click, true);
    addEventListener('keydown', key, true);
    document.documentElement.classList.add('threads-picking');
    return () => {
      removeEventListener('pointermove', move, true);
      removeEventListener('click', click, true);
      removeEventListener('keydown', key, true);
      document.documentElement.classList.remove('threads-picking');
    };
  });

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      post(true);
    }
    if (event.key === 'Escape') (event.currentTarget as HTMLElement).blur();
  }

  const when = (at: string) => {
    const d = new Date(at);
    if (isNaN(+d)) return at;
    const two = (n: number) => String(n).padStart(2, '0');
    return `${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
  };
</script>

<div class="threads-ui">
  {#each pins as pin (pin.id)}
    <button
      class="pin"
      style:left="{pin.left}px"
      style:top="{pin.top}px"
      title="{pin.count} open thread{pin.count === 1 ? '' : 's'}"
      onclick={() => show({ anchor: pin.id })}>{pin.count}</button
    >
  {/each}
  {#each focus as f}
    <div class="ring focus" style:left="{f.left}px" style:top="{f.top}px" style:width="{f.width}px" style:height="{f.height}px"></div>
  {/each}
  {#if hover}
    <div class="ring" style:left="{hover.left}px" style:top="{hover.top}px" style:width="{hover.width}px" style:height="{hover.height}px">
      <span>{hover.label}</span>
    </div>
  {/if}

  {#if open}
    <aside class="panel">
      <header>
        {#if draft || thread || filter}
          <button class="link" onclick={back}>‹ threads</button>
        {:else}
          <b>threads</b><span class="faint">{openTotal} open</span>
        {/if}
        <span class="grow"></span>
        {#if !draft && !thread}
          <label class="faint"><input type="checkbox" bind:checked={resolvedToo} /> resolved</label>
        {/if}
        <button class="link" title="close" onclick={() => (open = false)}>×</button>
      </header>

      {#if draft || thread}
        <div class="on">
          <span class="faint">on</span> <b>{draft?.label ?? thread?.anchor_label}</b>
          {#if thread}<span class="status {thread.status}">{thread.status}</span>{/if}
        </div>
        {#if thread}
          <ol class="messages">
            {#each thread.messages as m}
              <li class:agent={m.author !== 'you'}>
                <div class="meta"><b>{m.author}</b> <span class="faint">{when(m.at)}</span></div>
                <div class="text">{m.text}</div>
              </li>
            {/each}
          </ol>
        {/if}
        <div class="compose">
          <textarea
            bind:this={box}
            bind:value={text}
            rows="3"
            placeholder={draft ? 'Comment… (⌘↵ sends to the agent)' : 'Reply… (⌘↵ sends to the agent)'}
            onkeydown={keydown}
          ></textarea>
          <div class="row">
            {#if thread}
              {#if thread.status === 'open'}
                <Button variant="ghost" onclick={() => resolve('resolved')}>Resolve</Button>
              {:else}
                <Button variant="ghost" onclick={() => resolve('open')}>Reopen</Button>
              {/if}
            {/if}
            <span class="grow"></span>
            <Button onclick={() => post(false)} disabled={busy || !text.trim()}>{draft ? 'Comment' : 'Reply'}</Button>
            <Button variant="primary" kbd="⌘↵" onclick={() => post(true)} disabled={busy || !text.trim() || !session}>Send to agent</Button>
          </div>
          {#if notice}<p class="faint">{notice}</p>{/if}
        </div>
      {:else}
        <ul class="list">
          {#each listed as t (t.url)}
            <li>
              <button onclick={() => show({ thread: t.url })}>
                <span class="line"><b>{t.anchor_label}</b><span class="status {t.status}">{t.status}</span></span>
                <span class="excerpt">{t.messages.at(-1)?.author}: {t.messages.at(-1)?.text}</span>
                <span class="faint">{t.messages.length} message{t.messages.length === 1 ? '' : 's'} · {when(t.updated)}</span>
              </button>
            </li>
          {:else}
            <li class="faint empty">no {resolvedToo ? '' : 'open '}threads{filter ? ' here' : ''}: comment mode, then click a thing</li>
          {/each}
        </ul>
      {/if}
    </aside>
  {/if}
</div>

<style>
  :global(html.threads-picking), :global(html.threads-picking *) { cursor: crosshair !important; }
  .pin {
    position: fixed; z-index: 40; transform: translate(calc(-100% - 3px), 3px);
    min-width: 18px; height: 18px; padding: 0 4px; border: 0; cursor: pointer;
    font: 600 10.5px/18px var(--font-mono); background: var(--warn); color: var(--bg);
    box-shadow: 0 0 0 1px var(--bg);
  }
  .pin:hover { background: var(--accent); }
  .ring {
    position: fixed; z-index: 39; pointer-events: none; outline: 1px solid var(--accent);
    background: var(--accent-dim);
  }
  .ring.focus { outline: 1px dashed var(--warn); background: none; }
  .ring span {
    position: absolute; left: -1px; bottom: 100%; white-space: nowrap; padding: 0 5px;
    font: 10.5px/16px var(--font-mono); background: var(--accent); color: var(--bg);
  }
  .panel {
    position: fixed; z-index: 50; right: 0; top: var(--threads-top, 41px); bottom: var(--threads-bottom, 29px);
    width: min(340px, 100vw); display: flex; flex-direction: column; font-size: 12px;
    background: var(--surface); border-left: 1px solid var(--border-strong); box-shadow: -8px 0 24px #0006;
  }
  header { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--border); }
  .grow { flex: 1; }
  .faint { color: var(--faint); font: 11px var(--font-mono); }
  label.faint { display: inline-flex; align-items: center; gap: 4px; cursor: pointer; }
  .link { background: none; border: 0; padding: 0 2px; color: var(--muted); font: 12px var(--font-mono); cursor: pointer; }
  .link:hover { color: var(--accent); }
  .on { padding: 8px 10px; border-bottom: 1px solid var(--border); line-height: 1.5; }
  .on .status { margin-left: 4px; }
  .status { flex: none; align-self: flex-start; font: 9.5px/15px var(--font-mono); text-transform: uppercase; letter-spacing: .08em; padding: 0 5px; border: 1px solid currentColor; color: var(--warn); }
  .status.resolved { color: var(--ok); }
  .messages { list-style: none; margin: 0; padding: 4px 0; overflow: auto; flex: 1; min-height: 0; }
  .messages li { padding: 6px 10px; border-left: 2px solid transparent; }
  .messages li.agent { border-left-color: var(--info); }
  .meta b { font: 600 11px var(--font-mono); }
  .agent .meta b { color: var(--info); }
  .text { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.5; }
  .compose { border-top: 1px solid var(--border); padding: 8px 10px; display: flex; flex-direction: column; gap: 6px; margin-top: auto; }
  .compose textarea {
    font: 12px/1.5 var(--font-ui); background: var(--bg); color: var(--text); resize: vertical;
    border: 1px solid var(--border-strong); padding: 6px 8px;
  }
  .compose textarea:focus { outline: 1px solid var(--accent); }
  .row { display: flex; gap: 6px; align-items: center; }
  .compose p { margin: 0; }
  .list { list-style: none; margin: 0; padding: 0; overflow: auto; flex: 1; }
  .list button {
    width: 100%; text-align: left; background: none; border: 0; border-bottom: 1px solid var(--border);
    color: var(--text); font: 12px var(--font-ui); padding: 7px 10px; cursor: pointer;
    display: flex; flex-direction: column; gap: 2px;
  }
  .list button:hover { background: var(--raised); }
  .line { display: flex; justify-content: space-between; gap: 8px; }
  .excerpt { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 10px; }
</style>
