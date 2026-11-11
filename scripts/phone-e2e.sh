#!/bin/bash
# Runs the phone app's end-to-end tests (apps/composer_app/integration_test) on an Android device
# or emulator against a test server on this machine. Each test is a fresh install, so each gets
# its own pairing code; the server's port is forwarded into the device (`adb reverse`).
#
#   COMPOSER_HOME=<test home> scripts/phone-e2e.sh <device> http://127.0.0.1:7531/w/default [test...]
#
# A second workspace of the home is paired too (PAIR_LINE_2) when there is one: the pairing test
# pairs both and switches between them.
#
# A headless emulator: `emulator -avd <name> -read-only -no-window -no-audio`. Tests default to
# pairing, chat and reader; chat needs a `tools` site with `greet name:` printing `zebra-42` and a
# model that runs it, reader the orchestration app built, workflow a `demo` site (`app: demoapp`)
# with `[applications.demoapp]` and a `[viewer]` pointing at a running wayai. Chat also needs the greet recipe to be
# unapproved: use a fresh home, or drop it from state/tool_rules.json and restart the server.
set -euo pipefail
device=${1:?usage: phone-e2e.sh <device> <workspace url> [test...]}
server=${2:?usage: phone-e2e.sh <device> <workspace url> [test...]}
shift 2
tests=("$@")
[ ${#tests[@]} -eq 0 ] && tests=(pairing desk chat reader workflow)
composer=${COMPOSER:-composer}
port=$(sed -E 's#^[a-z]+://[^:/]+:([0-9]+).*#\1#' <<<"$server")
home=${COMPOSER_HOME:-$HOME/.composer}
workspace=$(basename "$server")
base=${server%/w/*}
other=$(for dir in "$home"/workspaces/*/; do name=$(basename "$dir"); [ "$name" != "$workspace" ] && echo "$name" && break; done || true)
adb -s "$device" reverse "tcp:$port" "tcp:$port" >/dev/null
cd "$(dirname "$0")/../apps/composer_app"

pair_code() {
  "$composer" -w "$1" pair --name phone-e2e 2>/dev/null \
    | sed -nE 's/^computer: +composer login [^ ]+ ([A-Z0-9-]+)$/\1/p' | tail -1
}

failed=()
for t in "${tests[@]}"; do
  code=$(pair_code "$workspace")
  defines=(--dart-define=PAIR_LINE="composer login $server $code")
  # SHOT_PAUSE=<seconds>: tests stop at their `shot`s (support.dart) so a watcher can screenshot them.
  [ -n "${SHOT_PAUSE:-}" ] && defines+=(--dart-define=SHOT_PAUSE="$SHOT_PAUSE")
  if [ -n "$other" ]; then
    code2=$(pair_code "$other")
    defines+=(--dart-define=PAIR_LINE_2="composer login $base/w/$other $code2")
  fi
  echo "== $t"
  # A fresh install asks for the notification permission, and the dialog takes the app out of the
  # foreground (the live view pauses then): grant it as soon as the install is there.
  (for _ in $(seq 90); do adb -s "$device" shell pm grant dev.composer.composer_app android.permission.POST_NOTIFICATIONS 2>/dev/null && break; sleep 2; done) &
  if ! flutter test "integration_test/${t}_test.dart" -d "$device" "${defines[@]}"; then
    failed+=("$t")
  fi
done
[ ${#failed[@]} -eq 0 ] && echo "all passed: ${tests[*]}" || { echo "failed: ${failed[*]}"; exit 1; }
