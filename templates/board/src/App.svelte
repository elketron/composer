<!--
  The board of a pipeline site: its works, the stages of the one you pick with their state, the
  output of a stage, and the gate (Continue / Rerun / Stop). Gates are page state: the buttons
  write the stage's `_stage.md`, and the server goes on from there.
-->
<script lang="ts">
  import { AppShell, Badge, Button, Kbd, file, onChange, page, putPage, request, session, site, type Page } from '$sdk';

  interface StageSummary { name: string; title: string; gate: boolean; harness?: string; model?: string; worktree: boolean }
  interface Pipeline { site: string; title: string; about: string; stages: StageSummary[]; needs_project: boolean; works: number }
  interface StageState { name: string; status: string; ticket?: string; summary?: string; outputs: string[]; stale: boolean; changed: string[] }
  interface Work { url: string; pipeline: string; title: string; status?: string; stages: StageState[]; project?: string }

  const finished = ['done', 'approved'];
  const tone = (status: string): 'muted' | 'warn' | 'ok' | 'err' | 'info' =>
    finished.includes(status) ? 'ok' : status === 'review' ? 'info' : status === 'running' || status === 'rerun' ? 'warn' : status === 'failed' || status === 'stopped' ? 'err' : 'muted';

  let pipeline: Pipeline | null = $state(null);
  let works: Work[] = $state([]);
  let error = $state('');
  let notice = $state('');
  let chosen: string | null = $state(new URLSearchParams(location.search).get('work'));
  let stageIndex: number | null = $state(null);
  let outputIndex = $state(0);
  let output: Page | null = $state(null);
  let editing = $state(false);
  let draft = $state('');
  let rerunning = $state(false);
  let note = $state('');
  let creating = $state(false);
  let newName = $state('');
  let newProject = $state('');
  let newBrief = $state('');
  let busy = $state(false);
  /** The agent's own version of the output on show (`.agent/`), while showing your edits. */
  let agentText: string | null = $state(null);
  let diffing = $state(false);

  const nameOf = (w: Work) => w.url.replace(/^site:\/\/[^/]+\//, '').replace(/\/index\.md$/, '');
  // The one asked for, else one waiting for you, else the first.
  const work = $derived(
    works.find((w) => nameOf(w) === chosen) ?? works.find((w) => w.stages.some((s) => s.status === 'review')) ?? works[0] ?? null,
  );
  /** The stage worth looking at: one waiting for you, else running, else the next (stale or not run), else the last. */
  const current = (w: Work) => {
    const at = (test: (s: StageState) => boolean) => w.stages.findIndex(test);
    for (const i of [at((s) => s.status === 'review' && !s.stale), at((s) => s.status === 'running'), at((s) => s.stale || !finished.includes(s.status))]) {
      if (i >= 0) return i;
    }
    return Math.max(w.stages.length - 1, 0);
  };
  const stage = $derived(work ? work.stages[stageIndex ?? current(work)] : undefined);
  const contract = $derived(pipeline?.stages.find((s) => s.name === stage?.name));
  /** What Run starts: the first stage that is stale or not finished. */
  const next = $derived(work?.stages.find((s) => s.stale || !finished.includes(s.status)));
  const runnable = $derived(
    !!work && !work.stages.some((s) => s.status === 'running') && !!next && (next.status !== 'review' || next.stale),
  );
  const dot = (w: Work) => (w.stages.some((s) => s.status === 'review' && !s.stale) ? 'accent' : w.stages.some((s) => s.stale) ? 'warn' : w.stages.some((s) => s.status === 'running') ? 'warn' : w.stages.every((s) => finished.includes(s.status)) ? 'ok' : 'muted');

  async function load() {
    try {
      const result = await request({ type: 'pipeline.list' });
      pipeline = result.pipelines.find((p: Pipeline) => p.site === site) ?? null;
      works = result.works.filter((w: Work) => w.pipeline === site);
      error = pipeline ? '' : `site://${site} is not a pipeline`;
    } catch (e) {
      error = String(e);
    }
  }

  // The output on show follows the stage picked and the page changing on disk.
  $effect(() => {
    const url = stage?.outputs[Math.min(outputIndex, (stage?.outputs.length ?? 1) - 1)];
    if (!url) {
      output = null;
      return;
    }
    page(url).then((p) => (output = p)).catch(() => (output = null));
  });

  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    load();
    return onChange([`site://${site}/**`], () => {
      clearTimeout(timer);
      timer = setTimeout(load, 250);
    });
  });

  /** Sets `key:` in a page's frontmatter, as the server does. */
  function setField(raw: string, key: string, value: string): string {
    const match = raw.match(/^---\n([\s\S]*?)\n---/);
    if (!match) return `---\n${key}: ${value}\n---\n${raw}`;
    const lines = match[1].split('\n');
    const i = lines.findIndex((l) => l.startsWith(`${key}:`));
    if (i >= 0) lines[i] = `${key}: ${value}`;
    else lines.push(`${key}: ${value}`);
    return `---\n${lines.join('\n')}\n---${raw.slice(match[0].length)}`;
  }

  async function decide(status: 'approved' | 'rerun' | 'stopped', withNote = '') {
    if (!work || !stage) return;
    busy = true;
    try {
      const url = work.url.replace(/index\.md$/, `${stage.name}/_stage.md`);
      const current = await page(url);
      let raw = setField(current.raw, 'status', status);
      if (withNote) raw = setField(raw, 'note', JSON.stringify(withNote.replace(/\s+/g, ' ').trim()));
      await putPage(url, raw, current.hash);
      notice = { approved: 'approved: the next stage starts', rerun: 'running it again with your note', stopped: 'stopped here' }[status];
      rerunning = false;
      note = '';
      stageIndex = null;
    } catch (e) {
      notice = String(e);
    } finally {
      busy = false;
    }
  }

  async function run(from?: string) {
    if (!work) return;
    busy = true;
    try {
      const result = await request({ type: 'work.run', work: `${site}/${nameOf(work)}`, stage: from ?? null, session });
      notice = result.work?.status ?? 'started';
      stageIndex = null;
    } catch (e) {
      notice = String(e);
    } finally {
      busy = false;
    }
  }

  async function create() {
    const name = newName.trim().replace(/\s+/g, '-');
    if (!name) return;
    busy = true;
    try {
      await request({ type: 'work.new', pipeline: site, name, brief: newBrief.trim() || null, project: newProject.trim() || null });
      creating = false;
      chosen = name;
      newName = newBrief = newProject = '';
      notice = `made ${name}: Run starts its first stage`;
    } catch (e) {
      notice = String(e);
    } finally {
      busy = false;
    }
  }

  async function save() {
    if (!output) return;
    try {
      await putPage(output.url, draft, output.hash);
      editing = false;
      notice = 'saved: the next stage reads your version';
    } catch (e) {
      notice = String(e);
    }
  }

  /** The kept copy of an output: `<stage>/.agent/<file>`. */
  const keptUrl = (url: string) => url.replace(/\/(\d+_[^/]+)\//, '/$1/.agent/');

  async function toggleDiff() {
    if (diffing) { diffing = false; return; }
    if (!output) return;
    try {
      agentText = await (await file(keptUrl(output.url))).text();
      diffing = true;
    } catch {
      notice = 'no copy of what the agent wrote (it ran before this was kept)';
    }
  }

  /** Lines of `after` against `before`: same, added by you, or removed by you. */
  function lineDiff(before: string, after: string): { kind: ' ' | '+' | '-'; text: string }[] {
    const a = before.split('\n'), b = after.split('\n');
    const n = a.length, m = b.length;
    const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
    for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    const out: { kind: ' ' | '+' | '-'; text: string }[] = [];
    let i = 0, j = 0;
    while (i < n || j < m) {
      if (i < n && j < m && a[i] === b[j]) { out.push({ kind: ' ', text: a[i] }); i++; j++; }
      else if (j < m && (i === n || lcs[i][j + 1] >= lcs[i + 1][j])) { out.push({ kind: '+', text: b[j] }); j++; }
      else { out.push({ kind: '-', text: a[i] }); i++; }
    }
    return out;
  }
  const edits = $derived(diffing && output && agentText !== null ? lineDiff(body(agentText), body(output.raw)) : []);

  const body = (raw: string) => raw.replace(/^---\n[\s\S]*?\n---\n?/, '').trimStart();
  const short = (url: string) => url.replace(/^site:\/\/[^/]+\//, '');

  function onkeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement;
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
      if (event.key === 'Escape') { rerunning = creating = editing = false; target.blur(); }
      return;
    }
    if (!work) return;
    const i = works.indexOf(work);
    const s = stageIndex ?? current(work);
    switch (event.key) {
      case 'j': chosen = nameOf(works[Math.min(i + 1, works.length - 1)]); stageIndex = null; break;
      case 'k': chosen = nameOf(works[Math.max(i - 1, 0)]); stageIndex = null; break;
      case 'l': stageIndex = Math.min(s + 1, work.stages.length - 1); outputIndex = 0; break;
      case 'h': stageIndex = Math.max(s - 1, 0); outputIndex = 0; break;
      case 'c': if (stage?.status === 'review') decide('approved'); break;
      case 'r': if (stage?.status === 'review') { event.preventDefault(); rerunning = true; } break;
      case 'R': if (runnable) run(); break;
    }
  }
</script>

<svelte:window {onkeydown} />

<AppShell crumb={work ? nameOf(work) : 'board'} status={output?.url ?? work?.url ?? ''}>
  {#snippet actions()}
    {#if runnable}<Button kbd="R" onclick={() => run()} disabled={busy}>{next?.stale ? `Rerun from ${next.name.split('_')[0]}` : 'Run'}</Button>{/if}
    <Button onclick={() => (creating = !creating)}>+ New work</Button>
  {/snippet}

  {#if error}<p class="notice err">{error}</p>{/if}
  {#if notice}<p class="notice">{notice}</p>{/if}

  {#if creating}
    <form class="new" onsubmit={(e) => { e.preventDefault(); create(); }}>
      <input bind:value={newName} placeholder="name (the work's folder)" />
      <input bind:value={newProject} placeholder={pipeline?.needs_project ? 'project (needed): a name or a path' : 'project (optional)'} />
      <textarea bind:value={newBrief} rows="3" placeholder="brief: what this piece of work is for"></textarea>
      <div class="row"><Button variant="primary" disabled={busy || !newName.trim()}>Create</Button></div>
    </form>
  {/if}

  <div class="head">
    <b>{pipeline?.title ?? site}</b>
    <span class="faint">pipeline · {works.length} work{works.length === 1 ? '' : 's'}</span>
  </div>

  {#if works.length === 0}
    <p class="empty">No works yet: + New work makes one.</p>
  {:else}
    <div class="works">
      {#each works as w (w.url)}
        <button class="w" class:on={w === work} onclick={() => { chosen = nameOf(w); stageIndex = null; outputIndex = 0; }}>
          <span class="dot {dot(w)}"></span>{nameOf(w)}<small>{w.status ?? ''}</small>
        </button>
      {/each}
    </div>
  {/if}

  {#if work}
    <div class="stages" style="--n: {Math.max(work.stages.length, 1)}">
      {#each work.stages as s, i (s.name)}
        {@const c = pipeline?.stages.find((x) => x.name === s.name)}
        <button class="st {tone(s.status)}" class:stale={s.stale} class:on={s === stage} onclick={() => { stageIndex = i; outputIndex = 0; }}>
          <span class="n">{s.name.split('_')[0]}</span>
          <span class="t"><b>{c?.title.toLowerCase() ?? s.name}</b><small>{s.status}{s.stale ? ' · stale' : ''}{c?.gate ? ' · gate' : ''}{c?.harness && c.harness !== 'composer' ? ` · ${c.harness}` : ''}</small></span>
        </button>
      {/each}
    </div>

    {#if stage}
      <section class="out">
        <div class="ohead">
          <Badge tone={tone(stage.status)}>{stage.status}</Badge>
          {#each stage.outputs as url, i (url)}
            <button class="file" class:on={i === outputIndex} onclick={() => { outputIndex = i; editing = false; diffing = false; }}>{short(url)}</button>
          {:else}
            <span class="faint">{stage.status === 'running' ? 'running…' : 'nothing written yet'}</span>
          {/each}
          <span class="grow"></span>
          {#if output && !editing}
            <Button variant="ghost" onclick={toggleDiff}>{diffing ? 'Output' : 'Your edits'}</Button>
            <Button variant="ghost" onclick={() => { draft = output!.raw; editing = true; diffing = false; }}>Edit</Button>
          {/if}
        </div>
        {#if stage.stale}
          <div class="stale-bar">
            <span>
              <b>Stale</b> ·
              {#if stage.changed.length}
                changed since it ran: {stage.changed.map(short).join(', ')}
              {:else}
                a stage before it is stale
              {/if}
            </span>
            <span class="grow"></span>
            {#if !work.stages.some((s) => s.status === 'running')}
              <Button onclick={() => run(stage!.name)} disabled={busy}>Rerun from here</Button>
            {/if}
          </div>
        {/if}
        {#if stage.summary}<p class="summary">{stage.summary}</p>{/if}
        {#if editing}
          <textarea class="editor" bind:value={draft}></textarea>
          <div class="row"><Button variant="ghost" onclick={() => (editing = false)}>Cancel</Button><Button variant="primary" onclick={save}>Save</Button></div>
        {:else if output && diffing}
          <pre class="doc">{#each edits as line}<span class="line {line.kind === '+' ? 'add' : line.kind === '-' ? 'del' : ''}">{line.text}
</span>{/each}</pre>
          {#if !edits.some((l) => l.kind !== ' ')}<p class="summary">No edits: this is what the agent wrote.</p>{/if}
        {:else if output}
          <pre class="doc">{body(output.raw)}</pre>
        {/if}
      </section>

      {#if stage.status === 'review'}
        <div class="gate">
          <span class="dot accent"></span>
          <span><b>Gate</b> · {contract?.title.toLowerCase() ?? stage.name} waits for you{work.stages[work.stages.indexOf(stage) + 1] ? `; the next stage reads this` : ''}</span>
          <span class="grow"></span>
          <Button kbd="r" onclick={() => (rerunning = !rerunning)} disabled={busy}>Rerun…</Button>
          <Button variant="danger" onclick={() => decide('stopped')} disabled={busy}>Stop</Button>
          <Button variant="primary" kbd="c" onclick={() => decide('approved')} disabled={busy}>Continue</Button>
        </div>
        {#if rerunning}
          <form class="note" onsubmit={(e) => { e.preventDefault(); decide('rerun', note); }}>
            <!-- svelte-ignore a11y_autofocus -->
            <input bind:value={note} autofocus placeholder="what should change? the stage gets this note with its inputs · enter reruns" />
          </form>
        {/if}
      {/if}
    {/if}
  {/if}

  {#snippet footer()}
    <Kbd keys="j/k" label="work" />
    <Kbd keys="h/l" label="stage" />
    <Kbd keys="c" label="continue" />
    <Kbd keys="r" label="rerun" />
    <Kbd keys="R" label="run" />
  {/snippet}
</AppShell>

<style>
  .head { display: flex; gap: 10px; align-items: baseline; padding: 14px 14px 8px; font-size: 14px; }
  .faint { color: var(--faint); font-size: 12px; }
  .grow { flex: 1; }
  .works { display: flex; flex-wrap: wrap; gap: 6px; padding: 0 14px 10px; }
  .w {
    display: inline-flex; align-items: center; gap: 7px; cursor: pointer; padding: 5px 10px;
    border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface);
    color: var(--text); font: 12.5px var(--font-mono);
  }
  .w small { color: var(--faint); font-size: 11px; }
  .w.on { border-color: var(--accent); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--faint); flex: none; }
  .dot.accent { background: var(--accent); }
  .dot.warn { background: var(--warn); }
  .dot.ok { background: var(--ok); }
  .stages { display: grid; grid-template-columns: repeat(var(--n), minmax(0, 1fr)); gap: 6px; padding: 10px 14px; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); }
  .st {
    display: flex; gap: 8px; align-items: flex-start; text-align: left; cursor: pointer; padding: 7px 9px;
    border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text);
  }
  .st .n { font: 600 11px var(--font-mono); color: var(--faint); padding-top: 1px; }
  .st .t { display: flex; flex-direction: column; font: 12.5px var(--font-mono); min-width: 0; }
  .st small { color: var(--faint); font-size: 10.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .st.ok { border-color: color-mix(in srgb, var(--ok) 45%, var(--border)); }
  .st.info { border-color: color-mix(in srgb, var(--accent) 60%, var(--border)); }
  .st.warn { border-color: color-mix(in srgb, var(--warn) 50%, var(--border)); }
  .st.err { border-color: color-mix(in srgb, var(--err) 50%, var(--border)); }
  .st.stale { border-style: dashed; border-color: color-mix(in srgb, var(--warn) 60%, var(--border)); }
  .stale-bar {
    display: flex; gap: 10px; align-items: center; margin-bottom: 8px; padding: 7px 10px; font-size: 12.5px; color: var(--muted);
    border: 1px solid color-mix(in srgb, var(--warn) 45%, var(--border)); border-radius: var(--radius);
    background: color-mix(in srgb, var(--warn) 6%, var(--surface));
  }
  .stale-bar b { color: var(--warn); font-weight: 500; }
  .st.on { outline: 1px solid var(--accent); background: color-mix(in srgb, var(--accent) 10%, var(--surface)); }
  .out { padding: 10px 14px; }
  .ohead { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-bottom: 8px; }
  .file { font: 12px var(--font-mono); color: var(--muted); background: none; border: 0; border-bottom: 1px solid transparent; cursor: pointer; padding: 2px 0; }
  .file.on { color: var(--text); border-bottom-color: var(--accent); }
  .summary { margin: 0 0 8px; font-size: 12.5px; color: var(--muted); }
  .doc { margin: 0; font: 13px/1.6 var(--font-ui); white-space: pre-wrap; color: var(--text); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); padding: 12px 14px; max-height: calc(100vh - 330px); overflow: auto; }
  .line.add { background: color-mix(in srgb, var(--ok) 14%, transparent); }
  .line.del { color: var(--faint); text-decoration: line-through; }
  .editor { width: 100%; min-height: 320px; font: 12.5px/1.55 var(--font-mono); background: var(--bg); color: var(--text); border: 1px solid var(--accent); border-radius: var(--radius); padding: 10px 12px; box-sizing: border-box; }
  .row { display: flex; gap: 8px; justify-content: flex-end; margin-top: 8px; }
  .gate {
    position: sticky; bottom: 0; z-index: 1;
    display: flex; gap: 10px; align-items: center; margin: 0 14px 14px; padding: 9px 12px; font-size: 13px;
    border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border)); border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 7%, var(--surface));
    box-shadow: 0 -8px 16px var(--bg);
  }
  .note, .new { padding: 0 14px 12px; display: flex; flex-direction: column; gap: 6px; }
  input, textarea { font: 13px var(--font-ui); padding: 7px 10px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--bg); color: var(--text); }
  .note input { border-color: var(--accent); }
  .notice { margin: 10px 14px 0; font-size: 12px; color: var(--muted); }
  .notice.err { color: var(--err); }
  .empty { margin: 8px 14px; font-size: 13px; color: var(--faint); }
</style>
