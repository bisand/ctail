// swift-tools-version:5.9
import PackageDescription

// The Rust engine (core/) is consumed as a static XCFramework plus generated
// Swift bindings; both are produced by scripts/build-core.sh (git-ignored).
let package = Package(
    name: "ctailmac",
    platforms: [.macOS(.v13)],
    targets: [
        .binaryTarget(
            name: "CtailCoreFFI",
            path: "Frameworks/CtailCoreFFI.xcframework"
        ),
        .target(
            name: "CtailCore",
            dependencies: ["CtailCoreFFI"],
            path: "Sources/CtailCore"
        ),
        .executableTarget(
            name: "ctailmac",
            dependencies: ["CtailCore"],
            path: "Sources/ctailmac",
            resources: [.process("Resources/appicon.png")]
        )
    ]
)
