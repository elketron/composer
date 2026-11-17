#!/usr/bin/env bash
# Ticket 0206 section 5: what the server serves to a browser today, so the `--features web`
# description is about a real route table and not an imagined one.
#   bash serving.sh <repo root>
set -uo pipefail
cd "${1:?usage: serving.sh <repo root>}" || exit 1

echo '### route definitions mentioning /p/ or /h/ (project and Home app roots)'
grep -rn '"/p/\|/p/\|"/h/\|/h/' crates/server/src --include=*.rs | grep -v '//' | head -25
echo
echo '### where app UI files come from: dist on disk vs compiled in'
grep -rn 'ServeDir\|ServeFile\|include_dir\|dist\b\|dist/' crates/server/src --include=*.rs | head -25
echo
echo '### the route table itself (nest/route calls in the workspace router)'
grep -rn 'nest(\|\.route(' crates/server/src/api/routes.rs crates/server/src/api/mod.rs crates/server/src/server.rs crates/server/src/host_routes.rs 2>/dev/null | head -40
echo
echo '### features declared in the server crate (is there any today?)'
sed -n '/\[features\]/,/^\[/p' crates/server/Cargo.toml
sed -n '/\[features\]/,/^\[/p' crates/cli/Cargo.toml
echo
echo '### what the desktop asks a pane to load (the URL builder the browser would reuse)'
grep -n 'fn \|/w/\|/p/\|/h/\|view=\|start=' apps/desktop/src/view_urls.rs | head -30
echo
echo '### how a site UI is built today (the same build a web client would use)'
grep -rn 'ui-build\|ui_build\|bun' crates/server/src/apps/app_build.rs | head -15
echo
echo '### the app token / cookie a browser would need'
grep -rn 'fn auth_cookie\|Set-Cookie\|cookie' crates/server/src/apps/app_tokens.rs crates/server/src/apps/app_grant.rs 2>/dev/null | head -15
echo
echo '### the desktop docs: what it says about reaching the server'
grep -n '^#\|token\|pair' docs/desktop.md | head -25
echo
echo '### the WSL docs: today'"'"'s Windows answer'
grep -n '^#' docs/wsl.md | head -20
