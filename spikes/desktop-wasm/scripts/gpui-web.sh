#!/usr/bin/env bash
# Ticket 0206: what is gpui-pre-web? Is it a real backend or a stub, and how does the platform
# crate pick it? Also: which wasm_thread versions exist, and is a nightly toolchain here?
set -uo pipefail
reg=$(ls -d "$HOME"/.cargo/registry/src/index.crates.io-*/ 2>/dev/null | head -1)
echo "### registry: $reg"
echo

echo '### how gpui-pre-platform selects a backend (Cargo.toml target sections)'
grep -n -A4 'target\.' "$reg"gpui-pre-platform-0.3.11/Cargo.toml 2>/dev/null | head -60
echo

echo '### gpui-pre-web: files and size'
find "$reg"gpui-pre-web-0.3.11/src -name '*.rs' 2>/dev/null | xargs wc -l | sort -n | tail -30
echo

echo '### what it talks to the browser with (web_sys / js_sys types it uses)'
grep -rho 'web_sys::[A-Za-z]*\|js_sys::[A-Za-z]*' "$reg"gpui-pre-web-0.3.11/src 2>/dev/null | sort | uniq -c | sort -rn | head -25
echo

echo '### gpui-pre-web: its own dependencies'
cat "$reg"gpui-pre-web-0.3.11/Cargo.toml 2>/dev/null | sed -n '/^\[dependencies/,/^\[target/p' | head -50
echo

echo '### is it referenced by the desktop build at all for wasm? (cargo tree, wasm target)'
( cd "${DESKTOP:-$HOME/.composer/workspaces/default/state/worktrees/0206/apps/desktop}" \
  && cargo tree -e no-dev --target wasm32-unknown-unknown 2>/dev/null | grep -i 'gpui\|cef\|wezterm-term' | head -20 )
echo

echo '### toolchains here, and wasm_thread versions on crates.io'
rustup toolchain list
cargo search wasm_thread --limit 3 2>&1 | head -5
echo

echo '### does gpui's own cross-platform core branch on wasm?'
grep -rl 'target_arch = "wasm32"\|target_family = "wasm"' "$reg"gpui-pre-0.3.11/src 2>/dev/null | head -15
echo "files that do: $(grep -rl 'target_arch = "wasm32"\|target_family = "wasm"' "$reg"gpui-pre-0.3.11/src 2>/dev/null | wc -l)"
