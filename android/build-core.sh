#!/bin/sh
# Builds the Rust core for Android and generates the Kotlin bindings.
# Run before ./gradlew; both outputs are build products and are gitignored.
#
# Needs: rustup with the Android targets, cargo-ndk, and ANDROID_NDK_HOME.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/../core"

cargo ndk -t arm64-v8a -t armeabi-v7a -o "$here/app/src/main/jniLibs" \
    build --release --features ffi

# The bindings are read from a host build of the same library.
cargo build --features ffi
lib=$(ls target/debug/libopenplug_core.dylib target/debug/libopenplug_core.so 2>/dev/null | head -1)
cargo run --quiet --features bindgen --bin uniffi-bindgen -- generate \
    --library "$lib" --language kotlin --no-format \
    --out-dir "$here/app/src/main/java"
echo "core built: jniLibs + com/openplug/core/openplug_core.kt"
