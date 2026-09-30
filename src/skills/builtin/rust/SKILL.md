# Rust workspaces and native boundaries

Original VCP guidance. This package supplies instructions, not a tool executor or authority.

## Read the project

Inspect these before choosing a command or editing code:

- `Cargo.toml` at the root and in the owning crate: `[workspace]` members and exclusions, `default-members`, `[features]`, `[workspace.dependencies]`, `[lints]`, `rust-version`, `edition`, and `[profile.*]`. The owning package name, not its directory name, is what `-p` selects.
- `Cargo.lock`: whether it is committed and must stay unchanged for source-only work.
- `rust-toolchain` or `rust-toolchain.toml`: the pinned channel, components and targets. Do not invoke a different toolchain just because it is on PATH.
- `.cargo/config.toml` (or `.cargo/config`): aliases, default target, rustflags, linker, `[env]`, and offline or vendored sources.
- `build.rs`, proc-macro crates, and `-sys` crates: native libraries, generated code and environment variables the build reads.
- `rustfmt.toml`, `clippy.toml`, `deny.toml` and neighboring modules and tests for existing conventions.

Determine the actual package, target (lib, bin, test, bench, example), feature set, target platform and working directory.

## Discover commands in this order

1. Project-declared tasks: `Makefile`, `justfile`, `Taskfile.yml`, `xtask` crates, cargo aliases in `.cargo/config.toml`, CI workflow files, CONTRIBUTING and AGENTS.md. These encode the features, flags and toolchain the project actually gates on.
2. Ecosystem defaults, only when the manifest supports them. Candidates to confirm: `cargo test -p <package>`, narrowed with `--test <target>`, `--lib` or a test-name filter; `cargo check -p <package>`; `cargo fmt --check`; `cargo clippy -p <package> --all-targets`, with the feature options CI uses.

Use `--locked` or `--offline` where the project requires them and the cache is provisioned. A missing cache is not permission to fetch or to rewrite the lockfile.

## Toolchain variants

- Workspaces with virtual manifests: commands at the root cover `default-members` or all members, which may be slow or pull unrelated native dependencies. Prefer `-p`.
- Features: `--all-features` can enable mutually exclusive or platform-only features. Mirror the combinations CI tests.
- Cross targets and `cfg` gates: code under `cfg(windows)` or `cfg(unix)` is not compiled on the other host. Say which cfg branches were compiled.
- Nightly-only features, `cargo +<channel>` overrides, and `no_std` or embedded crates with custom targets.
- Alternative runners such as `cargo nextest` or `cargo xtask`, when the project declares them.

## Engineering rules

Keep dependency direction and existing error types. Review ownership, cancellation, synchronization, and platform-specific path and process behavior. Avoid unsafe code or generic abstractions without a concrete requirement. A borrow-checker workaround that changes lifetime or scheduling semantics needs behavioral verification. Avoid `unwrap()` and `expect()` across production failure boundaries unless the project justifies them.

## Verification evidence

Run the narrowest test that exercises the change first, then broaden to the package and to dependents when a public interface changed. Run formatting and Clippy when the project uses them. Evidence is the observed command, toolchain, features, target and result, not a claim that code should compile. Report the selected toolchain and features, checks run, checks not run with the reason, and compiler or platform limits. Preserve unrelated formatting and avoid broad dependency upgrades to repair a local defect.

## Pitfalls

- A build script or proc macro failure can look like an error in unrelated code; read the first error.
- Warnings may be denied in CI through `RUSTFLAGS` or `[lints]` while passing locally.
- Doc tests run under `cargo test` but not with `--lib` or `--test`.
- Windows: a running test binary or IDE holds `.exe` and `.pdb` files, so rebuilds fail with access denied; stop the process rather than deleting the target directory. Deep `target` paths can exceed legacy path limits. MSVC and GNU targets link differently, and `-sys` crates may need tools that are absent. Path handling must accept `\` separators, drive prefixes and case-insensitive names. CRLF checkouts can break `include_str!` fixtures and snapshot tests.
- Process spawning on Windows does not resolve `.cmd` or `.bat` shims the way a shell does; do not assume a Unix shell.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
