# Contributing

Issues and pull requests are welcome. For anything bigger than a fix, open an issue first so we can
agree on the shape. What composer is: [README.md](README.md).

## Building and testing

```sh
cargo build
scripts/ci.sh                     # what CI runs: rust, protocol, flutter, desktop
scripts/ci.sh rust                # or some of them
```

The rust job is `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features
-- -D warnings` and `cargo test --workspace` (some tests run real recipes and need `just`). The
others need Dart, Flutter, or the desktop's CEF runtime (`apps/desktop/scripts/fetch-cef.sh`).
[CLAUDE.md](CLAUDE.md) lists every check, and how to run the app's integration tests.

Use a scratch home while trying things (`COMPOSER_HOME=/tmp/ch composer init`, another port in its
`config.toml`), so your own `~/.composer` stays untouched.

## Conventions

[CLAUDE.md](CLAUDE.md) holds them (it's also what coding agents read): one file per type with an
`impl`, Flux state in the TUI and the app, and how to regenerate the protocol's Dart models after
changing `crates/protocol`.

By contributing you agree that your contribution is licensed under the Apache License 2.0.
