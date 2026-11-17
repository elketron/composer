#!/usr/bin/env bash
# Ticket 0206: the exact line counts for the module-split section.
#   bash counts.sh <repo root>
set -uo pipefail
cd "${1:?usage: counts.sh <repo root>}" || exit 1
src=apps/desktop/src

sum() { # <label> <names...>
  local total=0 n
  for n in "$2"; do :; done
  for n in $2; do
    total=$((total + $(wc -l < "$src/$n.rs")))
  done
  printf '%-46s %6s lines, %s modules\n' "$1" "$total" "$(echo $2 | wc -w)"
}

{
  echo '# Exact module counts for the report (wc -l per file, names listed)'
  echo '# all:'; printf '  total %s lines in %s modules\n' "$(cat $src/*.rs | wc -l)" "$(ls $src/*.rs | wc -l)"
  sum 'proven to compile for wasm as-is (p-desktop-logic):' "\
browser_tabs chat_width dom_keys draft grid link_state markdown matcher overlay_frame \
project_binding prose stage_tabs theme uploads user_review view_urls workspaces"
  sum 'framework-free but not portable as-is:' "\
key_map term_config pick_lists home_lists terminal_link notice main"
  echo
  echo '# what the framework-free-but-not-portable ones hang on (grep for the import)'
  grep -nE '^use (tattoy_wezterm_term|notify_rust)|crate::picker' \
    $src/key_map.rs $src/term_config.rs $src/pick_lists.rs $src/home_lists.rs \
    $src/terminal_link.rs $src/notice.rs 2>/dev/null | head
  echo
  echo '# the rest: which UI/native crate each binds to'
  for f in $src/*.rs; do
    n=$(basename "$f" .rs)
    case " browser_tabs chat_width dom_keys draft grid link_state markdown matcher overlay_frame project_binding prose stage_tabs theme uploads user_review view_urls workspaces key_map term_config pick_lists home_lists terminal_link notice main " in
      *" $n "*) continue;;
    esac
    binds=$(grep -hoE '^use (gpui|cef|wezterm_surface|tattoy_wezterm_term|tokio|tokio_tungstenite|futures|composer_client|composer_core|composer_protocol|smallvec|base64|open|notify_rust|image)' "$f" \
      | sed 's/^use //' | sort -u | tr '\n' ' ')
    printf '%-18s %6s  %s\n' "$n" "$(wc -l < "$f")" "$binds"
  done
  echo
  echo '# chat_pane and view_urls: what a browser shell would need anyway'
  echo "  markdown.rs (the renderer):        $(wc -l < $src/markdown.rs)"
  echo "  prose.rs (block model):            $(wc -l < $src/prose.rs)"
  echo "  view_urls.rs (pane/view URLs):     $(wc -l < $src/view_urls.rs)"
  echo "  chat_pane.rs (its drawing):        $(wc -l < $src/chat_pane.rs)"
  echo
  echo '# composer-client, per module (wc -l)'
  wc -l crates/client/src/*.rs crates/client/src/cli/*.rs | sort -n
} 2>&1
