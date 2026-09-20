# Dart and Flutter projects

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Identify the package and SDK

Read pubspec.yaml/pubspec.lock, SDK constraints, analysis_options.yaml, package/workspace layout, and existing tests. Establish whether this is Dart-only or Flutter and which platforms are supported; do not infer that every pubspec permits device-based tests.

Preserve null-safety, async disposal/cancellation, state-management conventions, widget semantics and package APIs. Follow the repository's generation process for generated files instead of hand-editing output or invoking a generator without inspecting its effects.

Select the declared analyzer, Dart test, Flutter unit/widget test, or integration target using the pinned installed SDK. Pub resolution, SDK downloads, device startup and external-service tests require explicit prerequisites and authority. A widget test does not establish device integration correctness.

## Results

Return package/SDK identity, focused checks and observed outcomes. If a Flutter engine, emulator or platform SDK is unavailable, report those checks not run and keep useful analysis separate from execution claims.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
