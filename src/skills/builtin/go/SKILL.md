# Go modules and workspaces

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Inspect module boundaries

Read go.mod, go.sum, go.work if present, toolchain directives, build tags, generated-file notices, and nearby tests. Identify the affected module and whether workspace replacements change dependency resolution. A nested-only module may need explicit skill selection.

Preserve context cancellation, error wrapping, ownership of goroutines/channels, and interfaces already used by callers. For concurrency fixes, establish the actual race or lifecycle failure. Do not add goroutines or broad retry loops solely to hide blocking behavior.

Choose package-level go test and repository formatting/static checks from the configured environment. Race tests require a supported host/compiler and may not represent every deployment target. Module downloads, toolchain auto-downloads, go generate, and generated-code changes are effects requiring explicit task relevance and authority.

## Report

Record Go version, module/workspace, build tags and commands. Preserve sums for source-only changes. If dependencies or native prerequisites are absent, distinguish unrun compilation, race checks and integration services.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
