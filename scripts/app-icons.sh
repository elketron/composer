#!/bin/bash
# Renders the phone app's icons from apps/composer_app/assets/icon/*.svg: Android (legacy and
# adaptive). Needs rsvg-convert.
set -euo pipefail
app=$(cd "$(dirname "$0")/../apps/composer_app" && pwd)
icon=$app/assets/icon
res=$app/android/app/src/main/res

# Android: 48dp legacy icons and 108dp adaptive foregrounds, per density.
for d in mdpi:1 hdpi:1.5 xhdpi:2 xxhdpi:3 xxxhdpi:4; do
  name=${d%%:*} scale=${d##*:}
  legacy=$(awk "BEGIN{print int(48*$scale)}")
  adaptive=$(awk "BEGIN{print int(108*$scale)}")
  rsvg-convert -w "$legacy" -h "$legacy" "$icon/icon.svg" -o "$res/mipmap-$name/ic_launcher.png"
  rsvg-convert -w "$adaptive" -h "$adaptive" "$icon/foreground.svg" -o "$res/mipmap-$name/ic_launcher_foreground.png"
done

