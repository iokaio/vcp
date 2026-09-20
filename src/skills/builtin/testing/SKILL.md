# Targeted testing and verification

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Discover the check contract

Read project manifests, test configuration, CI commands, and neighboring tests. Identify the runner, working directory, version/features, required services, and inputs. Separate unit, integration, end-to-end, build, lint, and type checks: one passing category does not establish another.

Choose the smallest check that exercises the changed boundary. A regression should fail for the defect's observable consequence, not mirror implementation details. Reproduce a valid failure before changing its expectation. If a test conflicts with the task contract, resolve the evidence; do not skip it to obtain green output.

Broaden testing when shared contracts change, a focused failure exposes wider risk, or acceptance requires it. Do not repeatedly run an unchanged expensive suite without a reason. Treat skipped tests, timeouts, infrastructure failures, and missing services as distinct outcomes.

## Preserve verification identity

Record the exact command, working directory, exit status, result counts, and relevant source revision. Edits after a passing run invalidate checks that depend on those edits. Report not-run checks explicitly; a zero-test run is not coverage of the intended behavior.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
