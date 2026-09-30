# C and C++ native builds

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Read the project

- `CMakeLists.txt` (root and per-directory) and `CMakePresets.json`/`CMakeUserPresets.json`: targets, options, language standard, configure/build/test presets.
- `compile_commands.json`: the exact flags, defines and include paths used per translation unit.
- Existing build directories (`build/`, `out/`, `cmake-build-*`) and their `CMakeCache.txt`: the generator, compiler, build type and architecture already chosen.
- `vcpkg.json`, `conanfile.txt`/`conanfile.py`: dependency manifests and triplets/profiles.
- Alternatives: `meson.build`, `BUILD`/`MODULE.bazel`, `.sln`/`.vcxproj`, plain Makefiles or autotools `configure.ac`.
- Toolchain files, compiler flags and existing build instructions.

Identify architecture, generator, build type, language standard, and the affected targets. Do not mix an existing Ninja build directory with another generator or compiler.

## Discover commands in this order

1. Project-declared entry points: Makefile/justfile, scripts, CI workflows, CONTRIBUTING, AGENTS.md, named presets.
2. The existing configured build directory, reused with its generator.
3. Ecosystem defaults, as candidates only when evidence supports them:
   - `cmake --preset <name>` then `cmake --build --preset <name> --target <target>`
   - `cmake --build <build-dir> --target <target>`
   - `ctest --test-dir <build-dir> -R <pattern> --output-on-failure` (add `-C <config>` for multi-config generators)
   - `meson test -C <build-dir> <name>`, `bazel test //path:target`, `msbuild <project> /p:Configuration=<config>`

Prefer the declared preset/target and test registration rather than inventing flags. Configure steps and dependency acquisition can execute or download code and need authority.

## Toolchain variants

- Single-config (Ninja, Makefiles) vs multi-config (Visual Studio, Ninja Multi-Config, Xcode) generators.
- MSVC, clang-cl, MinGW GCC, Clang and GCC, each with different flags, warnings and ABIs.
- vcpkg manifest vs classic mode; Conan profiles; system packages.
- Test frameworks: GoogleTest, Catch2, doctest, Boost.Test, or custom executables registered with `add_test`.
- Sanitizers and cross-compilation need compatible toolchains; record their limits.

## Coding rules

Review lifetime, ownership, bounds, undefined behavior, ABI, exceptions, and platform-specific handles. Fit existing resource-management and error conventions. A fix to a header or compile definition may affect more targets than the edited source file suggests.

## Verification evidence

- Build the affected target, then run its registered tests; broaden to dependents when headers, defines or shared libraries changed.
- Run `clang-format`, `clang-tidy` or other checks only if the project configures them (`.clang-format`, `.clang-tidy`, CI steps).
- Report compiler/generator/architecture and actual target/test results. Do not claim a syntax-only or alternate-platform build proves native runtime correctness.
- Preserve existing build trees and user toolchain settings.

## Pitfalls

- MSVC needs a Developer Command Prompt or `vcvarsall` environment; running from a plain shell finds the wrong or no compiler. Do not mix MSVC and MinGW objects or runtimes.
- Reconfiguring an existing build directory with a different generator or compiler fails or corrupts the cache.
- Multi-config builds ignore `CMAKE_BUILD_TYPE`; pass the configuration to build and test.
- Windows is case-insensitive while Linux is not; `#include` case mismatches build only on one.
- Backslashes in paths, CRLF in generated files, and long paths in deep build trees break tools.
- Running executables or antivirus scanning lock `.exe`/`.dll`/`.pdb` files and cause link failures.
- Warnings-as-errors can differ between compilers; check the flags before concluding.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
