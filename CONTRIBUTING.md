# Contributing

Issues and pull requests are welcome. For anything bigger than a fix, open an issue first so we can agree on
the shape; the design lives in [docs/plan.md](docs/plan.md) and [docs/plan-app.md](docs/plan-app.md).

## Building and testing

```sh
cargo build
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace            # some tests run real recipes and need `just`
```

The app (`apps/composer_app`, Flutter): `flutter analyze` and `flutter test`; the integration tests and how to
run them headlessly are described in [CLAUDE.md](CLAUDE.md).

Use a scratch home while trying things (`COMPOSER_HOME=/tmp/ch composer init`, another port in its
`config.toml`), so your own `~/.composer` stays untouched.

## Conventions

[CLAUDE.md](CLAUDE.md) holds them (it's also what coding agents read): one file per type with an `impl`, Flux
state in the TUI and the app, how to regenerate the protocol's Dart models after changing `crates/protocol`, and
keeping changes to `seed/` and `templates/` small, since `composer update` merges them into existing homes.

By contributing you agree that your contribution is licensed under the Apache License 2.0.
