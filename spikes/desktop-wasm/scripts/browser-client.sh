#!/usr/bin/env bash
# Ticket 0206 sections 3 and 5: what a browser would have to do to be a client —
# auth, the open endpoint, and whether any client today is pure http+ws.
#   bash browser-client.sh <repo root>
set -uo pipefail
cd "${1:?usage: browser-client.sh <repo root>}" || exit 1

echo '### how a client authenticates (api/auth.rs)'
sed -n '1,60p' crates/server/src/api/auth.rs
echo
echo '### the token prefixes it guards'
sed -n '120,150p' crates/server/src/api/auth.rs
echo
echo '### the cookie the server hands a browser (composer_token)'
grep -rn 'composer_token' crates/server/src --include=*.rs | head -10
echo
echo '### the open endpoint a client calls to open an app (apps/app_open.rs)'
grep -n 'pub fn\|pub async fn\|route\|token\|client' crates/server/src/apps/app_open.rs | head -25
echo
echo '### is there a web-session login for browsers today (POST /api/web-session)?'
grep -n -A12 'async fn web_session' crates/server/src/api/http_routes.rs | head -30
echo
echo '### pair page / device flow a browser could use'
grep -rn 'fn pair_page\|pair.html\|/pair' crates/server/src/api/pair_routes.rs | head -12
echo
echo '### what the phone (a pure http+ws client) links against'
grep -rn 'ws://\|http\|WebSocket' apps/composer_app/lib/state/*.dart 2>/dev/null | head -8
echo
echo '### does the repo already build anything for the browser besides site apps?'
grep -rn 'wasm32\|wasm-pack\|wasm_bindgen' --include=*.toml --include=*.json --include=*.sh --include=*.yml . 2>/dev/null | grep -v 'spikes/\|node_modules\|Cargo.lock' | head -15
