// The composer server, as seen from a micro app.
//
// An app is served by the server at <workspace>/a/<app token>/, sandboxed: its page has an opaque
// origin (no cookies, no storage, `Origin: null`), and its token is in the path, so the API under
// it (`api/...`, `ws`) is authenticated by the URL itself. The server tells the page its site,
// project and session in `<meta name="composer-app">`. A client app (the chat) is served at
// <workspace>/<site>/ instead, with the workspace's cookie.
// Types mirror crates/protocol (generate them with `cargo test -p composer-protocol --features ts`).

export interface Page {
  url: string;
  title: string;
  frontmatter: Record<string, unknown>;
  frontmatter_error: string | null;
  raw: string;
  backlinks: string[];
  hash: string;
}

export interface SearchHit {
  url: string;
  title: string;
  site: string;
}

export interface PageChange {
  url: string;
  kind: 'created' | 'modified' | 'deleted';
  by: 'agent' | 'app' | 'external';
}

const params = new URLSearchParams(location.search);

/** This sdk's version, stamped into every app built with it (the server tells an older build
 * apart: one before 3 doesn't know the sandboxed `/a/<app token>/` routes). */
export const SDK_VERSION = 3;
document.documentElement.dataset.composerSdk = `composer-sdk:${SDK_VERSION}`;

const segments = location.pathname.split('/').filter(Boolean);
const inWorkspace = segments[0] === 'w' && segments.length >= 2;
/** Where the workspace's own segments end: after `/w/<workspace>`, or at the start. */
const at = inWorkspace ? 2 : 0;
/** Opened with a token of its own (on a project or on Home): `<workspace>/a/<app token>/`. */
const onToken = segments[at] === 'a' && segments.length >= at + 2;
const workspaceBase = inWorkspace ? `/w/${segments[1]}` : '';

/** What the server says about this app page (only under `/a/`). */
const facts: { site?: string; project?: string | null; session?: string | null } = (() => {
  if (!onToken) return {};
  try {
    return JSON.parse(document.querySelector('meta[name="composer-app"]')?.getAttribute('content') ?? '{}') ?? {};
  } catch {
    return {};
  }
})();

/** This app's workspace (`default`, ...), or `null` when served without one. */
export const workspace: string | null = inWorkspace ? segments[1] : null;

/** The name of the project the app was opened on, or `null` on Home. */
export const projectName: string | null = facts.project ?? null;

/** This app's site: from the server under `/a/`; a client app's is its path
 * (`/w/default/chat/` → `chat`). */
export const site: string = (onToken ? facts.site : segments[at]) ?? '';

/** Where this app's API lives: under its token (`/w/<workspace>/a/<app token>`), or for a client
 * app the workspace's (`/w/<workspace>`). */
export const base: string = onToken ? `${workspaceBase}/a/${segments[at + 1]}` : workspaceBase;

/** The session that opened the app; messages from `send` go there. (No storage: a sandboxed page
 * has none; the server says it on every load.) */
export const session: string | null = facts.session ?? params.get('session');

// Drop a client app's token or one-time code from the address bar once the cookie is set.
if (params.has('token') || params.has('app_token') || params.has('once')) {
  params.delete('token');
  params.delete('app_token');
  params.delete('once');
  const query = params.toString();
  history.replaceState(null, '', location.pathname + (query ? `?${query}` : '') + location.hash);
}

/** `site://design/x.md` → `design/x.md` */
function apiPath(url: string): string {
  return url.replace(/^site:\/\//, '').split('#')[0].replace(/\/$/, '');
}

async function json<T>(response: Response): Promise<T> {
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(body.error ?? `${response.status} ${response.statusText}`);
  }
  return response.json();
}

export interface Project {
  /** Its name in the workspace (and in this app's URL). */
  name: string;
  /** Its project page (`site://projects/<name>`), when it has one. */
  page?: string;
  /** The absolute folder. */
  root: string;
  /** The branch checked out (git projects). */
  branch?: string;
  /** The first site with a work linking it; `sites` has them all. */
  pipeline?: string;
  /** The old name for `sites`. */
  pipelines: string[];
  /** The sites operating on it. */
  sites: string[];
  tags: string[];
}

/** The project this app was opened on, or `null` on Home (no project open). */
export async function project(): Promise<Project | null> {
  if (!projectName) return null;
  return json(await fetch(`${base}/api/project`));
}

/** Reads a file of the project. With `work`, the work's `project/` view: the worktree while a
 * code stage runs, the project root otherwise. */
export async function projectFile(path: string, work?: string): Promise<Response> {
  const query = work ? `?work=${encodeURIComponent(work)}` : '';
  const response = await fetch(`${base}/api/project/files/${path}${query}`);
  if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
  return response;
}

/** Writes a file of the project (or the work's `project/` view). A `read` work refuses. */
export async function putProjectFile(
  path: string,
  body: Blob | string,
  work?: string,
): Promise<void> {
  const query = work ? `?work=${encodeURIComponent(work)}` : '';
  const response = await fetch(`${base}/api/project/files/${path}${query}`, { method: 'PUT', body });
  if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
}

/** This app's work data on its project (renders, view models, JSON): kept in the workspace, keyed
 * by project, not in the project. Its recipes see the same folder as `APP_DATA`. */
export async function data(path: string): Promise<Response> {
  if (!onToken) throw new Error('data() needs the app opened on a project or on Home');
  const response = await fetch(`${base}/api/data/${path}`);
  if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
  return response;
}

/** Writes this app's work data on its project (see `data`). */
export async function putData(path: string, body: Blob | string): Promise<void> {
  if (!onToken) throw new Error('putData() needs the app opened on a project or on Home');
  const response = await fetch(`${base}/api/data/${path}`, { method: 'PUT', body });
  if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
}

/** What the user points at in this app, for the agent (ticket 0175): it shows as a chip in the
 * chat and goes with the next message; `null` clears it. Only for an app opened from a chat. */
export async function select(
  selection: { label: string; text?: string; url?: string; data?: unknown } | null,
): Promise<void> {
  const response = await fetch(`${base}/api/selection`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(selection ?? { label: null }),
  });
  if (!response.ok && response.status !== 403) throw new Error(`${response.status} ${await response.text()}`);
}

/** Something happened in this app that the agent should know (a gate flipped, approve clicked):
 * it sees it at its next turn; it doesn't start one. At most 20 a minute. */
export async function emit(kind: string, detail: unknown = {}): Promise<void> {
  const response = await fetch(`${base}/api/events`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ kind, detail }),
  });
  if (!response.ok && response.status !== 403) throw new Error(`${response.status} ${await response.text()}`);
}

/** Reads a markdown page (frontmatter parsed). */
export async function page(url: string): Promise<Page> {
  return json(await fetch(`${base}/api/pages/${apiPath(url)}`));
}

/** Writes a page. With `ifMatch` (a page's `hash`), fails if it changed meanwhile. */
export async function putPage(url: string, content: string, ifMatch?: string): Promise<{ url: string; hash: string }> {
  return json(
    await fetch(`${base}/api/pages/${apiPath(url)}`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ content, if_match: ifMatch ?? null }),
    }),
  );
}

/** Reads any file of a site (JSON, images, ...). */
export async function file(url: string): Promise<Response> {
  const response = await fetch(`${base}/api/files/${apiPath(url)}`);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return response;
}

/** Writes any file of a site, e.g. `mockup.json` or a `snapshot.png` blob. */
export async function putFile(url: string, body: Blob | string): Promise<void> {
  const response = await fetch(`${base}/api/files/${apiPath(url)}`, { method: 'PUT', body });
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
}

// ---- site actions: the justfile recipes the server will run ----

/** Mirrors crates/protocol (`ActionSummary`): one public recipe of a site's justfile. */
export interface Action {
  name: string;
  doc: string | null;
  /** `name`, `name=default` or `*name`, as in the agent `actions` tool. */
  parameters: string[];
  /** Whether this version runs unasked; otherwise the session's clients are asked. */
  approved: boolean;
}

/** The result of `runAction`; mirrors crates/protocol (`RunActionResponse`). */
export interface ActionResult {
  status: 'ok' | 'failed' | 'timed out' | 'killed';
  exit_code: number | null;
  output: string;
}

/** A site's runnable actions (its justfile's public recipes). Defaults to this app's site. */
export async function listActions(siteName: string = site): Promise<Action[]> {
  return json(await fetch(`${base}/api/actions/${siteName}`));
}

/**
 * Runs one of a site's actions through the server, e.g. `await runAction('ui-build')`.
 * A recipe version not yet approved asks the session that opened this app (or `options.session`)
 * and waits; if the answer is no, this throws. Pass the args in the order the recipe declares.
 */
export async function runAction(
  recipe: string,
  args: string[] = [],
  options: { site?: string; session?: string } = {},
): Promise<ActionResult> {
  return json(
    await fetch(`${base}/api/actions/${options.site ?? site}/${recipe}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ args, session: options.session ?? session }),
    }),
  );
}

/** Full text and frontmatter search: `type:ticket status:open`, `site:design words`. */
export async function search(query: string): Promise<SearchHit[]> {
  return json(await fetch(`${base}/api/search?q=${encodeURIComponent(query)}`));
}

/**
 * Sends a message to the agent in the session that opened this app. `pages` are
 * URLs it should look at; `images` are site URLs of images it should see.
 */
export async function send(text: string, options: { pages?: string[]; images?: string[] } = {}): Promise<void> {
  if (!session) throw new Error('no session: open this app from the TUI (a)');
  const response = await fetch(`${base}/api/sessions/${session}/messages`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ site, text, pages: options.pages ?? [], images: options.images ?? [] }),
  });
  if (!response.ok) throw new Error(`send failed: ${response.status}`);
}

// ---- live page changes: one shared socket for the whole app ----

type Watcher = { patterns: string[]; callback: (change: PageChange) => void };
const watchers = new Set<Watcher>();
let socket: WebSocket | null = null;
// Requests waiting for their `ok` / `error` reply, by id.
const waiting = new Map<number, { resolve: (result: any) => void; reject: (error: Error) => void }>();
let nextId = 100;
const listeners = new Set<(message: any) => void>();
let ready: Promise<void>;
let markReady: () => void = () => {};
const resetReady = () => (ready = new Promise((resolve) => (markReady = resolve)));
resetReady();
// A rune: components reading isConnected() update when it changes.
let connected = $state(false);

/** Whether the live connection is up (for a status dot). */
export function isConnected(): boolean {
  return connected;
}

function connect() {
  const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}${base}/ws`);
  socket = ws;
  ws.onopen = () => {
    connected = true;
    markReady();
    const urls = [...watchers].flatMap((w) => w.patterns);
    ws.send(JSON.stringify({ id: 1, type: 'subscribe', urls }));
  };
  ws.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if ((message.type === 'ok' || message.type === 'error') && waiting.has(message.id)) {
      const { resolve, reject } = waiting.get(message.id)!;
      waiting.delete(message.id);
      if (message.type === 'ok') resolve(message.result);
      else reject(new Error(message.message));
      return;
    }
    for (const listener of listeners) listener(message);
    if (message.type !== 'page.changed') return;
    for (const watcher of watchers) {
      if (watcher.patterns.some((p) => matches(p, message.url))) watcher.callback(message);
    }
  };
  ws.onclose = () => {
    connected = false;
    resetReady();
    for (const { reject } of waiting.values()) reject(new Error('connection lost'));
    waiting.clear();
    socket = null;
    setTimeout(connect, 2000);
  };
}

/** Calls `callback` when a matching page changes. Patterns: `site://x/**`, `site://x/*`, exact. Returns an unsubscribe function. */
export function onChange(patterns: string[], callback: (change: PageChange) => void): () => void {
  const watcher = { patterns, callback };
  watchers.add(watcher);
  if (socket?.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify({ id: 1, type: 'subscribe', urls: [...watchers].flatMap((w) => w.patterns) }));
  }
  return () => watchers.delete(watcher);
}

function matches(pattern: string, url: string): boolean {
  if (pattern === '*' || pattern === '**') return true;
  if (pattern.endsWith('/**')) return url === pattern.slice(0, -3) || url.startsWith(pattern.slice(0, -2));
  if (pattern.endsWith('/*')) {
    const rest = url.slice(pattern.length - 1);
    return url.startsWith(pattern.slice(0, -1)) && rest.length > 0 && !rest.includes('/');
  }
  return pattern === url;
}

/**
 * Sends a command over the WebSocket API (`session.list`, `session.attach`, `message.send`,
 * `approval.respond`, ...; see crates/protocol) and resolves with its result.
 */
export async function request(command: Record<string, unknown>): Promise<any> {
  await ready;
  const id = nextId++;
  return new Promise((resolve, reject) => {
    waiting.set(id, { resolve, reject });
    socket!.send(JSON.stringify({ id, ...command }));
  });
}

/** Every server event (session events of attached sessions, approvals, slots, ...). */
export function onMessage(callback: (message: any) => void): () => void {
  listeners.add(callback);
  return () => listeners.delete(callback);
}

connect();
