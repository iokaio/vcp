// swift-tools-version: 5.9
import PackageDescription
let package = Package(name: "Fixture", products: [.library(name: "Core", targets: ["Core"])], targets: [.target(name: "Core"), .testTarget(name: "CoreTests", dependencies: ["Core"])])
