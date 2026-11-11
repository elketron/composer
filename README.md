# composer

A workspace where your agents (and you) browse memory, plans, agent definitions and scripts as a
set of **sites**: folders of markdown pages, each with optional actions (a `justfile`, run after
your approval) and an optional micro app (Svelte). Agents run on your own model server.

- **`composer serve`** — the server: sessions, the agent loop, sites, actions, micro apps, workers.
- **`composer`** — the TUI: chat and page browser side by side, keyboard first.
- **The app** (`apps/composer_app`, Flutter) — the same for Linux, Windows and Android, with several
  servers at once, the micro apps in an embedded browser, and notifications.
- **`site://chat`** — a small web chat for phones, no install.

## Requirements

- Rust (stable, edition 2024) to build it; Linux is the main platform, Windows via [WSL](docs/wsl.md).
- A model server with an OpenAI-compatible API: llama.cpp's `llama-server`, vLLM, Ollama, or a hosted provider
  such as OpenRouter (`[provider]` in `config.toml`).
- [`just`](https://github.com/casey/just) for site actions, and [`bun`](https://bun.sh) to build micro apps.
- Optional: the Claude Code CLI, logged in, to run tickets with it; Flutter to build the app.

## Start

```sh
cargo install --path crates/cli
composer init                     # ~/.composer: sites, templates, config.toml (set your model server)
composer serve &                  # http://127.0.0.1:7420
composer                          # the TUI
```

After installing a new version, stop the server and run `composer update`: it brings the stock
sites, agents and templates up to date, merges them with your changes, and leaves what it can't
merge in `~/.composer/state/updates/` for you to look at.

Other devices: expose the server to your tailnet (`tailscale serve --bg http://127.0.0.1:7420`)
and pair them with `composer pair --name <device>`.

Micro apps run sandboxed at `<server>/a/<app token>/` (opened from the TUI's `a`, the desktop or the app); their
code can be written by an agent, so they never get the workspace's credentials. The chat client (`site://chat`)
does: build it (`just ui-build` in its site), then approve that build with `composer app approve chat`. A changed
build is refused until you approve it again.

## Docs

- [docs/plan.md](docs/plan.md) — design of the server, TUI and micro apps
- [docs/plan-app.md](docs/plan-app.md) — the app
- [docs/lean-orchestration.md](docs/lean-orchestration.md) — proposal: a small harness, capabilities injected by sites, workflows as files
- [docs/workflows.md](docs/workflows.md) — design: one agent, a workflow folder per kind of work, and a shared view (an application in wayai, or a view type)
- [docs/project-hub.md](docs/project-hub.md) — design: the project as the hub, with sites operating on it
- [docs/tui-mode.md](docs/tui-mode.md) — TUI capabilities: a terminal program with a control adapter (Hunk)
- [docs/wsl.md](docs/wsl.md) — running the server in WSL on Windows
- [docs/releasing.md](docs/releasing.md) — releases and Android signing
- [CLAUDE.md](CLAUDE.md) — conventions, and how to test the TUI and the app headlessly
- [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md) — before exposing a server beyond your machine

## License

Apache License 2.0: see [LICENSE](LICENSE). The bundled fonts keep their own licenses.
