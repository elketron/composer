---
title: Memory
type: index
---
# Memory

What you know that outlives a conversation. **Read the relevant pages before starting work; write to memory as soon as you learn something worth keeping** — don't wait for the end of the session.

## Layout

| path | holds |
|------|-------|
| `user.md` | who the user is: role, preferences, how they like to work |
| `projects/<name>.md` | one page per project: what it is, where it lives, decisions, current state |
| `topics/<name>.md` | knowledge not tied to one project (tools, libraries, techniques) |
| `feedback/<name>.md` | corrections and confirmed approaches from the user, with why |

Keep this index's [pages](#pages) list up to date when you add or remove a page.

## Page format

```markdown
---
title: composer
type: memory
kind: project          # user | project | topic | feedback
tags: [rust, tui]
path: ~/Projects/composer   # projects only
updated: 2026-10-18
---
# composer

Short summary first. Then facts, one per bullet or short section.
Link related pages: [plan](site://plans/composer).
```

## Rules

- **Update, don't duplicate.** Search first (`site:memory <topic>`); extend the existing page if there is one.
- **Facts, not transcripts.** Write what is true and useful later, not what was said.
- **Feedback carries a reason.** Write the rule, then **Why:** and **How to apply:**.
- **Absolute dates.** "next week" becomes `2026-10-25`.
- **Remove what is wrong.** If a memory turns out false, fix or delete it.
- Don't store what is already in the project's code or git history.
- When the context fills up, you are asked to save memory before older messages are summarized. Save then what the summary would lose: decisions with their reasons, preferences, and where things are.

## Pages

- [user](user.md)
