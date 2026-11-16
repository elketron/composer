#!/usr/bin/env bash
# Ticket 0206 (spike: does the desktop compile to wasm) — throwaway probe crates.
#
# One tiny crate per dependency, so that
#   cargo check --target wasm32-unknown-unknown --workspace --keep-going
# answers "does THIS crate compile for a browser target, on its own" for each suspect, including
# the stand-ins a browser client would use instead. Not production code; delete with the spike.
#
#   bash make-probes.sh <dir to create the probe workspace in>
set -euo pipefail

root="${1:?usage: make-probes.sh <probe workspace dir>}"
mkdir -p "$root"
members=()

# mk <name> <dependency lines>
mk() {
  local name="$1" deps="$2"
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
EOF
  members+=("$name")
}

# --- what the ticket calls "needs sockets / threads / a filesystem" -------------
# The exact tokio spec composer-client asks for (workspace dep: features = ["full"]).
mk p-tokio-full 'tokio = { version = "=1.53.3", features = ["full"] }'
# The same runtime with sockets left out: the hypothesis to test.
mk p-tokio-async 'tokio = { version = "=1.53.3", default-features = false, features = ["sync", "time", "macros", "rt", "rt-multi-thread"] }'
# Sockets alone.
mk p-tokio-net 'tokio = { version = "=1.53.3", default-features = false, features = ["net"] }'
# getrandom says the fix is its wasm_js feature; prove it.
mk p-getrandom-wasmjs 'getrandom = { version = "=0.4.3", features = ["wasm_js"] }'
mk p-rustls 'rustls = { version = "=0.23.46" }'
mk p-tokio-tungstenite 'tokio-tungstenite = { version = "=0.30.0", features = ["rustls-tls-webpki-roots"] }'

# --- what a browser would use instead ------------------------------------------
mk p-wasm-websocket 'js-sys = "0.3"
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = ["WebSocket", "MessageEvent", "CloseEvent", "ErrorEvent", "EventTarget", "BinaryType", "TextDecoder"] }'
# reqwest as composer-client asks for it, and without the TLS/multipart extras.
mk p-reqwest-default 'reqwest = { version = "=0.13.5", features = ["json", "stream", "form", "multipart"] }'
mk p-reqwest-minimal 'reqwest = { version = "=0.13.5", default-features = false, features = ["json"] }'

# --- the parts of composer-client a browser client would reuse -----------------
# The modules are included verbatim from the repo through #[path], minus the transport
# (connection.rs, cli/) and minus endpoint.rs, which is probed on its own.
CLIENT_DEPS='composer-protocol = { path = "../../../../crates/protocol" }
serde = { version = "=1.0.229", features = ["derive"] }
serde_json = { version = "=1.0.154", features = ["preserve_order"] }
base64 = "=0.22"'

mk p-client-logic "$CLIENT_DEPS"
cat > "$root/p-client-logic/src/lib.rs" <<'EOF'
//! composer-client without its transport: the chat reducer and the store, verbatim.
#[path = "../../../../../crates/client/src/chat_item.rs"]
mod chat_item;
#[path = "../../../../../crates/client/src/tool_text.rs"]
mod tool_text;
#[path = "../../../../../crates/client/src/chat.rs"]
mod chat;
#[path = "../../../../../crates/client/src/terminal_view.rs"]
mod terminal_view;
#[path = "../../../../../crates/client/src/window_binding.rs"]
mod window_binding;
#[path = "../../../../../crates/client/src/store.rs"]
mod store;

pub use chat::Chat;
pub use chat_item::{Approval, ChatItem, Streaming};
pub use store::Store;
pub use terminal_view::TerminalView;
pub use window_binding::WindowBinding;
EOF

# Endpoint on its own: it reads the home with std::fs and talks http with reqwest.
mk p-client-endpoint "$CLIENT_DEPS
anyhow = \"=1.0.104\"
reqwest = { version = \"=0.13.5\", features = [\"json\", \"stream\", \"form\", \"multipart\"] }
toml = \"=1.1.8\""
cat > "$root/p-client-endpoint/src/lib.rs" <<'EOF'
//! composer-client's Endpoint, verbatim: no sockets, but std::fs and reqwest.
#[path = "../../../../../crates/client/src/endpoint.rs"]
mod endpoint;

pub use endpoint::Endpoint;
EOF

# Connection on its own: tokio channels + tungstenite's Message at its boundary.
mk p-client-connection "$CLIENT_DEPS
futures = \"=0.3\"
tokio = { version = \"=1.53.3\", features = [\"full\"] }
tokio-tungstenite = { version = \"=0.30.0\", features = [\"rustls-tls-webpki-roots\"] }"
cat > "$root/p-client-connection/src/lib.rs" <<'EOF'
//! The desktop's connection code as it stands: channels + tungstenite.
pub mod chat_item {
    #[path = "../../../../../crates/client/src/chat_item.rs"]
    mod m;
    pub use m::*;
}
#[path = "../../../../../crates/client/src/connection.rs"]
mod connection;

pub use connection::{Connection, Event};
EOF

# --- the desktop's own dependencies that are not UI ----------------------------
mk p-pulldown-cmark 'pulldown-cmark = { version = "=0.13.4", default-features = false }
composer-protocol = { path = "../../../../crates/protocol" }'
mk p-image 'image = { version = "=0.25.10", default-features = false, features = ["png", "jpeg"] }'
mk p-wezterm-term 'tattoy-wezterm-term = "=0.1.0-fork.5"'
mk p-toml 'toml = "=1.1.8"'
mk p-open 'open = "=5.4"'

# --- the two that are expected to be impossible --------------------------------
# Kept out of the default workspace: they need a native runtime to even configure.
# Run them with --workspace excluding the rest if you want their exact errors; the
# desktop build reports them anyway.

cat > "$root/Cargo.toml" <<EOF
# Ticket 0206's throwaway probes. Its own workspace on purpose: nothing here is a member of
# the main workspace or of apps/desktop, and nothing outside imports it.
[workspace]
resolver = "3"
members = [$(printf '"%s",' "${members[@]}" | sed 's/,$//')]

[profile.dev]
debug = false
EOF

echo "wrote $root (${#members[@]} probes)"
