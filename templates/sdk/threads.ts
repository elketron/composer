// Comment threads, the shared view's shell (docs/workflows.md, "Threads"): one markdown page per
// thread in a `threads/` folder beside the work. Pure functions only (parsing and editing a thread
// page's text); `components/Threads.svelte` reads and writes them through the server.
//
// A thread page:
//
//   ---
//   type: thread
//   title: "mockup login-screen: the button is too small"
//   anchor: "mockup/login-screen"
//   anchor_label: "mockup login-screen"
//   status: open
//   created: 2026-11-11T14:02:00Z
//   updated: 2026-11-11T14:05:00Z
//   ---
//   ### you · 2026-11-11T14:02:00Z
//
//   The button is too small on a phone.
//
//   ### agent · 2026-11-11T14:05:00Z
//
//   Made it full width below 600px.
//
// A message starts at a line `### <author> · <time>` (time in ISO 8601, UTC) and runs to the next
// such line. The agent replies by appending a section and resolves with `status: resolved`.

export type ThreadStatus = 'open' | 'resolved';

export interface ThreadMessage {
  /** `you` (the user), or the agent's name. */
  author: string;
  /** ISO 8601, UTC, seconds (`2026-11-11T14:02:00Z`). */
  at: string;
  text: string;
}

export interface Thread {
  /** The thread page (`site://<site>/threads/<name>.md`), when known. */
  url: string;
  /** The view's element id (`data-anchor`). */
  anchor: string;
  /** A human label for it (`data-anchor-label`). */
  anchor_label: string;
  status: ThreadStatus;
  created: string;
  updated: string;
  messages: ThreadMessage[];
}

/** An element the view named: `data-anchor` and `data-anchor-label`. */
export interface Anchor {
  id: string;
  label: string;
}

const HEADER = /^### (.+?) · (\d{4}-\d\d-\d\dT[\d:.]+Z?)\s*$/;

/** Now, as a thread time: ISO 8601 in UTC without milliseconds. */
export function threadTime(date: Date = new Date()): string {
  return date.toISOString().replace(/\.\d+Z$/, 'Z');
}

/** A YAML scalar the parser below (and any YAML reader) reads back as the same string. */
function scalar(value: string): string {
  return /^[\w./@+-][\w ./@+,%:-]*$/.test(value) &&
    !/:( |$)/.test(value) &&
    !/^(true|false|null|yes|no|on|off|~)$/i.test(value)
    ? value
    : JSON.stringify(value);
}

function unquote(value: string): string {
  const v = value.trim();
  if (v.startsWith('"')) {
    try {
      return JSON.parse(v);
    } catch {
      return v.slice(1, -1);
    }
  }
  if (v.startsWith("'") && v.endsWith("'")) return v.slice(1, -1).replace(/''/g, "'");
  return v;
}

/** Splits a page into its frontmatter lines and its body. */
function split(raw: string): { front: string[]; body: string } {
  const text = raw.replace(/\r\n/g, '\n');
  const match = text.match(/^---\n([\s\S]*?)\n---\n?/);
  if (!match) return { front: [], body: text };
  return { front: match[1].split('\n'), body: text.slice(match[0].length) };
}

function join(front: string[], body: string): string {
  return `---\n${front.join('\n')}\n---\n${body.replace(/^\n*/, '')}`;
}

/** Reads a thread page. Unknown or missing fields get defaults; text before the first message
 * header is ignored. */
export function parseThread(raw: string, url = ''): Thread {
  const { front, body } = split(raw);
  const fields: Record<string, string> = {};
  for (const line of front) {
    const m = line.match(/^([\w-]+):\s*(.*)$/);
    // A YAML comment (`status: resolved  # fixed`) ends an unquoted value.
    if (m) fields[m[1]] = /^["']/.test(m[2]) ? unquote(m[2]) : m[2].replace(/\s+#.*$/, '').trim();
  }
  const messages: ThreadMessage[] = [];
  let current: ThreadMessage | null = null;
  const lines: string[] = [];
  const flush = () => {
    if (current) messages.push({ ...current, text: lines.join('\n').trim() });
    lines.length = 0;
  };
  for (const line of body.split('\n')) {
    const header = line.match(HEADER);
    if (header) {
      flush();
      current = { author: header[1].trim(), at: header[2], text: '' };
    } else if (current) {
      lines.push(line);
    }
  }
  flush();
  return {
    url,
    anchor: fields.anchor ?? '',
    anchor_label: fields.anchor_label ?? fields.anchor ?? '',
    status: fields.status === 'resolved' ? 'resolved' : 'open',
    created: fields.created ?? messages[0]?.at ?? '',
    updated: fields.updated ?? messages.at(-1)?.at ?? fields.created ?? '',
    messages,
  };
}

/** One message section, as appended to a thread page. */
export function formatMessage(message: ThreadMessage): string {
  return `### ${message.author} · ${message.at}\n\n${message.text.trim()}\n`;
}

/** A new thread page with its first message. */
export function formatThread(anchor: Anchor, message: ThreadMessage): string {
  const front = [
    'type: thread',
    `title: ${JSON.stringify(`${anchor.label}: ${firstLine(message.text, 60)}`)}`,
    `anchor: ${scalar(anchor.id)}`,
    `anchor_label: ${scalar(anchor.label)}`,
    'status: open',
    `created: ${message.at}`,
    `updated: ${message.at}`,
  ];
  return join(front, formatMessage(message));
}

/** Sets (or adds) one frontmatter field, leaving the rest of the page as it is. */
export function setField(raw: string, key: string, value: string): string {
  const { front, body } = split(raw);
  const line = `${key}: ${scalar(value)}`;
  const at = front.findIndex((l) => l.startsWith(`${key}:`));
  if (at >= 0) front[at] = line;
  else front.push(line);
  return join(front, body);
}

/** Appends a message to a thread page and stamps `updated`. */
export function appendMessage(raw: string, message: ThreadMessage): string {
  const page = setField(raw, 'updated', message.at);
  return `${page.replace(/\s*$/, '')}\n\n${formatMessage(message)}`;
}

/** Resolves or reopens a thread page (stamping `updated`). */
export function setStatus(raw: string, status: ThreadStatus, at: string = threadTime()): string {
  return setField(setField(raw, 'status', status), 'updated', at);
}

/** Open threads on an anchor, counting its sub-anchors (`mockup/a` counts `mockup/a/region/...`). */
export function openCount(threads: Thread[], anchor: string): number {
  return threads.filter((t) => t.status === 'open' && (t.anchor === anchor || t.anchor.startsWith(`${anchor}/`)))
    .length;
}

/** A new thread page's URL under `root` (a site or work folder, `site://mockups`):
 * `<root>/threads/<date>-<label slug>-<4 random>.md`. */
export function threadUrl(root: string, anchor: Anchor, at: string, random = Math.random()): string {
  const slug = anchor.label.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 40) || 'thread';
  const tag = Math.floor(random * 36 ** 4).toString(36).padStart(4, '0');
  return `${root.replace(/\/$/, '')}/threads/${at.slice(0, 10)}-${slug}-${tag}.md`;
}

function firstLine(text: string, max: number): string {
  const line = text.trim().split('\n')[0];
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}

/** The chat message `send` posts for a comment: it names the thread page and the anchor. */
export function agentMessage(thread: Pick<Thread, 'url' | 'anchor_label'>, text: string): string {
  return `Comment on ${thread.anchor_label} (thread ${thread.url}): ${firstLine(text.replace(/\s+/g, ' '), 280)}`;
}
