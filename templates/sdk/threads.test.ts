// `bun test` in templates/sdk (the thread page format; see threads.ts).
import { describe, expect, test } from 'bun:test';
import {
  agentMessage,
  appendMessage,
  formatThread,
  openCount,
  parseThread,
  setStatus,
  threadTime,
  threadUrl,
  type Thread,
} from './threads';

const anchor = { id: 'mockup/login-screen', label: 'mockup login-screen' };
const first = { author: 'you', at: '2026-11-11T14:02:00Z', text: 'The button is too small.\n\nOn a phone.' };

describe('thread pages', () => {
  test('a new page reads back as one open thread', () => {
    const raw = formatThread(anchor, first);
    expect(raw.startsWith('---\ntype: thread\n')).toBe(true);
    const thread = parseThread(raw, 'site://mockups/threads/x.md');
    expect(thread).toEqual({
      url: 'site://mockups/threads/x.md',
      anchor: 'mockup/login-screen',
      anchor_label: 'mockup login-screen',
      status: 'open',
      created: '2026-11-11T14:02:00Z',
      updated: '2026-11-11T14:02:00Z',
      messages: [first],
    });
  });

  test('appending keeps earlier messages and stamps updated', () => {
    const reply = { author: 'agent', at: '2026-11-11T14:05:00Z', text: 'Made it full width.' };
    const thread = parseThread(appendMessage(formatThread(anchor, first), reply));
    expect(thread.messages).toEqual([first, reply]);
    expect(thread.updated).toBe(reply.at);
    expect(thread.created).toBe(first.at);
    expect(appendMessage(formatThread(anchor, first), reply)).toContain('\nupdated: 2026-11-11T14:05:00Z\n');
  });

  test('an append the agent made with `edit` (no trailing newline, extra blank lines) parses', () => {
    const raw = formatThread(anchor, first).trimEnd() + '\n\n\n### claude · 2026-11-11T15:00:00Z\nDone.';
    const thread = parseThread(raw);
    expect(thread.messages.map((m) => [m.author, m.text])).toEqual([
      ['you', first.text],
      ['claude', 'Done.'],
    ]);
  });

  test('resolve and reopen change only status and updated', () => {
    const raw = formatThread(anchor, first);
    const resolved = setStatus(raw, 'resolved', '2026-11-11T16:00:00Z');
    expect(parseThread(resolved).status).toBe('resolved');
    expect(parseThread(resolved).updated).toBe('2026-11-11T16:00:00Z');
    expect(parseThread(resolved).messages).toEqual([first]);
    expect(resolved.split('\n').length).toBe(raw.split('\n').length);
    expect(parseThread(setStatus(resolved, 'open')).status).toBe('open');
  });

  test('the agent setting the field by hand, with a comment, counts', () => {
    const raw = formatThread(anchor, first).replace('status: open', 'status: resolved   # fixed');
    expect(parseThread(raw).status).toBe('resolved');
  });

  test('labels with YAML-special characters survive', () => {
    const odd = { id: 'mockup/a/region/12.5,30,40,10', label: 'mockup a: region "top" #1' };
    const thread = parseThread(formatThread(odd, first));
    expect(thread.anchor).toBe(odd.id);
    expect(thread.anchor_label).toBe(odd.label);
  });

  test('text before the first header is ignored; a page without frontmatter has defaults', () => {
    const thread = parseThread('# notes\n\n### you · 2026-11-11T14:02:00Z\n\nhi\n');
    expect(thread.status).toBe('open');
    expect(thread.messages).toEqual([{ author: 'you', at: '2026-11-11T14:02:00Z', text: 'hi' }]);
  });
});

describe('helpers', () => {
  test('open threads count on an anchor and its sub-anchors', () => {
    const t = (a: string, status: Thread['status'] = 'open') => ({ anchor: a, status }) as Thread;
    const threads = [t('mockup/a'), t('mockup/a/region/1,2,3,4'), t('mockup/a', 'resolved'), t('mockup/ab')];
    expect(openCount(threads, 'mockup/a')).toBe(2);
    expect(openCount(threads, 'mockup/a/region/1,2,3,4')).toBe(1);
    expect(openCount(threads, 'mockup/ab')).toBe(1);
  });

  test('thread urls and times', () => {
    expect(threadTime(new Date('2026-11-11T14:02:00.123Z'))).toBe('2026-11-11T14:02:00Z');
    expect(threadUrl('site://mockups/', anchor, '2026-11-11T14:02:00Z', 0)).toBe(
      'site://mockups/threads/2026-11-11-mockup-login-screen-0000.md',
    );
  });

  test('the agent message names the thread and the anchor', () => {
    const text = agentMessage({ url: 'site://mockups/threads/t.md', anchor_label: 'mockup a' }, 'Bigger\nplease');
    expect(text).toBe('Comment on mockup a (thread site://mockups/threads/t.md): Bigger please');
  });
});
