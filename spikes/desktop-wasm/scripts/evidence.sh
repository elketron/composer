#!/usr/bin/env bash
# Ticket 0206: the rest of the evidence in one pass.
#  1. what actually fails on the terminal-emulator path
#  2. does a browser wasm binary LINK (not just type-check), and how big is it
#  3. is anything in the release path signing the Windows build today
#  4. how app UIs are served by the server (the route a feature flag would sit beside)
set -uo pipefail
root="${1:?usage: evidence.sh <worktree>}"
d="$root/spikes/desktop-wasm"
cd "$d" || exit 1

echo '### 1. the failing crate on the terminal path, and who pulls it'
grep -n 'could not compile' logs/per-probe3/p-wezterm-term-js.txt | head -3
( cd probe3 && cargo tree -p p-wezterm-term-js --target wasm32-unknown-unknown -i mac_address 2>&1 | head -12 )
echo
echo '### 1b. does the surface crate the desktop also uses pull the emulator?'
( cd probe2 && cargo tree -p p-wezterm-surface --target wasm32-unknown-unknown 2>&1 | head -25 )
echo
echo '### 2. link test: a cdylib with the reusable logic, built for the browser'
mkdir -p link-test/src
cat > link-test/Cargo.toml <<'EOF'
[package]
name = "p-link"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
wasm-bindgen = "0.2"
composer-protocol = { path = "../../../../crates/protocol" }
composer-core = { path = "../../../../crates/core" }
composer_client = { package = "composer-client-lite", path = "../probe3/composer-client-lite" }
serde_json = "1"
EOF
cat > link-test/src/lib.rs <<'EOF'
//! A browser bundle that carries real composer logic: the chat reducer and the page model.
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn tool_summary(name: &str, args: &str) -> String {
    let parsed = serde_json::from_str(args).unwrap_or_default();
    composer_client::tool_summary(name, &parsed)
}
EOF
cat >> Cargo.toml <<'EOF'

# link-test is ticket 0206's browser link check; it is part of this throwaway workspace.
EOF
# add it as a member
python3 - <<'PY'
import pathlib
p = pathlib.Path("Cargo.toml")
t = p.read_text()
if "link-test" not in t.split("members")[1][:200]:
    t = t.replace('members = ["p-core"', 'members = ["link-test", "p-core"')
    p.write_text(t)
PY
if cargo build -p p-link --target wasm32-unknown-unknown > logs/link-test.txt 2>&1; then
  echo 'RESULT: PASS — it links into a .wasm'
  ls -la target/wasm32-unknown-unknown/debug/*.wasm 2>/dev/null | head -3
else
  echo 'RESULT: FAIL'; grep -n 'error\|could not compile' logs/link-test.txt | head -8
fi
echo
echo '### 2b. is the wasm toolchain for a real bundle present?'
for t in wasm-bindgen wasm-pack trunk; do printf '%-14s %s\n' "$t" "$(command -v $t || echo 'not installed')"; done
echo
echo '### 3. signing in the release path today'
cd "$root" || exit 1
grep -rniE 'sign|codesign|signtool|smartscreen' .github/workflows scripts apps/desktop/packaging docs 2>/dev/null | grep -vi 'signal\|design\|assign' | head -20
echo '--- release workflow jobs ---'
ls .github/workflows 2>/dev/null
grep -n 'windows\|desktop\|package' .github/workflows/release.yml 2>/dev/null | head -20
echo
echo '### 4. how app UIs reach a browser today'
grep -rn 'p/{project}/{app}\|/p//\|\"/p/\"\|route(\"/\|get(\"/' crates/server/src/http* crates/server/src/api* 2>/dev/null | head -0
grep -rln 'app_root\|static_app\|serve_app\|include_dir\|SiteApp\|asset' crates/server/src | head -20
