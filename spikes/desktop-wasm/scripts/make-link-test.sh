#!/usr/bin/env bash
# Ticket 0206: add the browser link-check crate to probe3's workspace and build it.
# p-link is a cdylib with wasm-bindgen exports that calls the reusable logic, so a `cargo build
# --target wasm32-unknown-unknown` answers "does it LINK for a browser", not just type-check.
#   bash make-link-test.sh <spike dir>
set -euo pipefail
d="${1:?usage: make-link-test.sh <spike dir>}"
root="$d/probe3/link-test"
mkdir -p "$root/src"

cat > "$root/Cargo.toml" <<'EOF'
[package]
name = "p-link"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
serde_json = "1"
composer-protocol = { path = "../../../../crates/protocol" }
composer-core = { path = "../../../../crates/core" }
composer_client = { package = "composer-client-lite", path = "../composer-client-lite" }
desktop_logic = { package = "p-desktop-logic", path = "../p-desktop-logic" }
EOF

cat > "$root/src/lib.rs" <<'EOF'
//! A browser bundle of the parts a web client would reuse, reached through the same names the
//! desktop uses: the chat reducer's tool summaries, the desktop's markdown block model, its page
//! URL rules, and its view-URL builder. Nothing here is composer production code; it is proof the
//! real modules link into a .wasm.
use wasm_bindgen::prelude::*;

/// `composer_client::tool_summary` + `desktop_logic`'s markdown model, as a browser would call them.
#[wasm_bindgen]
pub fn tool_summary(name: &str, args_json: &str) -> String {
    let parsed = serde_json::from_str(args_json).unwrap_or_default();
    composer_client::tool_summary(name, &parsed)
}

/// How many blocks the desktop's renderer would make of this page.
#[wasm_bindgen]
pub fn markdown_block_count(md: &str) -> usize {
    desktop_logic::markdown::blocks(md, None).len()
}

/// Which workspace route a page link maps to (`/w/<ws>/view/...`).
#[wasm_bindgen]
pub fn page_url(link: &str, base: &str) -> Option<String> {
    desktop_logic::view_urls::page_for(link, base)
}
EOF

python3 - "$d/probe3/Cargo.toml" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
t = p.read_text()
if "link-test" not in t:
    p.write_text(t.replace('members = [', 'members = ["link-test", ', 1))
PY
echo "added $root to $d/probe3/Cargo.toml"
