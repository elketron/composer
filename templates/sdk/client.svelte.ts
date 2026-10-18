// The composer server, as seen from a micro app.
//
// The app is served by the server at /<site>/, so every request is same-origin and
// authenticated by the cookie set when the app was opened (?token=...).
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

/** This app's site: the first path segment (`/design/` → `design`). */
export const site: string = location.pathname.split('/').filter(Boolean)[0] ?? '';

/** The session that opened the app; messages from `send` go there. Survives reloads. */
export const session: string | null = (() => {
  const key = `composer:${site}:session`;
  const fromUrl = params.get('session');
  try {
    if (fromUrl) sessionStorage.setItem(key, fromUrl);
    return fromUrl ?? sessionStorage.getItem(key);
  } catch {
    return fromUrl;
  }
})();

// Drop the token from the address bar once the cookie is set.
if (params.has('token')) {
  params.delete('token');
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

/** Reads a markdown page (frontmatter parsed). */
export async function page(url: string): Promise<Page> {
  return json(await fetch(`/api/pages/${apiPath(url)}`));
}

/** Writes a page. With `ifMatch` (a page's `hash`), fails if it changed meanwhile. */
export async function putPage(url: string, content: string, ifMatch?: string): Promise<{ url: string; hash: string }> {
  return json(
    await fetch(`/api/pages/${apiPath(url)}`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ content, if_match: ifMatch ?? null }),
    }),
  );
}

/** Reads any file of a site (JSON, images, ...). */
export async function file(url: string): Promise<Response> {
  const response = await fetch(`/api/files/${apiPath(url)}`);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return response;
}

/** Writes any file of a site, e.g. `mockup.json` or a `snapshot.png` blob. */
export async function putFile(url: string, body: Blob | string): Promise<void> {
  const response = await fetch(`/api/files/${apiPath(url)}`, { method: 'PUT', body });
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
}

/** Full text and frontmatter search: `type:ticket status:open`, `site:design words`. */
export async function search(query: string): Promise<SearchHit[]> {
  return json(await fetch(`/api/search?q=${encodeURIComponent(query)}`));
}

/**
 * Sends a message to the agent in the session that opened this app. `pages` are
 * URLs it should look at; `images` are site URLs of images it should see.
 */
export async function send(text: string, options: { pages?: string[]; images?: string[] } = {}): Promise<void> {
  if (!session) throw new Error('no session: open this app from the TUI (a)');
  const response = await fetch(`/api/sessions/${session}/messages`, {
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
  const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`);
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
