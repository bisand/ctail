#!/usr/bin/env bash
# Build AppIcon.icns from a 1024x1024 source PNG using sips + iconutil.
set -euo pipefail
SRC="${1:-Resources/appicon.png}"
OUT="${2:-.build/AppIcon.icns}"
SET="$(dirname "$OUT")/AppIcon.iconset"

rm -rf "$SET"; mkdir -p "$SET"
for sz in 16 32 64 128 256 512; do
  sips -z "$sz" "$sz"        "$SRC" --out "$SET/icon_${sz}x${sz}.png"   >/dev/null
  sips -z "$((sz*2))" "$((sz*2))" "$SRC" --out "$SET/icon_${sz}x${sz}@2x.png" >/dev/null
done
iconutil -c icns "$SET" -o "$OUT"
rm -rf "$SET"
echo "wrote $OUT"
