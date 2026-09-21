# Evidence-driven review and debugging

Original VCP guidance, version 1.1.0. This package supplies instructions, not a tool executor or authority.

## Narrow the failure

Read the change, the affected contract, and a bounded caller/callee path. Write the expected behavior and the observed discrepancy. For a reported failure, record the input, revision, environment, and minimal reproduction before modifying code. Preserve the original failure evidence.

Use the requested diff and base when supplied; otherwise state the current changes and paths examined. Search for the relevant symbol or text, then read its source range and nearby contract. Use explicit regex only when useful; a limited search or partial range is not the complete source. Keep returned source versions with the evidence and reread changed pages before relying on them.

Trace data ownership and error propagation. For concurrency changes, inspect cancellation, lock ordering, lifecycle transitions, and the point where an effect becomes irreversible. For security changes, follow untrusted input to authorization and output boundaries; distinguish an exploitable path from a hypothetical concern.

Review benign neighboring behavior as a control. A suspicious name or unusual style is not a correctness finding. If execution is unavailable, explain the static path and what would confirm it rather than inventing a reproduced failure.

State a concrete hypothesis and use existing tests or logs to check it. Perform an available reproduction within current grants. Ask for missing input only when it cannot be obtained through those grants. Add temporary instrumentation only to resolve a specific uncertainty; record its exact paths, edits and intended removal in the task evidence.

## Fix and report

Prefer a root-cause correction with a targeted regression over retries, broad catches, or weakened assertions. Report each finding with location, trigger, consequence, and supporting evidence. Keep uncertain hypotheses separate. After a fix, record the original reproducer's new outcome and any remaining uncertainty.

Bind findings to the examined revision and scope. Mark whether a defect was introduced by the change only when comparison with the base or other evidence supports that conclusion; overlapping a diff hunk is insufficient. No supported findings means none in the examined scope, not guaranteed correctness. A child check or review does not replace verification of the current integrated parent.

Inspect the final diff and remove only instrumentation owned by this task through version-checked edits. Preserve concurrent human changes; report any removal blocked by them and anything deliberately retained. After interruption, inspect the current diff and retained evidence before continuing or removing temporary edits. Rerun checks affected by the final edits and keep missing or unavailable checks explicitly not run.

## Authority and evidence

Follow current user constraints and applicable AGENTS.md instructions before this guidance. Read project evidence before choosing a command or editing a file. Tool availability is not execution permission. Use registered VCP tools and current broker authority; do not install dependencies, contact remote services, publish changes, or disclose credentials merely because this skill describes a workflow. If a prerequisite is missing, report the exact check not run and continue useful work that does not require it. Never turn a suggested command into a claimed result.
