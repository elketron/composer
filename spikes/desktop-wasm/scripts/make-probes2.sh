#!/usr/bin/env bash
# Ticket 0206, probe set 2: the two the ticket's route A turns on — gpui and CEF for a browser
# target, alone, so their own errors are visible instead of hidden behind the desktop's graph.
#   bash make-probes2.sh <dir>
set -euo pipefail
root="${1:?usage: make-probes2.sh <dir>}"
mkdir -p "$root"
members=()

mk() {
  local name="$1" deps="$2" extra="${3:-}"
  mkdir -p "$root/$name/src"
  : > "$root/$name/src/lib.rs"
  cat > "$root/$name/Cargo.toml" <<EOF
[package]
name = "$name"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
$deps
$extra
EOF
  members+=("$name")
}

# gpui's cross-platform core on its own: what does the toolkit itself say about wasm32?
mk p-gpui-core 'gpui = { package = "gpui-pre", version = "=0.3.11" }'
# The platform layer the desktop actually asks for (x11 + wayland + font-kit).
mk p-gpui-platform 'gpui_platform = { package = "gpui-pre-platform", version = "=0.3.11", features = ["font-kit", "x11", "wayland"] }'
# Native Chromium.
mk p-cef 'cef = "=156.3"'
# The terminal emulator's screen model without gpui (the desktop draws it itself).
mk p-wezterm-surface 'wezterm-surface = { package = "tattoy-wezterm-surface", version = "=0.1.0-fork.1" }'
# The other C-side question: rustls without the aws-lc provider, the way a wasm build would pick.
mk p-rustls-ring 'rustls = { version = "=0.23.46", default-features = false, features = ["std", "ring", "tls12"] }'
# tokio with only the features the crate's own error says are supported on wasm.
mk p-tokio-wasm 'tokio = { version = "=1.53.3", default-features = false, features = ["sync", "macros", "io-util", "rt", "time"] }'

cat > "$root/Cargo.toml" <<EOF
# Ticket 0206's probe set 2. Its own workspace; nothing outside imports it.
[workspace]
resolver = "3"
members = [$(printf '"%s",' "${members[@]}" | sed 's/,$//')]

[profile.dev]
debug = false
EOF
echo "wrote $root (${#members[@]} probes)"
