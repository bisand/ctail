#!/usr/bin/env bash
# Builds the Rust engine (core/) for the macOS app:
#   1. cargo-builds libctail_core.a for each target and lipo's them together,
#   2. generates the Swift bindings with uniffi-bindgen into Sources/CtailCore/,
#   3. wraps the library + C header + modulemap as Frameworks/CtailCoreFFI.xcframework.
# Both SwiftPM (Package.swift) and the xcodegen project (project.yml) consume
# the results. Everything it writes is git-ignored; run it before `swift build`.
#
#   PROFILE=release|debug   (default release — the engine is the hot path)
#   CORE_TARGETS="aarch64-apple-darwin x86_64-apple-darwin"   (default: both)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=..
PROFILE=${PROFILE:-release}
CORE_TARGETS=${CORE_TARGETS:-"aarch64-apple-darwin x86_64-apple-darwin"}
OUT=.build/core
CARGO_FLAGS=()
[ "$PROFILE" = release ] && CARGO_FLAGS+=(--release)

rm -rf "$OUT"; mkdir -p "$OUT/lib" "$OUT/include" "$OUT/gen"
LIBS=()
for t in $CORE_TARGETS; do
  rustup target list --installed | grep -qx "$t" || rustup target add "$t"
  # ${a[@]} on an empty array is "unbound" under this bash and `set -u`, which
  # is exactly what PROFILE=debug leaves behind.
  (cd "$ROOT" && cargo build -q ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} -p ctail-core --features ffi --target "$t")
  LIBS+=("$ROOT/target/$t/$PROFILE/libctail_core.a")
done
if [ ${#LIBS[@]} -eq 1 ]; then cp "${LIBS[0]}" "$OUT/lib/libctail_core.a"
else lipo -create "${LIBS[@]}" -output "$OUT/lib/libctail_core.a"; fi

(cd "$ROOT" && cargo run -q -p ctail-core --features cli --bin uniffi-bindgen -- \
  generate --library "macos/$OUT/lib/libctail_core.a" --language swift --out-dir "macos/$OUT/gen")
mkdir -p Sources/CtailCore
cp "$OUT/gen/CtailCore.swift" Sources/CtailCore/CtailCore.swift
cp "$OUT/gen/CtailCoreFFI.h" "$OUT/include/CtailCoreFFI.h"
cp "$OUT/gen/CtailCoreFFI.modulemap" "$OUT/include/module.modulemap"

rm -rf Frameworks/CtailCoreFFI.xcframework; mkdir -p Frameworks
xcodebuild -quiet -create-xcframework -library "$OUT/lib/libctail_core.a" -headers "$OUT/include" \
  -output Frameworks/CtailCoreFFI.xcframework
echo "core: $(lipo -archs "$OUT/lib/libctail_core.a") ($PROFILE) -> Frameworks/CtailCoreFFI.xcframework, Sources/CtailCore/CtailCore.swift"
