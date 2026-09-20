# Rust workspaces and native boundaries

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Read the workspace

Inspect the owning Cargo.toml, workspace membership, Cargo.lock, rust-toolchain file, cargo configuration, and neighboring modules/tests. Determine the actual package, target, features, target platform, and working directory. Do not invoke a different toolchain just because it is on PATH.

Keep dependency direction and existing error types. Review ownership, cancellation, synchronization, and platform-specific path/process behavior. Avoid unsafe code or generic abstractions without a concrete requirement. A borrow-checker workaround that changes lifetime or scheduling semantics needs behavioral verification.

Choose a package/test target and feature set from the project's contract. Cargo test -p <package> --test <target>, cargo fmt, or targeted Clippy may fit, but only when those targets and options are applicable. Use locked/offline modes where required and provisioned; a missing cache is not permission to fetch or rewrite the lockfile.

## Evidence

Report the selected toolchain/features, checks run, compiler or platform limits, and relevant observed results. Preserve unrelated formatting and avoid broad dependency upgrades to repair a local defect.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
