#!/usr/bin/env bash
# Ticket 0206: the desktop module split. For every module, its size and what it imports, so the
# UI-framework-bound part can be separated from plain logic by line count.
#   bash module-split.sh <repo root> <out file>
set -uo pipefail
root="${1:?usage: module-split.sh <repo root> <out>}"
out="${2:?}"
cd "$root" || exit 1

mods() { # <glob> <label>
  echo
  echo "# $2"
  echo "# command: wc -l <dir>/*.rs ; grep -E '^use |^pub use ' per file, first path segment kept"
  printf '%-24s %6s  %s\n' module lines 'imported crate roots'
  for f in $1; do
    n=$(basename "$f" .rs)
    lines=$(wc -l < "$f")
    imports=$(grep -hE '^[[:space:]]*(pub )?use [a-z_:]' "$f" \
      | sed -E 's/^[[:space:]]*(pub )?use //' \
      | awk -F'[:; ]' '{print $1}' \
      | grep -vE '^(self|super|crate)$' | sort -u | tr '\n' ' ')
    printf '%-24s %6s  %s\n' "$n" "$lines" "$imports"
  done
}

{
  echo '# Desktop module split (ticket 0206)'
  mods 'apps/desktop/src/*.rs' 'apps/desktop: every module'
  mods 'crates/client/src/*.rs crates/client/src/cli/*.rs' 'crates/client: the portable-core candidate'

  echo
  echo '# Totals by what a module imports'
  echo 'gpui-bound lines:  '$(grep -rlE '^use gpui|^pub use gpui' apps/desktop/src | xargs -r cat | wc -l)
  echo 'cef-bound lines:   '$(grep -rlE '^use cef|^pub use cef' apps/desktop/src | xargs -r cat | wc -l)
  echo 'wezterm-bound:     '$(grep -rlE '^use wezterm|wezterm::' apps/desktop/src | xargs -r cat | wc -l)
  echo 'all desktop lines: '$(cat apps/desktop/src/*.rs | wc -l)
  echo
  echo '# modules with no reference anywhere to the UI framework, CEF, the terminal, or tokio'
  echo '# command: grep -q for gpui:: / cef:: / wezterm / tokio / notify_rust / open:: in the file'
  plain_total=0
  for f in apps/desktop/src/*.rs; do
    if ! grep -qE '(^|[^a-z_])(gpui|cef|wezterm|tokio|tokio_tungstenite|notify_rust|open)::' "$f"; then
      n=$(wc -l < "$f")
      plain_total=$((plain_total + n))
      printf '%-24s %6s\n' "$(basename "$f" .rs)" "$n"
    fi
  done
  echo "plain-logic total: $plain_total lines of $(cat apps/desktop/src/*.rs | wc -l)"
} > "$out" 2>&1
echo "wrote $out"
