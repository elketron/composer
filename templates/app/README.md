# __SITE__ app

The human-facing app of `site://__SITE__`, built with Svelte and the shared composer sdk (`$sdk`).

- Build: the site's `ui-build` action (`just ui-build`), output in `dist/`.
- Served by the composer server, on Home or on a project, at `/w/<workspace>/a/<app token>/` with
  a token of its own (it reads any page, writes only this site and the project, runs only this
  site's actions). The page is sandboxed (an opaque origin): no cookies, no `localStorage` or
  `sessionStorage` (they throw), and keep URLs relative (the sdk does). Its `app:` block in
  `site://__SITE__/index.md` says where it shows (`scope`, `when`).
- On a project: `project()` gives `{ name, root, branch, pipeline, tags }`; `data(path)` /
  `putData(path, body)` keep the app's work data for that project (in the workspace, not the
  project); its recipes get `PROJECT_ROOT`, `PROJECT_NAME` and `APP_DATA` (that same folder).
- State the agent should see belongs in files of the site (`putPage` / `putFile`), not in the app.
- `send(text, { pages, images })` puts a message in the chat of the session that opened the app.
