#!/usr/bin/env bash
# Runs every CI job's steps (.github/workflows/ci.yml) locally, so a merge can't skip one:
# rust (fmt, clippy, tests), apps (every stock app and the template build against the sdk in a
# fresh home), protocol (the Dart models are current, analyze, test), flutter (analyze, test).
#   scripts/ci.sh            all
#   scripts/ci.sh rust apps  some
set -euo pipefail
cd "$(dirname "$0")/.."
jobs=("$@")
[ ${#jobs[@]} -eq 0 ] && jobs=(rust apps protocol flutter)

step() { printf '\n== %s\n' "$*"; }

for job in "${jobs[@]}"; do
  case "$job" in
    rust)
      step "rust: fmt"; cargo fmt --all --check
      step "rust: clippy"; cargo clippy --workspace --all-targets --all-features -- -D warnings
      step "rust: test"; cargo test --workspace
      step "rust: ts bindings"; cargo test -q -p composer-protocol --features ts
      ;;
    apps)
      home=$(mktemp -d)
      trap 'rm -rf "$home"' EXIT
      export COMPOSER_HOME="$home"
      step "apps: init a fresh home"; cargo run -q -p composer-cli -- init
      for justfile in "$COMPOSER_HOME"/sites/*/justfile; do
        site=$(dirname "$justfile")
        if grep -q '^ui-build:' "$justfile"; then
          step "apps: $(basename "$site")"; (cd "$site" && just ui-build)
        fi
      done
      step "apps: the template"
      mkdir -p "$COMPOSER_HOME/sites/template-check"
      cp -r "$COMPOSER_HOME/templates/app" "$COMPOSER_HOME/sites/template-check/ui"
      (cd "$COMPOSER_HOME/sites/template-check/ui" && bun install && bun run build)
      unset COMPOSER_HOME
      ;;
    protocol)
      step "protocol: the Dart models are current"
      cargo run -q -p composer-protocol --features schema --bin dartgen -- --check
      step "protocol: analyze, test"
      (cd apps/composer_protocol && dart pub get >/dev/null && dart analyze --fatal-infos && dart test)
      ;;
    flutter)
      step "flutter: analyze, test"
      (cd apps/composer_app && flutter pub get >/dev/null && flutter analyze --fatal-infos && flutter test)
      ;;
    *) echo "unknown job: $job (rust, apps, protocol, flutter)"; exit 2 ;;
  esac
done
printf '\nall CI steps passed: %s\n' "${jobs[*]}"
