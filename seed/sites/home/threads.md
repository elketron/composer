---
title: Threads
type: guide
---
# Threads

Apps built on the comment shell (the sdk's `Threads`) let the user point at something in the view and
comment on it. Each thread is a page in a `threads/` folder of the app's site (or of the work folder the
view is opened on), so you read them with `open` and `search` (`search type:thread status:open site:<site>`)
and answer them with `edit`. The site's index says what its anchors mean.

```markdown
---
type: thread
title: "mockup login: the button is too small"
anchor: mockup/login
anchor_label: mockup login
status: open
created: 2026-11-11T14:02:00Z
updated: 2026-11-11T14:02:00Z
---
### you · 2026-11-11T14:02:00Z

The button is too small on a phone.
```

- `anchor` is the view's id of the thing (`a/b` is a part of `a`); `anchor_label` names it for people.
- Each message is a section starting with `### <author> · <time>`: `you` is the user, the time is UTC
  (`2026-11-11T14:05:00Z`). Don't start a line of your own text with `### ` followed by ` · ` and a time.
- When the user sends a comment to you, the message reads `Comment on <anchor_label> (thread <url>):
  <text>`, from the app's site, with the thread page attached.
- **Reply** by appending a section at the end, with `edit` (or `write` with the whole page), signed with your
  agent name, and set `updated:` to the same time:

  ```markdown
  ### default · 2026-11-11T14:05:00Z

  Made it full width below 600px.
  ```

- **Resolve** by setting `status: resolved`, only for what your change addressed; otherwise reply and
  leave it open. Never resolve a thread you only read. The user reopens a thread by replying to it.
- The app shows replies and status changes live. Don't create, rename or delete thread pages unless asked.
