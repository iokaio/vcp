# Swift and SwiftPM projects

Original VCP guidance.

## Read the project

- `Package.swift`: products, targets, test targets, declared `platforms`, dependencies, plugins and the `swift-tools-version` line. A package manifest is executable Swift configuration; inspect it as source before authorizing a build command.
- `Package.resolved`: pinned dependency revisions.
- `.swift-version` or tool-version declarations: the expected toolchain.
- `*.xcodeproj`, `*.xcworkspace`, `project.yml` (XcodeGen) or `Tuist/`: an Xcode-driven app whose schemes, not SwiftPM, define builds and tests.
- Conditional code (`#if os(...)`, `#if canImport(...)`, `@available`): what each platform actually compiles.
- Nearby XCTest or Swift Testing usage, which determines the test filter syntax.
- `.swiftlint.yml`, `.swift-format` or `.swiftformat`: lint and format policy.

## Discover commands in this order

1. Project-declared entry points: Makefile/justfile, scripts, CI workflows, Fastlane lanes, CONTRIBUTING, AGENTS.md.
2. Ecosystem defaults, as candidates only when evidence supports them and the toolchain and dependencies are already configured:
   - `swift build --target <Target>`
   - `swift test --filter <TestTarget>.<TestCase>`
   - `xcodebuild test -scheme <Scheme> -destination <destination>` (macOS only)
   - `swiftlint lint <paths>` or the configured formatter in check mode

Package resolution and plugin execution may require network/process authority. Do not bootstrap toolchains, fetch packages, or run build plugins merely to make a check available.

## Toolchain variants

- Pure SwiftPM packages vs Xcode projects vs mixed setups where a package is embedded in an app.
- Apple platforms (iOS, macOS, watchOS, tvOS, visionOS) vs Linux and Windows toolchains, which lack Apple frameworks.
- XCTest vs Swift Testing (`@Test`, `#expect`).
- Build tool and command plugins, which run code during the build.

## Coding rules

Respect declared deployment targets, concurrency isolation, ownership, optionals, error propagation and API availability. Changes to shared targets must account for their declared platforms. Guard platform-specific APIs with the conditions the project already uses.

## Verification evidence

- Build and test the narrowest affected target first, then the full package when shared targets changed.
- Run the configured lint or format check on changed files.
- Provide useful source analysis and generation even when platform checks are unavailable. Report the exact unsupported SDK/target or missing toolchain, which checks were not run, and what platform-specific validation remains.
- Do not claim that Windows or Linux execution validates Apple-only frameworks, UIKit/SwiftUI behavior, or simulator and device behavior. iOS and other Apple-platform targets need macOS with Xcode.

## Pitfalls

- On Windows, the Swift toolchain depends on Visual Studio components and a matching developer environment; builds from a plain shell may fail to find the SDK or linker.
- Foundation behavior differs across platforms; tests that pass on Linux or Windows may fail on Apple platforms and vice versa.
- `Package.resolved` can change on resolution; do not commit incidental updates.
- `.build/` holds locked artifacts on Windows while processes run; stale caches can mask changes.
- Case-insensitive filesystems hide file name mismatches that fail on Linux.
- CRLF endings and path separators can break plugins and resource processing.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
