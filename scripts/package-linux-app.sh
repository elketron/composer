#!/bin/bash
# Builds the Linux app and packs it as dist/composer-app-<version>-linux-x86_64.tar.gz.
# Needs flutter, and the system libraries the app links (WPE WebKit with WPEPlatform, GTK 3,
# libsecret): the release workflow runs it in an Arch Linux container.
set -euo pipefail
version=${1:?usage: package-linux-app.sh <version>}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root/apps/composer_app"
flutter build linux --release --build-name="${version%%-*}"
name="composer-app-$version-linux-x86_64"
out="$root/dist/$name"
rm -rf "$out" && mkdir -p "$out"
cp -r build/linux/x64/release/bundle/. "$out/"
# The webview plugin bundles the build machine's WPE WebKit. WebKit starts its helper processes
# from the system's install, so a bundled copy breaks once the system's WPE is updated: use the
# system's library (required anyway, see README.txt).
rm -f "$out"/lib/libWPEWebKit-2.0.so* "$out"/lib/libwpe-1.0.so*
cp linux/packaging/composer-app.desktop linux/packaging/README.txt linux/packaging/install.sh "$out/"
cp linux/packaging/composer-app.png "$out/"
tar -C "$root/dist" -czf "$out.tar.gz" "$name"
echo "$out.tar.gz"
