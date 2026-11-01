#!/bin/bash
# Renders the app's icons from apps/composer_app/assets/icon/*.svg: Android (legacy and adaptive),
# Windows (.ico) and Linux (the .desktop icon). Needs rsvg-convert and ImageMagick (magick).
set -euo pipefail
app=$(cd "$(dirname "$0")/../apps/composer_app" && pwd)
icon=$app/assets/icon
res=$app/android/app/src/main/res
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Android: 48dp legacy icons and 108dp adaptive foregrounds, per density.
for d in mdpi:1 hdpi:1.5 xhdpi:2 xxhdpi:3 xxxhdpi:4; do
  name=${d%%:*} scale=${d##*:}
  legacy=$(awk "BEGIN{print int(48*$scale)}")
  adaptive=$(awk "BEGIN{print int(108*$scale)}")
  rsvg-convert -w "$legacy" -h "$legacy" "$icon/icon.svg" -o "$res/mipmap-$name/ic_launcher.png"
  rsvg-convert -w "$adaptive" -h "$adaptive" "$icon/foreground.svg" -o "$res/mipmap-$name/ic_launcher_foreground.png"
done

# Windows: one .ico with the usual sizes.
for s in 16 24 32 48 64 128 256; do rsvg-convert -w $s -h $s "$icon/icon.svg" -o "$tmp/$s.png"; done
magick "$tmp"/{16,24,32,48,64,128,256}.png "$app/windows/runner/resources/app_icon.ico"

# Linux: installed as hicolor/256x256/apps/composer-app.png by the tarball's install.sh.
rsvg-convert -w 256 -h 256 "$icon/icon.svg" -o "$app/linux/packaging/composer-app.png"
