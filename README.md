# composer

One home for your workflows and memory. The home is a web of markdown **sites**: folders of
pages that agents (and you) open, search and edit. Workflows are folders inside the sites, worked
the [ICM](docs/icm.md) way. Agents run on your own model server.

Every workflow runs in one of three kinds:

| kind | what it may do | what comes back |
|------|----------------|-----------------|
| **single shot** | nothing: the workflow's instructions and the input go to the model once | the model's answer |
| **workflow run** | reads the web, changes nothing, may take several steps | one value (`finish`) |
| **agentic** | reads and writes, runs recipes, terminals and MCP tools | a session you follow |

Composer's own agent loop runs all three. An agentic session can also run on an outside agent
over the Agent Client Protocol (ACP); composer hands that agent itself as its MCP server.

## What it does

- **Sites.** A site's `index.md` is its homepage and its instructions. It can bring actions (a
  `justfile`, each recipe run after your approval), MCP servers (`mcp.toml`, started once you
  approve the file) and a folder outside the web it works on (`folder:` in its frontmatter, used
  once you approve that folder). Setting a site (`set_site`) changes the agent's tools to fit it.
- **Composer's MCP server.** `composer mcp --session <id>` serves a session's tools over MCP on
  stdio, to any harness. The list follows the session's site and announces changes.
- **MCP events.** An MCP server composer runs (`[mcp.<name>]`, or a site's `mcp.toml`) can tell
  a session what the user did in its app. Every `tools/call` composer sends carries the calling
  session's id as `params._meta["composer/session"]`. The server sends the notification
  `notifications/composer/event` with params `{"session"?: string, "text": string, "data"?: any}`
  (over stdio, or on the GET event stream of a streamable-HTTP server). It goes to `session` when
  that is a session with the server's tools (the server is `config.toml`'s, or the session is in
  the site whose `mcp.toml` started it), else, without `session`, to the session that called one
  of the server's tools last; otherwise it is dropped with a warning in the log. The session gets
  `[automatic notice: <server> event] <text>` with `data` as compact JSON below it (each cut at
  4 KB): it wakes an idle agent and waits behind a running turn. A session gets at most one notice
  per server a second; events that come meanwhile go together in the next one.
- **Webhooks.** `POST /api/hooks/<name>` starts a run of any kind with its own token, waits, and
  answers with its value. The run is not kept unless the hook says `store = true`.
- **Runs that start runs.** `spawn_run` starts any kind from an agentic session; an agentic child
  posts its answer back when it ends.
- **Terminals and background processes.** Each session has server-owned terminals. The agent can
  read and type into them (each command approved) and start long commands in the background; it
  hears when they exit.

## Clients

- **`composer`**: the TUI. Chat, page browser and the session's terminals, keyboard first.
- **The desktop** (`apps/desktop`, gpui + CEF): the chat on the left, the stage on the right
  (pages, terminals, the wayai live view). Linux and Windows.
- **The phone app** (`apps/composer_app`, Flutter): Android only.

Other devices reach the server over your tailnet: `tailscale serve --bg http://127.0.0.1:7420`,
then `composer pair --name <device>` on the server prints a QR code for the phone and a
`composer login` line for a computer. `composer devices` lists them; `composer devices revoke
<name>` ends one.

## Requirements

- Rust (stable, edition 2024). Linux is the main platform; Windows via [WSL](docs/wsl.md).
- A model server with an OpenAI-compatible API: llama.cpp's `llama-server`, vLLM, Ollama, or a
  hosted provider such as OpenRouter.
- [`just`](https://github.com/casey/just) for site actions.
- Optional: an ACP agent (such as `claude-agent-acp`) for outside agents; the CEF runtime for the
  desktop (`apps/desktop/scripts/fetch-cef.sh`); Flutter and the Android SDK for the phone app.

## Start

```sh
cargo install --path crates/cli
composer init                     # ~/.composer (or $COMPOSER_HOME): sites, config.toml, state/
$EDITOR ~/.composer/config.toml   # [provider]: your model server
composer serve &                  # http://127.0.0.1:7420
composer                          # the TUI
composer ask "what's on my plans?"   # one question, answered on stdout
```

`config.toml` has these sections, each with defaults and explained in the file `init` writes:
`[provider]`, `[server]`, `[agent]`, `[terminals]`, `[attachments]`, `[desktop]`, `[viewer]`,
`[mcp.<name>]` (MCP servers in every session), `[hooks.<name>]` (webhooks) and `[acp.<name>]`
(outside agents). Saving it applies most changes to the running server.

## Docs

- [docs/icm.md](docs/icm.md): ICM, and how workflows map onto sites
- [docs/tui-mode.md](docs/tui-mode.md): terminals on the stage, and `type: tui` pages
- [docs/wsl.md](docs/wsl.md): the server in WSL on Windows
- [docs/releasing.md](docs/releasing.md): releases and Android signing
- [CLAUDE.md](CLAUDE.md): conventions, checks, and how to test the TUI and the app headlessly
- [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md): read before exposing a server
  beyond your machine

## License

Apache License 2.0: see [LICENSE](LICENSE). The bundled fonts keep their own licenses.
