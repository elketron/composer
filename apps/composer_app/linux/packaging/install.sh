#!/bin/sh
# Installs this bundle for the current user.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
lib="$HOME/.local/lib/composer-app"
rm -rf "$lib" && mkdir -p "$lib" "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/256x256/apps"
cp -r "$here/composer-app" "$here/lib" "$here/data" "$lib/"
ln -sf "$lib/composer-app" "$HOME/.local/bin/composer-app"
cp "$here/composer-app.desktop" "$HOME/.local/share/applications/"
cp "$here/composer-app.png" "$HOME/.local/share/icons/hicolor/256x256/apps/" 2>/dev/null || true
echo "installed: ~/.local/bin/composer-app"
