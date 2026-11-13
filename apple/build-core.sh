#!/bin/sh
# Builds the Rust core for the host Mac and generates the Swift bindings.
# Run before `swift build`; the outputs are build products and are gitignored.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/../core"

cargo build --release --features ffi
mkdir -p "$here/lib"
cp target/release/libopenplug_core.a "$here/lib/"

tmp=$(mktemp -d)
cargo run --quiet --features bindgen --bin uniffi-bindgen -- generate \
    --library target/release/libopenplug_core.dylib --language swift --no-format \
    --out-dir "$tmp"
cp "$tmp/OpenPlugCore.swift" "$here/Sources/OpenPlugCore/"
cp "$tmp/OpenPlugCoreFFI.h" "$here/Sources/OpenPlugCoreFFI/include/"
cp "$tmp/OpenPlugCoreFFI.modulemap" "$here/Sources/OpenPlugCoreFFI/include/module.modulemap"
rm -rf "$tmp"
echo "core built: lib/libopenplug_core.a + Swift bindings"
