#!/usr/bin/env bash
# Ticket 0206: append the two verifications the report quotes verbatim.
set -uo pipefail
wt="${1:?}"; dst="${2:?}"
cd "$wt" || exit 1
{
  echo
  echo '### does anything sign a Windows binary? (the release path, excluding vendored CEF/docs)'
  echo '# command: grep -rniE "codesign|signtool|authenticode" .github scripts | head'
  grep -rniE 'codesign|signtool|authenticode' .github scripts 2>/dev/null | head
  echo '# command: grep -rniE "sign" .github/workflows/release.yml scripts/package-desktop-windows.ps1'
  grep -rniE 'sign' .github/workflows/release.yml scripts/package-desktop-windows.ps1 2>/dev/null \
    | grep -viE 'design|assign|signal' | head
  echo
  echo '### the desktop-windows job, verbatim'
  sed -n '/^  desktop-windows:/,/^  publish:/p' .github/workflows/release.yml
} >> "$dst/logs/signing-and-tools.txt" 2>&1
tail -30 "$dst/logs/signing-and-tools.txt"
