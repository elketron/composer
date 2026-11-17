#!/usr/bin/env bash
# Ticket 0206, route C: is the Windows build signed anywhere today? And what web/wasm tooling does
# this machine have, for the cost estimate in the report?
#   bash signing-and-tools.sh <repo root>
set -uo pipefail
cd "${1:?usage: signing-and-tools.sh <repo root>}" || exit 1

echo '### any code-signing step in the repo? (sign|codesign|signtool|certificate|smartscreen)'
grep -rniE 'codesign|signtool|sign[- ]?tool|code sign|signing|certificate|smartscreen|Authenticode' \
  .github scripts apps/desktop docs 2>/dev/null | grep -v 'target/\|node_modules' | head -20
echo "(exit $?; empty = nothing found)"
echo
echo '### the Windows packaging script, in full'
cat scripts/package-desktop-windows.ps1
echo
echo '### the release workflow: the Windows job and the publish step'
sed -n '/desktop-windows:/,/^  publish:/p' .github/workflows/release.yml
echo
echo '### does any crate declare cargo features? (the report claims server/cli declare none)'
for c in crates/server crates/cli crates/client apps/desktop; do
  printf '%-16s [features] sections: %s\n' "$c" "$(grep -c '^\[features\]' "$c/Cargo.toml")"
done
echo
echo '### wasm/browser tooling present on this machine'
for t in wasm-bindgen wasm-pack trunk node npm bun; do
  printf '%-14s %s\n' "$t" "$(command -v "$t" || echo 'not installed')"
done
rustup target list --installed
rustup toolchain list
