# Dart and Flutter projects

Original VCP guidance.

## Read the project

- `pubspec.yaml`: package name, SDK constraints, dependencies, `dev_dependencies`, and whether a `flutter:` section or `sdk: flutter` dependency makes this a Flutter project.
- `pubspec.lock`: resolved versions; keep changes attributable to a dependency task.
- `analysis_options.yaml`: included lint sets and analyzer rules the code must satisfy.
- Workspace layout: `packages/` with a `melos.yaml` or a pub workspace declaration indicates a multi-package repository.
- Flutter platform folders (`android/`, `ios/`, `web/`, `windows/`, `macos/`, `linux/`): which platforms are supported; do not infer that every pubspec permits device-based tests.
- `build.yaml` and generated files (`*.g.dart`, `*.freezed.dart`, `*.mocks.dart`): build_runner code generation.
- `test/`, `integration_test/` and `test_driver/`: unit/widget vs device integration tests.

## Discover commands in this order

1. Project-declared entry points: Makefile/justfile, melos scripts, CI workflows, CONTRIBUTING, AGENTS.md.
2. Ecosystem defaults, as candidates only when evidence supports them and packages are already resolved:
   - `dart analyze` or `flutter analyze --no-pub`
   - `dart test <test/file_test.dart>` or `--name <pattern>` for pure Dart
   - `flutter test --no-pub <test/file_test.dart>` for Flutter unit and widget tests
   - `dart format --output=none --set-exit-if-changed <paths>` to check formatting
   - `dart run build_runner build` only when the project uses it and authority covers regenerating files

Select the declared analyzer, Dart test, Flutter unit/widget test, or integration target using the pinned installed SDK. Pub resolution, SDK downloads, device startup and external-service tests require explicit prerequisites and authority. `flutter test`, `flutter analyze` and other Flutter commands run `pub get` implicitly unless given `--no-pub`, and a first `flutter` run may download engine artifacts. `dart run` and `dart test` may resolve packages when `.dart_tool/package_config.json` is missing or stale; check it exists before running them.

## Toolchain variants

- Pure Dart packages, CLIs or servers vs Flutter apps and plugins.
- Standalone Dart SDK vs the Dart SDK bundled in Flutter; version managers such as FVM (`.fvmrc`).
- Melos or pub workspaces for monorepos.
- State management and codegen stacks (Riverpod, Bloc, Provider, freezed, json_serializable).

## Coding rules

Preserve null-safety, async disposal/cancellation, state-management conventions, widget semantics and package APIs. Follow the repository's generation process for generated files instead of hand-editing output or invoking a generator without inspecting its effects.

## Verification evidence

- Run the test file covering the change, then the package suite, then dependent packages when a shared package changed.
- Run the analyzer and format check the project uses; fix new diagnostics rather than suppressing them.
- A widget test does not establish device integration correctness.
- Return package/SDK identity, focused checks and observed outcomes. If a Flutter engine, emulator or platform SDK is unavailable, report those checks not run and keep useful analysis separate from execution claims.

## Pitfalls

- On Windows, `flutter` and `dart` are batch wrappers; use the SDK already on the path rather than a different copy, and note Developer Mode or Visual Studio requirements for Windows desktop builds.
- iOS and macOS builds require macOS with Xcode; report them as not run elsewhere.
- Stale generated files cause confusing analyzer errors; check whether generation is out of date before editing code around them.
- `pubspec.lock` and `.dart_tool/` change on resolution; do not commit incidental updates.
- CRLF endings can fail formatting checks and golden-file comparisons.
- File locking on Windows can block rebuilding `build/` while an app or IDE runs.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
