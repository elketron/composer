#!/usr/bin/env bash
# Ticket 0206: summarize a cargo log — what got built, which crates failed, and the first error
# of each failure. Usage: analyze.sh <cargo log>
set -uo pipefail
log="${1:?usage: analyze.sh <cargo log>}"

echo "### log: $log"
echo "lines: $(wc -l < "$log")  built units: $(grep -c 'Checking\|Compiling' "$log")"
echo
echo '### crates that failed (cargo: could not compile)'
grep -o 'could not compile .[a-z0-9_-]*' "$log" | sed 's/could not compile //' | sort -u
echo
echo '### build-script / manifest failures'
grep -n 'failed to run custom build command\|error: failed to get\|failed to select a version\|is not supported on this platform\|no matching package' "$log" | cut -c1-200 | head -30
echo
echo '### distinct first lines of rustc errors, in order'
grep -n '^error' "$log" | sed 's/\(:[0-9]*\):.*error: /\1: /' | cut -c1-190 | awk '!seen[$0]++' | head -40
echo
echo '### which crates the errors are in (file paths in error headers)'
grep -o '/registry/src/[^/]*/[a-z0-9_.-]*-[0-9][^/]*/' "$log" | sed 's#.*/\([a-z0-9_.-]*\)-[0-9][^/]*/#\1#' | sort | uniq -c | sort -rn | head -30
