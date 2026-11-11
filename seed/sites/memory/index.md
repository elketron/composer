---
title: Memory
type: index
tools: []
---
# Memory

What you know that outlives a conversation. **Read the relevant pages before starting work; write to memory as soon as you learn something worth keeping** — don't wait for the end of the session.

## Layout

| path | holds |
|------|-------|
| `user.md` | who the user is: role, preferences, how they like to work |
| `topics/<name>.md` | knowledge not tied to one project (tools, libraries, techniques) |
| `feedback/<name>.md` | corrections and confirmed approaches from the user, with why |

Projects are not here: a project's page is its hub, at `site://projects/<name>` ([projects](site://projects)). Link it; don't duplicate its facts. The old `projects/<name>.md` pages still read as a fallback, but write new project knowledge to the project page and keep only what outlives the project here.

Keep this index's [pages](#pages) list up to date when you add or remove a page.

## Page format

```markdown
---
title: Rust async
type: memory
kind: topic          # user | topic | feedback
tags: [rust, async]
updated: 2026-10-18
---
# Rust async

Short summary first. Then facts, one per bullet or short section.
Link related pages: [plan](site://plans/composer).
```

## Rules

- **Update, don't duplicate.** Search first (`site:memory <topic>`); extend the existing page if there is one.
- **Facts, not transcripts.** Write what is true and useful later, not what was said.
- **Feedback carries a reason.** Write the rule, then **Why:** and **How to apply:**.
- **Absolute dates.** "next week" becomes `2026-10-25`.
- **Remove what is wrong.** If a memory turns out false, fix or delete it.
- Don't store what is already in the project's page, its code or its git history.
- When the context fills up, you are asked to save memory before older messages are summarized. Save then what the summary would lose: decisions with their reasons, preferences, and where things are.

## Pages

- [user](user.md)
