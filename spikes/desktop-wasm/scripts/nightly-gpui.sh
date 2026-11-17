#!/usr/bin/env bash
# Ticket 0206: gpui-pre ships a browser backend (gpui-pre-web, selected by cfg(target_family =
# "wasm")); the only thing that stopped it on stable was wasm_thread needing a nightly feature.
# A nightly toolchain is installed here, so retry with it.
#   bash nightly-gpui.sh <probe2 dir> <logs dir>
set -uo pipefail
probe="${1:?}"
logs="${2:?}"
mkdir -p "$logs"

echo '### nightly toolchain and its wasm target'
rustup target list --installed --toolchain nightly 2>&1 | head -5
case "$(rustup target list --installed --toolchain nightly 2>&1)" in
  *wasm32-unknown-unknown*) echo 'target present';;
  *) rustup target add wasm32-unknown-unknown --toolchain nightly 2>&1 | tail -3;;
esac

cd "$probe" || exit 1
echo
echo '### command: cargo +nightly check -p p-gpui-platform --target wasm32-unknown-unknown'
if cargo +nightly check -p p-gpui-platform --target wasm32-unknown-unknown > "$logs/p-gpui-platform-nightly.txt" 2>&1; then
  echo 'RESULT: PASS (gpui-pre-platform + its web backend type-check for wasm32 on nightly)'
else
  echo 'RESULT: FAIL'
  grep -n 'could not compile\|^error' "$logs/p-gpui-platform-nightly.txt" | cut -c1-200 | head -15
fi

echo
echo '### command: cargo +nightly check -p p-wezterm-surface --target wasm32-unknown-unknown'
if cargo +nightly check -p p-wezterm-surface --target wasm32-unknown-unknown > "$logs/p-wezterm-surface-nightly.txt" 2>&1; then
  echo 'RESULT: PASS'
else
  echo 'RESULT: FAIL'
  grep -n 'could not compile\|^error' "$logs/p-wezterm-surface-nightly.txt" | cut -c1-200 | head -10
fi
echo
rustc --version --toolchain nightly 2>/dev/null || rustup run nightly rustc --version
