#!/usr/bin/env bash
# Ticket 0206: one cargo check per probe crate, for a browser target, each in its own call on
# purpose — a single `cargo check --workspace --keep-going` unifies features across all members,
# so a crate that only needs `tokio/sync` gets built with `tokio/full` and its own verdict is
# hidden behind another probe's failure.
#   bash run-probes.sh <probe workspace dir> <log dir>
set -uo pipefail

probe="${1:?usage: run-probes.sh <probe workspace> <log dir>}"
logs="${2:?usage: run-probes.sh <probe workspace> <log dir>}"
mkdir -p "$logs"
matrix="$logs/matrix.txt"
: > "$matrix"
cd "$probe" || exit 1

for manifest in */Cargo.toml; do
  dir=$(dirname "$manifest")
  pkg=$(sed -n 's/^name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest" | head -1)
  [ -n "$pkg" ] || continue
  log="$logs/$pkg.txt"
  if cargo check -p "$pkg" --target wasm32-unknown-unknown > "$log" 2>&1; then
    printf 'PASS  %s\n' "$pkg" >> "$matrix"
  else
    first=$(grep -m1 -E '^(error|error\[)' "$log" | cut -c1-200)
    printf 'FAIL  %s :: %s\n' "$pkg" "$first" >> "$matrix"
  fi
done

cat "$matrix"
