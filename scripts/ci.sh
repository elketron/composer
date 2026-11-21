#!/usr/bin/env bash
# Runs every CI job's steps (.github/workflows/ci.yml) locally, so a merge can't skip one:
# rust (fmt, clippy, tests), protocol (the Dart models are current, analyze, test), flutter
# (analyze, test).
#   scripts/ci.sh            all
#   scripts/ci.sh rust flutter  some
#   scripts/ci.sh desktop    the desktop app (apps/desktop, its own workspace; not in CI yet: 0187)
set -euo pipefail
cd "$(dirname "$0")/.."
jobs=("$@")
[ ${#jobs[@]} -eq 0 ] && jobs=(rust protocol flutter)

step() { printf '\n== %s\n' "$*"; }

for job in "${jobs[@]}"; do
  case "$job" in
    rust)
      step "rust: fmt"; cargo fmt --all --check
      step "rust: clippy"; cargo clippy --workspace --all-targets --all-features -- -D warnings
      step "rust: test"; cargo test --workspace
      step "rust: ts bindings"; cargo test -q -p composer-protocol --features ts
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
    desktop)
      step "desktop: fmt, clippy, test"
      (cd apps/desktop && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test)
      ;;
    *) echo "unknown job: $job (rust, protocol, flutter, desktop)"; exit 2 ;;
  esac
done
printf '\nall CI steps passed: %s\n' "${jobs[*]}"
