# __SITE__ app

The human-facing app of `site://__SITE__`, built with Svelte and the shared composer sdk (`$sdk`).

- Build: the site's `ui-build` action (`just ui-build`), output in `dist/`.
- Served by the composer server at `/__SITE__/`; open it from the TUI with `a`.
- State the agent should see belongs in files of the site (`putPage` / `putFile`), not in the app.
- `send(text, { pages, images })` puts a message in the chat of the session that opened the app.
