# Swift and SwiftPM projects

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Determine supported targets

Read Package.swift, Package.resolved, tool-version declaration, targets/products, conditional platform code, and nearby XCTest or testing-library usage. A package manifest is executable Swift configuration; inspect it as source before authorizing a build command.

Respect declared deployment targets, concurrency isolation, ownership, optionals, error propagation and API availability. Changes to shared targets must account for their declared platforms. Do not claim that Windows execution validates Apple-only frameworks or simulator behavior.

If the required Swift toolchain and dependencies are already configured, select the documented swift build/test target or test filter. Package resolution and plugin execution may require network/process authority. Do not bootstrap toolchains, fetch packages, or run build plugins merely to make a check available.

## Evidence

Provide useful source analysis and generation even when platform checks are unavailable. Report the exact unsupported SDK/target or missing toolchain, which checks were not run, and what platform-specific validation remains.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
