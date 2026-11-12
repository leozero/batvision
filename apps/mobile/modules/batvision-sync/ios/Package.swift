// swift-tools-version: 5.9
// Unit tests of the engine's pure logic, run on the Mac with `swift test` (D91). The app
// itself builds these files through BatvisionSync.podspec.
import PackageDescription

let package = Package(
  name: "BatvisionSyncCore",
  platforms: [.macOS(.v13)],
  targets: [
    .target(name: "BatvisionSyncCore", path: "Core"),
    .testTarget(name: "BatvisionSyncCoreTests", dependencies: ["BatvisionSyncCore"], path: "CoreTests"),
  ]
)
