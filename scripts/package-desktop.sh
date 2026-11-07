#!/bin/bash
# Builds the desktop app and packs it as dist/composer-desktop-<version>-linux-x86_64.tar.gz:
# the binary, the CEF runtime beside it (found through $ORIGIN), install.sh, the menu entry.
#   scripts/package-desktop.sh <version>
set -euo pipefail
version=${1:?usage: package-desktop.sh <version>}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root/apps/desktop"
scripts/fetch-cef.sh
COMPOSER_PACKAGE=1 cargo build --release --locked
name="composer-desktop-$version-linux-x86_64"
out="$root/dist/$name"
rm -rf "$out" && mkdir -p "$out"
cp target/release/composer-desktop "$out/"
# The runtime only: not CEF's headers, wrapper sources or build files.
cef=$(readlink -f .cef)
for f in "$cef"/*; do
  case "$(basename "$f")" in include|cmake|libcef_dll|CMakeLists.txt|archive.json|CREDITS.html) ;; *) cp -r "$f" "$out/" ;; esac
done
# CEF's library ships with its debug symbols (1.4 GB): without them it is a fraction.
strip --strip-unneeded "$out/libcef.so" "$out/composer-desktop"
cp "$cef/CREDITS.html" "$out/CEF-CREDITS.html"
cp assets/fonts/OFL.txt "$out/FONT-LICENSE.txt"
cp packaging/composer-desktop.desktop packaging/composer-desktop.png packaging/install.sh \
  packaging/README.txt "$out/"
tar -C "$root/dist" -czf "$out.tar.gz" "$name"
echo "$out.tar.gz"
