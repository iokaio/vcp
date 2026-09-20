# C and C++ native builds

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Establish the native build

Inspect CMakePresets.json/CMakeLists.txt or the actual MSBuild/Make configuration, toolchain files, compiler flags, dependency manager settings, and existing build instructions. Identify architecture, generator, build type, language standard, and the affected targets. Do not mix an existing Ninja build directory with another generator or compiler.

Review lifetime, ownership, bounds, undefined behavior, ABI, exceptions, and platform-specific handles. Fit existing resource-management and error conventions. A fix to a header or compile definition may affect more targets than the edited source file suggests.

Prefer the declared preset/target and test registration, for example the applicable CMake build and CTest selection, rather than inventing flags. Sanitizers and cross-compilation need compatible toolchains; record their limits. Configure steps and dependency acquisition can execute or download code and need authority.

## Evidence

Report compiler/generator/architecture and actual target/test results. Do not claim a syntax-only or alternate-platform build proves native runtime correctness. Preserve existing build trees and user toolchain settings.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
