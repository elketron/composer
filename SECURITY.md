# Security

composer runs agents that read and write files and, after your approval, run commands as your user.
Read this before exposing a server beyond your own machine.

## Reporting a vulnerability

Please don't open a public issue. Use **Report a vulnerability** on the repository's Security tab
(GitHub private vulnerability reporting).

## Tokens

- **The server binds to `127.0.0.1:7420`** by default. Every API call and WebSocket needs a token.
  To reach it from other devices, put it behind something that authenticates the network, such as
  `tailscale serve`; don't bind it to a public interface.
- **The local token** is in `~/.composer/state/token`. The TUI, the desktop and `composer mcp` on
  the same machine read it from there.
- **Device tokens** come from pairing: `composer pair` makes a one-time code that expires after 10
  minutes, and the device trades it for its own token. Only the token's hash is stored
  (`state/devices.json`). `composer devices revoke <name>` ends it.
- **A webhook token** (`[hooks.<name>]`, `token` or `token_file`) opens only
  `POST /api/hooks/<name>`; device and local tokens don't open it, and it opens nothing else. It
  starts a run of the kind, site, folder and harness the hook names, with the request body as the
  input. The caller can't choose them, but it chooses the input, so treat the body as untrusted:
  an `agentic` hook's run has every tool. A run that is not stored can't ask for approvals: the
  first approval it needs stops it with an error. A stored run (`store = true`) asks in your
  clients like any session.

## Approvals

What runs outside composer's own page tools waits for your answer in a client:

- **Recipes.** A site's `justfile` recipe runs after you approve it, and again after it changes
  (approved versions in `state/approvals.json`). Approving a recipe approves what it runs,
  including code it builds.
- **Terminal commands and background processes.** `terminal_send` and `process_start` ask for each
  command. "Always" stores a `Bash(<command>:*)` rule for the site's folder
  (`state/tool_rules.json`); it covers that command with more arguments, never one that chains,
  substitutes or redirects (`;`, `&&`, `|`, `$`, `` ` ``, `>`, ...).
- **MCP tools.** Servers in `config.toml` are in every session. A call to a tool outside the
  server's `allow` list asks you; "always" remembers that tool.
- **A site's `mcp.toml`.** Nothing in it starts until you approve the file's exact bytes. The
  approval is kept by the file's hash, so any edit asks again.
- **A site's `folder:`.** The folder a site names counts only once you approve that folder for
  that site ("Let <site> work in <path>"), asked when a session sets the site or starts in it.
  The approval is kept by a hash of the resolved absolute folder (in `state/approvals.json`), so
  `folder:` naming another folder asks again, and the old folder stops counting at once. Until
  then the site works as if it named no folder: no terminals, background processes or outside
  agents start there, recipes get an empty `COMPOSER_FOLDER`, there is no changes view, and no mount
  may point into it. A run that is not stored can't ask, so it only uses an earlier approval.
- **Outside agents (ACP).** An outside agent asks composer through `session/request_permission`;
  composer shows that as an approval and passes your answer back. What the agent asks about, and
  what it does without asking, is decided by the agent and its own settings, not by composer.

## What composer does not protect

- **Nothing composer starts is sandboxed.** Recipes, terminals, background processes, MCP servers
  and outside agents run as your user, with your `$HOME`, your logins and the network. A run you
  approve can do whatever your user can.
- **An approved folder is open to the page tools.** The page tools (`open`, `search`, `write`,
  `edit`) work in `~/.composer/sites` and never reach `~/.composer/state`. Once you approve a
  site's `folder:`, `open` may read image files there and a symlink inside a site may point into
  it, so `write` and `edit` follow such a symlink there. "Always" rules for terminal commands are
  kept per folder and apply through any site whose approved folder it is.
- **Prompt injection is not solved.** A page, a web result, an attachment or a webhook body can
  steer an agent. Approvals are the boundary: read what you approve.
