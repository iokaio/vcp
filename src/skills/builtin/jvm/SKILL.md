# Java and Kotlin build conventions

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Determine the build

Read Maven POMs or Gradle settings/build files, wrapper metadata, toolchain/JVM version declarations, and the affected module's tests. Kotlin DSL build.gradle.kts or nested-only projects may require explicit selection with the current descriptor detector. Do not replace a wrapper with a global Maven/Gradle binary without validating equivalence.

Inspect the wrapper and repository configuration before execution: wrappers can download distributions and build plugins can execute arbitrary tasks. Use only the configured tool and current network/process authority. Select the module and existing test task/filter; understand whether dependent modules must also build.

Keep package/API compatibility, nullability, error and resource handling, and concurrency conventions. For Kotlin/Java interop, inspect generated/public signatures and callers rather than assuming source syntax preserves behavior.

## Verification

Report the JVM, wrapper version, module and tasks actually used. Missing JDKs, distribution caches, or credentials are explicit not-run constraints; do not retrieve credentials from project samples or initiate dependency downloads implicitly.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
