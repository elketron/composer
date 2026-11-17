#!/usr/bin/env bash
# Ticket 0206 routes C and 4: what the release path does today, and what reaches a browser for
# terminals and the chat.
#   bash release-and-terms.sh <repo root>
set -uo pipefail
cd "${1:?usage: release-and-terms.sh <repo root>}" || exit 1

echo '### release workflow: which targets are built and published'
grep -n 'name:\|runs-on\|matrix\|target\|windows\|desktop\|package\|upload\|sign' .github/workflows/release.yml | head -40
echo
echo '### does the packaging script know Windows at all'
grep -n 'windows\|exe\|dll\|zip\|tar\|sign' scripts/package-desktop.sh | head -25
echo
echo '### desktop docs: the Windows section as it stands'
sed -n '/Windows/,/^## /p' docs/desktop.md | head -30
echo
echo '### what a terminal tab sends over the ws (protocol shape, server side)'
grep -rn 'pub struct\|pub enum' crates/protocol/src/*term* 2>/dev/null | head -20
ls crates/server/src/terminals/
grep -rn 'TerminalOutput\|terminal.output\|bytes\|base64' crates/protocol/src/lib.rs | head -12
echo
echo '### the chat over ws: what the phone does (a browser would do the same)'
grep -rn 'class \|void send\|ws' apps/composer_app/lib/state/connection.dart 2>/dev/null | head -12
ls apps/composer_app/lib/state/ | head -20
echo
echo '### the client-app path a browser shell would be: is the chat site still a client app?'
grep -rn 'clients' crates/server/src/config_template.toml | head -10
sed -n '/\[apps\]/,/^#/p' crates/server/src/config_template.toml | head -20
echo
echo '### CI: is the desktop even built in CI today'
grep -n 'desktop\|windows\|archlinux\|podman' .github/workflows/ci.yml | head -20
