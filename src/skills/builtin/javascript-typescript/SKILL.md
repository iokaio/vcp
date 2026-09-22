# JavaScript, TypeScript, and web projects

Original VCP guidance, version 1.1.0. This package supplies instructions, not a tool executor or authority.

## Resolve the project contract

Read package.json, its packageManager/engines fields, the applicable lockfile, workspace configuration, and relevant tsconfig/framework configuration. Determine the package working directory and module format before editing imports. Multiple lockfiles require repository guidance or an explicit ambiguity report; do not pick a manager arbitrarily.

Use the project's declared scripts and pinned toolchain. For example, a test script may be invoked with the selected manager's run form, but inspect the script and filter behavior first. Do not assume npm test, a global TypeScript compiler, npx downloads, or a framework CLI is appropriate. A script can perform network or lifecycle effects and still needs current authority.

Review async error handling, input validation, browser/server boundaries, and public API behavior against nearby tests. Generate changes that preserve existing rendering, module, lint, and formatting conventions. Avoid opportunistic lockfile churn.

## Exact numeric contracts

When a contract requires exact integer results, bound intermediate arithmetic as well as inputs and outputs. Multiplying safe integers can exceed Number's exact range before a later division or range check. When Number intermediates cannot be proven exact, use native BigInt if the pinned runtime and TypeScript target support it, or the project's existing numeric representation appropriate to the contract. Preserve decimal semantics when required. Apply the specified rounding rule before converting a proven bounded result back to Number. Do not add a dependency when the runtime already supplies the required arithmetic.

Distinguish an omitted option from an explicitly invalid value; a nullish default must not silently accept null when the contract requires an integer. Check boundary, extreme and invalid inputs alongside the project's declared checks. Passing a small existing suite does not establish the full numeric contract; report the actual coverage and any remaining uncertainty.

## Verify

Choose the affected workspace's test/type/build checks. Explain unsupported browser or service checks and missing dependencies without automatically installing them. Return the selected package/script, preserved lockfile rationale, and actual results.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
