#!/bin/sh
# Builds the Rust core for macOS, iOS and the iOS simulator (arm64), packs it into
# OpenPlugCoreFFI.xcframework and generates the Swift bindings.
# Run before `swift build` or opening the Xcode project; the outputs are build
# products and are gitignored.
#
# Needs rustup with the targets aarch64-apple-darwin, aarch64-apple-ios and
# aarch64-apple-ios-sim.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/../core"

targets="aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim"
for t in $targets; do
    cargo build --release --features ffi --target "$t"
done

tmp=$(mktemp -d)
cargo run --quiet --features bindgen --bin uniffi-bindgen -- generate \
    --library target/aarch64-apple-darwin/release/libopenplug_core.dylib \
    --language swift --no-format --out-dir "$tmp"
mkdir -p "$tmp/Headers"
cp "$tmp/OpenPlugCoreFFI.h" "$tmp/Headers/"
cp "$tmp/OpenPlugCoreFFI.modulemap" "$tmp/Headers/module.modulemap"
cp "$tmp/OpenPlugCore.swift" "$here/Sources/OpenPlugCore/"

rm -rf "$here/OpenPlugCoreFFI.xcframework"
args=""
for t in $targets; do
    args="$args -library target/$t/release/libopenplug_core.a -headers $tmp/Headers"
done
# shellcheck disable=SC2086
xcodebuild -create-xcframework $args -output "$here/OpenPlugCoreFFI.xcframework" >/dev/null
rm -rf "$tmp"
echo "core built: OpenPlugCoreFFI.xcframework + Swift bindings"
