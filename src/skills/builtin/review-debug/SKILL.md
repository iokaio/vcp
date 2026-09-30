# Evidence-driven review and debugging

Original VCP guidance.

## Inspect first

- The reproduction: input, revision, environment, command and the exact observed output, logs or stack trace.
- The smallest failing case you can reach: a single test, fixture or script.
- The change under review and its base, or the last known good revision if one exists (a bisect candidate).
- The affected contract, a bounded caller/callee path, and the existing tests for it.

## Narrow the failure

Read the change, the affected contract, and a bounded caller/callee path. Write the expected behavior and the observed discrepancy. For a reported failure, record the input, revision, environment, and minimal reproduction before modifying code. Preserve the original failure evidence.

Use the requested diff and base when supplied; otherwise state the current changes and paths examined. Search for the relevant symbol or text, then read its source range and nearby contract. Use explicit regex only when useful; a limited search or partial range is not the complete source. Keep returned source versions with the evidence and reread changed pages before relying on them.

Trace data ownership and error propagation. For concurrency changes, inspect cancellation, lock ordering, lifecycle transitions, and the point where an effect becomes irreversible. For security changes, follow untrusted input to authorization and output boundaries; distinguish an exploitable path from a hypothetical concern.

Review benign neighboring behavior as a control. A suspicious name or unusual style is not a correctness finding. If execution is unavailable, explain the static path and what would confirm it rather than inventing a reproduced failure.

## Proceed and verify

1. State a concrete hypothesis and use existing tests or logs to check it. Perform an available reproduction within current grants. Ask for missing input only when it cannot be obtained through those grants.
2. Shrink the case until one change flips it. When a known good revision exists and bisecting is authorized, remember that `git bisect` checks out revisions in the worktree it runs in. Run it only in a separate task-owned worktree at a clean revision (for example from an authorized `git worktree add`), never in the user's worktree, especially one with uncommitted work. Test midpoints with the same reproducer, end with `git bisect reset` there, and never discard local work to bisect.
3. Add temporary instrumentation only to resolve a specific uncertainty; record its exact paths, edits and intended removal in the task evidence.
4. Prefer a root-cause correction with a targeted regression over retries, broad catches, or weakened assertions. Confirm the regression fails before the fix and passes after it.
5. Rerun checks affected by the final edits and keep missing or unavailable checks explicitly not run.

## Pitfalls

- Fixing the first plausible cause without confirming it explains the observed failure.
- Treating a flaky pass as a fix; repeat the reproducer when timing is involved.
- Mixing findings with style suggestions; keep correctness findings, uncertain hypotheses and optional suggestions separate.
- Windows: path separators and case in assertions, CRLF in expected output, files locked by a running process, and different default shell or encoding than CI.

## Fix and report

Report each finding with location, trigger, consequence, and supporting evidence. Keep uncertain hypotheses separate. After a fix, record the original reproducer's new outcome and any remaining uncertainty.

Bind findings to the examined revision and scope. Mark whether a defect was introduced by the change only when comparison with the base or other evidence supports that conclusion; overlapping a diff hunk is insufficient. No supported findings means none in the examined scope, not guaranteed correctness. A child check or review does not replace verification of the current integrated parent.

Inspect the final diff and remove only instrumentation owned by this task through version-checked edits. Preserve concurrent human changes; report any removal blocked by them and anything deliberately retained. After interruption, inspect the current diff and retained evidence before continuing or removing temporary edits.

Authority: this guidance ranks below current user constraints and AGENTS.md, grants no tools, installs or network access, and missing prerequisites are reported as not run.
