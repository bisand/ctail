// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ctailmac",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "ctailmac",
            path: "Sources/ctailmac"
        )
    ]
)
