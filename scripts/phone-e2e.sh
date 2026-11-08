#!/bin/bash
# Runs the phone app's end-to-end tests (apps/composer_app/integration_test) on an Android device
# or emulator against a test server on this machine. Each test is a fresh install, so each gets
# its own pairing code; the server's port is forwarded into the device (`adb reverse`).
#
#   COMPOSER_HOME=<test home> scripts/phone-e2e.sh <device> http://127.0.0.1:7531/w/default [test...]
#
# A headless emulator: `emulator -avd <name> -read-only -no-window -no-audio`. Tests default to
# pairing, chat and reader; chat needs a `tools` site with `greet name:` printing `zebra-42` and a
# model that runs it, reader the orchestration app built. Chat also needs the greet recipe to be
# unapproved: use a fresh home, or drop it from state/tool_rules.json and restart the server.
set -euo pipefail
device=${1:?usage: phone-e2e.sh <device> <workspace url> [test...]}
server=${2:?usage: phone-e2e.sh <device> <workspace url> [test...]}
shift 2
tests=("$@")
[ ${#tests[@]} -eq 0 ] && tests=(pairing chat reader)
composer=${COMPOSER:-composer}
port=$(sed -E 's#^[a-z]+://[^:/]+:([0-9]+).*#\1#' <<<"$server")
adb -s "$device" reverse "tcp:$port" "tcp:$port" >/dev/null
cd "$(dirname "$0")/../apps/composer_app"
failed=()
for t in "${tests[@]}"; do
  code=$("$composer" pair --name phone-e2e | sed -nE 's/^computer: +composer login [^ ]+ ([A-Z0-9-]+)$/\1/p')
  echo "== $t"
  if ! flutter test "integration_test/${t}_test.dart" -d "$device" \
      --dart-define=PAIR_LINE="composer login $server $code"; then
    failed+=("$t")
  fi
done
[ ${#failed[@]} -eq 0 ] && echo "all passed: ${tests[*]}" || { echo "failed: ${failed[*]}"; exit 1; }
