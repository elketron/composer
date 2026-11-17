#!/usr/bin/env bash
# Ticket 0206: is the terminal emulator on the browser path or not? What exactly does each of the
# two wezterm crates pull, and where does the hard blocker (mac_address) come from?
#   bash term-tree.sh <probe2 dir> <probe3 dir>
set -uo pipefail
p2="${1:?}"; p3="${2:?}"

cd "$p2" || exit 1
echo '### does p-wezterm-surface include the emulator (tattoy-wezterm-term)?'
cargo tree -p p-wezterm-surface --target wasm32-unknown-unknown 2>&1 | head -30
echo
echo '### what wezterm-surface is, by its own description'
grep -m2 -E 'description|^name|^version' "$(ls -d "$HOME"/.cargo/registry/src/index.crates.io-*/tattoy-wezterm-surface-0.1.0-fork.1)/Cargo.toml" 2>/dev/null
echo
echo '### its declared dependencies'
sed -n '/^\[dependencies/,/^\[target/p' "$(ls -d "$HOME"/.cargo/registry/src/index.crates.io-*/tattoy-wezterm-surface-0.1.0-fork.1)/Cargo.toml" 2>/dev/null | head -30
echo
cd "$p3" || exit 1
echo '### who pulls mac_address on the emulator path'
cargo tree -p p-wezterm-term-js --target wasm32-unknown-unknown 2>&1 \
  | grep -A3 -B3 'mac_address' | head -20
echo
echo '### does the emulator crate itself (wezterm-term) reach mac_address?'
cargo tree -p p-wezterm-term-js --target wasm32-unknown-unknown -i mac_address 2>&1 | head -20
