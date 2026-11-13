// swift-tools-version:5.9
import PackageDescription

// The Rust core is linked as a static library; run ./build-core.sh first.
let package = Package(
    name: "OpenPlug",
    platforms: [.macOS(.v13), .iOS(.v16)],
    products: [
        .library(name: "OpenPlugBLE", targets: ["OpenPlugBLE"]),
        .executable(name: "openplug-cli", targets: ["openplug-cli"]),
    ],
    targets: [
        // C header + module map of the UniFFI scaffolding (generated).
        .target(name: "OpenPlugCoreFFI"),
        // Swift bindings to the Rust core (generated).
        .target(
            name: "OpenPlugCore",
            dependencies: ["OpenPlugCoreFFI"],
            linkerSettings: [
                .unsafeFlags(["-L", "lib"]),
                .linkedLibrary("openplug_core"),
            ]
        ),
        // The Core Bluetooth transport, shared by macOS and iOS.
        .target(name: "OpenPlugBLE", dependencies: ["OpenPlugCore"]),
        .executableTarget(name: "openplug-cli", dependencies: ["OpenPlugBLE"]),
    ]
)
