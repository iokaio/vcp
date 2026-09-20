# Evidence-driven review and debugging

Original VCP guidance, version 1.0.0. This package supplies instructions, not a tool executor or authority.

## Narrow the failure

Read the change, the affected contract, and a bounded caller/callee path. Write the expected behavior and the observed discrepancy. For a reported failure, record the input, revision, environment, and minimal reproduction before modifying code. Preserve the original failure evidence.

Trace data ownership and error propagation. For concurrency changes, inspect cancellation, lock ordering, lifecycle transitions, and the point where an effect becomes irreversible. For security changes, follow untrusted input to authorization and output boundaries; distinguish an exploitable path from a hypothetical concern.

Review benign neighboring behavior as a control. A suspicious name or unusual style is not a correctness finding. If execution is unavailable, explain the static path and what would confirm it rather than inventing a reproduced failure.

## Fix and report

Prefer a root-cause correction with a targeted regression over retries, broad catches, or weakened assertions. Report each finding with location, trigger, consequence, and supporting evidence. Keep uncertain hypotheses separate. After a fix, record the original reproducer's new outcome and any remaining uncertainty.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
