#!/usr/bin/env bash
# Puts the CEF runtime the desktop builds and runs against in one shared place, and links it as
# apps/desktop/.cef (what .cargo/config.toml points CEF_PATH at). Without this, the `cef` crate
# downloads its own 1.5 GB copy into every build directory.
#   scripts/fetch-cef.sh            (CEF_DIR overrides ~/.local/share/composer/cef/<version>)
set -euo pipefail
cd "$(dirname "$0")/.."
version=156.0.15                        # CEF of the `cef` crate in Cargo.toml (156.3.x)
tool_version=156.3.1+156.0.15           # export-cef-dir for that CEF
dir="${CEF_DIR:-$HOME/.local/share/composer/cef/$version}"
if [ ! -f "$dir/libcef.so" ] && [ ! -f "$dir/libcef.dll" ]; then
  if ! command -v export-cef-dir >/dev/null; then
    cargo install --locked export-cef-dir --version "$tool_version"
  fi
  mkdir -p "$(dirname "$dir")"
  export-cef-dir --force "$dir"
fi
ln -sfn "$dir" .cef
echo "CEF: $dir (linked as apps/desktop/.cef)"
