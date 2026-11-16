#!/usr/bin/env bash
# Ticket 0206, appended to logs/signing-and-tools.txt: the narrowed signing question.
set -uo pipefail
root="${1:?}"; log="${2:?}"
cd "$root" || exit 1
{
  echo
  echo '### narrowed: Windows code-signing tools only, vendored trees excluded'
  echo '# command: grep -rniE "codesign|signtool|authenticode" .github scripts docs'
  grep -rniE 'codesign|signtool|authenticode' .github scripts docs 2>/dev/null
  echo "(no output above = the repository never signs a Windows binary)"
  echo
  echo '### the only signing step in CI is the Android keystore:'
  grep -n -A6 'Signing key from secrets' .github/workflows/release.yml
} >> "$log" 2>&1
