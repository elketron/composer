# Security

composer runs agents that read and write files and, after your approval, run commands as your user.
Read this before exposing a server beyond your own machine.

## Reporting a vulnerability

Please don't open a public issue. Use **Report a vulnerability** on the repository's Security tab
(GitHub private vulnerability reporting).

## What composer protects, and what it doesn't

- **The server binds to `127.0.0.1:7420`** by default. Every API call and WebSocket needs a token: the local
  one in `~/.composer/state/token`, or a per-device token from `composer pair` (revocable). To reach it from
  other devices, put it behind something that authenticates the network, such as `tailscale serve`; don't
  bind it to a public interface.
- **Agents' own tools reach the sites only** (`site://…` pages under `~/.composer/sites`), never
  `~/.composer/state` (tokens, sessions, approvals).
- **Recipes run after approval, as your user, unsandboxed.** Approving a recipe approves what it runs,
  including project code it builds (a micro app's `ui-build` runs its `vite.config`). Approve with that in mind;
  "always" rules live in `state/approvals.json`.
- **Claude Code workers are off by default** and allowed per project in `config.toml` (not in the repository, so
  a cloned repo can't switch itself on). They run with a cleared environment and a tool allowlist.
- **MCP servers** are started from `config.toml` only; their calls need approval unless `allow` matches.
- **Prompt injection is not solved.** A page, a web result or an attachment can steer an agent. Approvals are
  the boundary: read what you approve.
