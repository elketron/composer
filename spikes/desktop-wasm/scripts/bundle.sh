#!/usr/bin/env bash
# Ticket 0206: build a real browser bundle of the reusable logic, so the spike ends with an
# artifact and not only a type-check. Needs wasm-bindgen-cli (cargo install wasm-bindgen-cli),
# which is not installed on this machine — the second command is what a page would be served from.
#   bash bundle.sh <spike dir>
set -uo pipefail
d="${1:?usage: bundle.sh <spike dir>}"
cd "$d" || exit 1

echo '### cargo build -p p-link --target wasm32-unknown-unknown --release'
if cargo build -p p-link --target wasm32-unknown-unknown --release > logs/link-release.txt 2>&1; then
  ls -la probe3/target/wasm32-unknown-unknown/release/*.wasm 2>/dev/null \
    || ls -la target/wasm32-unknown-unknown/release/*.wasm 2>/dev/null
else
  echo 'FAIL'; grep -n 'error' logs/link-release.txt | head -8
fi
echo
echo '### the exports the browser would see (without a JS glue layer yet)'
wasm=$(find . -name 'p_link.wasm' -path '*release*' | head -1)
[ -n "$wasm" ] && echo "$wasm: $(stat -c%s "$wasm") bytes" && command -v wasm-objdump >/dev/null \
  && wasm-objdump -x "$wasm" | grep -A8 'Export' | head -12
echo
echo '### the glue step, for the record'
echo 'wasm-bindgen --target web --out-dir web target/wasm32-unknown-unknown/release/p_link.wasm'
echo '# then: python3 -m http.server in web/, and open http://127.0.0.1:8000/ with an index.html'
echo '# that imports p_link.js and calls tool_summary("open", "{\"url\":\"site://home\"}").'
