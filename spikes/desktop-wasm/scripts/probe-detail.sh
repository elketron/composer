#!/usr/bin/env bash
# Ticket 0206: for one probe log, show the failing crate(s) and the first real error, plus which
# dependency of the probe pulls it. Usage: probe-detail.sh <probe workspace> <log> <crate...>
set -uo pipefail
probe="${1:?usage: probe-detail.sh <probe ws> <log> <crate...>}"
log="${2:?}"
shift 2
echo "#### $log"
grep -n 'could not compile\|failed to run custom build command\|^error' "$log" | cut -c1-240 | head -12
echo
echo '--- the error context of the first error ---'
n=$(grep -n '^error' "$log" | head -1 | cut -d: -f1)
[ -n "$n" ] && sed -n "$((n > 2 ? n - 2 : 1)),$((n + 14))p" "$log"
echo
cd "$probe" || exit 1
for c in "$@"; do
  echo "--- who pulls $c (in $(basename "$probe")) ---"
  cargo tree -i "$c" --target wasm32-unknown-unknown 2>&1 | head -18
done
