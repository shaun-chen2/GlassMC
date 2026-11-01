// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "GlassMC",
    platforms: [.macOS(.v15)],
    targets: [
        .executableTarget(
            name: "GlassMC",
            path: "Sources/GlassMC"
        )
    ]
)
