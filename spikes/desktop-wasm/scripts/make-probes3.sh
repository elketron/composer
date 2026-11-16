#!/usr/bin/env bash
# Ticket 0206, probe set 3: the reusable part, proven by compiling it.
#
#   p-core                composer-core (pages, frontmatter, site urls) on its own.
#   p-wezterm-term-js     the terminal emulator plus the getrandom features its error names.
#   composer-client-lite  composer-client's modules minus the transport.
#   p-desktop-logic       the desktop's framework-free modules, compiled against
#                         composer-client-lite under the name `composer_client`.
#
# The last two answer "which of the plain logic is the reusable part": it is what still
# type-checks once only the transport is swapped. Modules are pulled in verbatim with #[path];
# nothing in the repo is modified.
#   bash make-probes3.sh <dir>
set -euo pipefail
root="${1:?usage: make-probes3.sh <dir>}"
mkdir -p "$root"
# from <probe>/<pkg>/Cargo.toml the repo root is four levels up; from its src/ five.
T='../../../../'
S='../../../../../'

mk() { # <name> <deps>
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
}

PROTO="composer-protocol = { path = \"${T}crates/protocol\" }"

mk p-core "composer-core = { path = \"${T}crates/core\" }"

# The terminal emulator, plus what its own error asks for.
mk p-wezterm-term-js "tattoy-wezterm-term = \"=0.1.0-fork.5\"
uuid = { version = \"1\", features = [\"js\"] }
getrandom = { version = \"0.2\", features = [\"js\"] }
getrandom03 = { package = \"getrandom\", version = \"0.3\", features = [\"wasm_js\"] }
getrandom04 = { package = \"getrandom\", version = \"0.4\", features = [\"wasm_js\"] }"

# --- composer-client without its transport -------------------------------------
mk composer-client-lite "$PROTO
serde = { version = \"=1.0.229\", features = [\"derive\"] }
serde_json = { version = \"=1.0.154\", features = [\"preserve_order\"] }
base64 = \"=0.22\"
anyhow = \"=1.0.104\"
reqwest = { version = \"=0.13.5\", features = [\"json\", \"stream\", \"form\", \"multipart\"] }
toml = \"=1.1.8\"
getrandom04 = { package = \"getrandom\", version = \"0.4\", features = [\"wasm_js\"] }"
cat > "$root/composer-client-lite/src/lib.rs" <<EOF
//! composer-client minus \`connection\` and \`cli\`, verbatim: the endpoint, the reducer, the store.
//! Declared at the crate root exactly as the real crate does, so \`crate::\` paths inside them
//! resolve the same way they do there.
#![allow(dead_code)]
#[path = "${S}crates/client/src/endpoint.rs"] mod endpoint;
#[path = "${S}crates/client/src/chat_item.rs"] mod chat_item;
#[path = "${S}crates/client/src/tool_text.rs"] mod tool_text;
#[path = "${S}crates/client/src/chat.rs"] mod chat;
#[path = "${S}crates/client/src/terminal_view.rs"] mod terminal_view;
#[path = "${S}crates/client/src/window_binding.rs"] mod window_binding;
#[path = "${S}crates/client/src/store.rs"] mod store;

pub use chat::Chat;
pub use chat_item::{Approval, ChatItem, Streaming};
pub use endpoint::Endpoint;
pub use store::Store;
pub use terminal_view::TerminalView;
pub use tool_text::{tool_detail, tool_summary};
pub use window_binding::WindowBinding;
EOF

# --- the desktop's framework-free modules --------------------------------------
mkdir -p "$root/p-desktop-logic/src"
cat > "$root/p-desktop-logic/Cargo.toml" <<EOF
[package]
name = "p-desktop-logic"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
$PROTO
composer-core = { path = "${T}crates/core" }
composer_client = { package = "composer-client-lite", path = "../composer-client-lite" }
pulldown-cmark = { version = "=0.13.4", default-features = false }
toml = "=1.1.8"
serde_json = { version = "=1.0.154", features = ["preserve_order"] }
EOF
{
  echo '//! The desktop'"'"'s modules that never name a UI framework, CEF or tokio, verbatim, for a'
  echo '//! browser target, against a transport-free composer-client. Declared at the crate root so'
  echo '//! their own \`crate::\` paths mean the same thing they mean in the desktop crate.'
  echo '//! The modules that draw are not here. Neither are key_map and term_config (they need the'
  echo '//! terminal emulator crate, which does not build for wasm), pick_lists and home_lists'
  echo '//! (they reference crate::picker, which is gpui-bound), terminal_link (it imports'
  echo '//! composer_client::Connection, the transport) and notice (notify_rust, a desktop daemon).'
  echo '#![allow(warnings)]'
  for m in browser_tabs chat_width dom_keys draft grid link_state markdown matcher \
           overlay_frame project_binding prose stage_tabs theme uploads user_review \
           view_urls workspaces; do
    echo "#[path = \"${S}apps/desktop/src/$m.rs\"]"
    echo "pub mod $m;"
  done
} > "$root/p-desktop-logic/src/lib.rs"

cat > "$root/Cargo.toml" <<EOF
# Ticket 0206's probe set 3. Its own workspace; nothing outside imports it.
[workspace]
resolver = "3"
members = ["p-core", "p-wezterm-term-js", "composer-client-lite", "p-desktop-logic"]

[profile.dev]
debug = false
EOF
echo "wrote $root"
