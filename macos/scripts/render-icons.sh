#!/usr/bin/env bash
# Render every app-icon PNG from the SVG sources in packaging/icons/.
# ctail-small.svg (fewer, bolder tail lines) is used up to 32 px, where the full art turns into a dot.
set -euo pipefail
cd "$(dirname "$0")/../.."
ICONS=packaging/icons
SET=macos/Assets.xcassets/AppIcon.appiconset
BIN=$(mktemp -d)/svg2png
trap 'rm -rf "$(dirname "$BIN")"' EXIT
swiftc -O macos/scripts/svg2png.swift -o "$BIN"

render() { # <px> <out>
  local src=$ICONS/ctail.svg
  [ "$1" -le 32 ] && src=$ICONS/ctail-small.svg
  "$BIN" "$src" "$2" "$1"
}

for sz in 16 32 128 256 512; do
  render "$sz" "$SET/icon_${sz}.png"
  render "$((sz*2))" "$SET/icon_${sz}@2x.png"
done
render 1024 macos/Sources/ctailmac/Resources/appicon.png
"$BIN" "$ICONS/ctail-small.svg" "$ICONS/ctail-48.png" 48

# The site rounds and shadows its logos itself, so they are cropped to the tile, without Apple's margin.
TMP=$(dirname "$BIN")
site() { # <svg> <px> <out>
  sed 's/viewBox="0 0 1024 1024"/viewBox="100 100 824 824"/' "$ICONS/$1" > "$TMP/site.svg"
  "$BIN" "$TMP/site.svg" "site/static/$3" "$2"
}
site ctail.svg 1024 logo.png
site ctail.svg 256 logo-256.png
site ctail-small.svg 64 favicon.png
echo "icons rendered"
