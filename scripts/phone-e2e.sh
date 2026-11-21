#!/bin/bash
# Runs the phone app's end-to-end tests (apps/composer_app/integration_test) on an Android device
# or emulator against a test server on this machine. Each test is a fresh install, so each gets
# its own pairing code; the server's port is forwarded into the device (`adb reverse`).
#
#   COMPOSER_HOME=<test home> scripts/phone-e2e.sh <device> http://127.0.0.1:7531 [test...]
#
# The server has one home, so its URL has no path. A headless emulator:
# `emulator -avd <name> -read-only -no-window -no-audio`.
#
# Tests default to pairing, chat, reader and site. What the test home needs:
# - chat: a `tools` site whose justfile has `greet name:` printing `zebra-42`, and a model that
#   runs it. The greet recipe must not be approved yet: use a fresh home, or remove it from
#   state/approvals.json and restart the server.
# - site: a `demo` site, and a `[viewer]` in config.toml pointing at a running wayai.
# - measure_open (not in the default list): a few sessions to open.
set -euo pipefail
device=${1:?usage: phone-e2e.sh <device> <server url> [test...]}
server=${2:?usage: phone-e2e.sh <device> <server url> [test...]}
server=${server%/}
shift 2
tests=("$@")
[ ${#tests[@]} -eq 0 ] && tests=(pairing chat reader site)
composer=${COMPOSER:-composer}
port=$(sed -E 's#^[a-z]+://[^:/]+:([0-9]+).*#\1#' <<<"$server")
adb -s "$device" reverse "tcp:$port" "tcp:$port" >/dev/null
cd "$(dirname "$0")/../apps/composer_app"

pair_code() {
  "$composer" pair --name phone-e2e 2>/dev/null \
    | sed -nE 's/^computer: +composer login [^ ]+ ([A-Z0-9-]+)$/\1/p' | tail -1
}

failed=()
for t in "${tests[@]}"; do
  code=$(pair_code)
  defines=(--dart-define=PAIR_LINE="composer login $server $code")
  # SHOT_PAUSE=<seconds>: tests stop at their `shot`s (support.dart) so a watcher can screenshot them.
  [ -n "${SHOT_PAUSE:-}" ] && defines+=(--dart-define=SHOT_PAUSE="$SHOT_PAUSE")
  echo "== $t"
  # A fresh install asks for the notification permission, and the dialog takes the app out of the
  # foreground (the live view pauses then): grant it as soon as the install is there.
  (for _ in $(seq 90); do adb -s "$device" shell pm grant dev.composer.composer_app android.permission.POST_NOTIFICATIONS 2>/dev/null && break; sleep 2; done) &
  if ! flutter test "integration_test/${t}_test.dart" -d "$device" "${defines[@]}"; then
    failed+=("$t")
  fi
done
[ ${#failed[@]} -eq 0 ] && echo "all passed: ${tests[*]}" || { echo "failed: ${failed[*]}"; exit 1; }
