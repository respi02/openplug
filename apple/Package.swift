// swift-tools-version:5.9
import PackageDescription

// The Rust core is linked from OpenPlugCoreFFI.xcframework; run ./build-core.sh first.
let package = Package(
    name: "OpenPlug",
    platforms: [.macOS(.v13), .iOS(.v16)],
    products: [
        .library(name: "OpenPlugBLE", targets: ["OpenPlugBLE"]),
        .executable(name: "openplug-cli", targets: ["openplug-cli"]),
        .executable(name: "openplug-fake-ble", targets: ["openplug-fake-ble"]),
    ],
    targets: [
        // The Rust core as static libraries, with the UniFFI C header (generated).
        .binaryTarget(name: "OpenPlugCoreFFI", path: "OpenPlugCoreFFI.xcframework"),
        // Swift bindings to the Rust core (generated).
        .target(name: "OpenPlugCore", dependencies: ["OpenPlugCoreFFI"]),
        // The Core Bluetooth transport, shared by macOS and iOS. Re-exports OpenPlugCore.
        .target(name: "OpenPlugBLE", dependencies: ["OpenPlugCore"]),
        .executableTarget(name: "openplug-cli", dependencies: ["OpenPlugBLE"]),
        // A BLE stand-in for the dongle (macOS), bridging to the core's fake_dongle.
        .executableTarget(name: "openplug-fake-ble"),
    ]
)
