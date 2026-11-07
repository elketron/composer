#!/bin/sh
# Installs this bundle for the current user: ~/.local/lib/composer-desktop, a link in ~/.local/bin,
# the menu entry and its icon.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
lib="$HOME/.local/lib/composer-desktop"
rm -rf "$lib" && mkdir -p "$lib" "$HOME/.local/bin" "$HOME/.local/share/applications" \
  "$HOME/.local/share/icons/hicolor/256x256/apps"
# Everything but the installer's own files: the binary and the CEF runtime beside it.
for f in "$here"/*; do
  case "$(basename "$f")" in install.sh|README.txt|*.desktop|*.png) ;; *) cp -r "$f" "$lib/" ;; esac
done
ln -sf "$lib/composer-desktop" "$HOME/.local/bin/composer-desktop"
cp "$here/composer-desktop.desktop" "$HOME/.local/share/applications/"
cp "$here/composer-desktop.png" "$HOME/.local/share/icons/hicolor/256x256/apps/"
echo "installed: ~/.local/bin/composer-desktop"
