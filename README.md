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

## Docs

- [docs/plan.md](docs/plan.md) — design of the server, TUI and micro apps
- [docs/plan-app.md](docs/plan-app.md) — the app
- [docs/wsl.md](docs/wsl.md) — running the server in WSL on Windows
- [docs/releasing.md](docs/releasing.md) — releases and Android signing
- [CLAUDE.md](CLAUDE.md) — conventions, and how to test the TUI and the app headlessly
- [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md) — before exposing a server beyond your machine

## License

Apache License 2.0: see [LICENSE](LICENSE). The vendored `apps/vendor/flutter_inappwebview_linux`
and the bundled fonts keep their own licenses.
