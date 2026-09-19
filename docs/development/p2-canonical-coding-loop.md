# Canonical coding through the retained loop

P2-01 and P2-05 remain in progress. `CanonicalHost` now supplies registered
`vcp_read`, `vcp_list`, `vcp_search`, `vcp_patch`, `vcp_exec` and `vcp_verify` wrappers to the
retained Codex scheduler. Each actual model request assembles fresh canonical
context, passes the OpenRouter codec and shared accounting, and captures its
exact manifest/body. This is an internal native host milestone; the installable
VCP interface and complete P2 acceptance remain separate work.

The owning host installs itself as the turn, work and tool contributor, supplies
`foundation::coding::allowed_tools()` as the retained startup ceiling, and binds
the retained thread to a canonical task. It explicitly configures the provider
and any native executable profiles before calling `configure_coding`. The latter
takes trusted operating guidance, affected relative paths, a cumulative root
request limit from 1 through 128, and an absolute deadline within one hour.
It cannot be deserialized from model input. Provider/profile changes require a
fresh coding owner setup. The private P1 synthetic codec cannot enable wrappers.

## Context and effects

Each attempt reads the current canonical objective and task state, refreshes
scoped AGENTS.md sources, and includes recorded tool-call/result pairs. It uses
the existing conservative byte estimate, actual schemas, output reserve and
margin. Arbitrary retained prompt text is not recaptured as user authority.
Native read ceilings conservatively apply across the registered tool names;
these checks do not establish a general-purpose data-access policy.

The wrapper obtains the thread identity from the retained thread extension
store. A completed, validated and durably accounted response grants one-use
eligibility for its exact call ID, tool name and JSON arguments. Current
canonical revisions and native instruction probes are checked again before
preparation. Default namespaces use the retained name matcher. Partial streams,
unknown charges, changed instructions, historical call IDs and unregistered
handlers cannot gain effect authority. Missing provider cost retains liability
and pauses the root before another request.

Read/patch paths and process working directories can reveal a new instruction
scope. That operation returns an explicit unexecuted result; the next request
includes refreshed guidance before the model may reissue it. Selection remains
inside the registered workspace. Outside-workspace parent instruction grants
are supported by the lower-level context API, but are not enabled by this
automatic setup.

File wrappers consume existing prepared tickets. Process wrappers consume
explicit executable profiles and wait for owned job/output reconciliation.
Policy questions remain durable waiting states; a wrapper does not grant itself
approval. Process timeout must fit the remaining coding deadline; model response
deadlines also respect that deadline. Full output and receipts precede the
model-visible result. Process results identify full stdout/stderr artifacts,
exit status and bounded lossy UTF-8 display tails, with explicit truncation flags.
The provider schema subset now accepts an explicit string/null union for terminal
input; unsupported unions and invalid argument types remain rejected.

## Continuity and bounds

Pair records retain original response and result artifact references. Every
assembly resolves them through current history access, alongside exact captured
content. Reopening requires deliberate resume and fresh owner/provider setup;
it restores history references, never executable call eligibility. Cumulative
root attempts survive reopen and include helpers/children. The strictest configured
request limit and deadline apply across all requests, including manually prepared
helper contexts; a child cannot widen them. Context capacity,
deadline, unresolved calls and request-limit failures pause the root. The owner
can enable [deterministic compaction](p2-context-continuity.md) after configuring
its verification baseline. Current facts remain mandatory, and an oversized
compacted request still pauses before transport admission.

The assembler currently selects objectives, state, instructions and tool pairs.
Full provider bytes remain in history; this increment does not implement semantic
selection of assistant prose, retry orchestration, ongoing terminal
interaction, or automatic verification-based task completion. The separate
[native verification adapter](p2-verification.md) provides owner-driven checks
and current-evidence completion. Its [retained tool integration](p2-loop-verification.md)
adds isolated `vcp_verify` calls and final-cost evidence. Model prose and a process
exit code alone do not certify completion. A paused stale-call sequence requires
fresh owner setup; transparent same-owner recovery is subsequent work.

The source lives in `src/crates/vcp-lifecycle/src/foundation/coding.rs` and
`foundation/worker/coding.rs`, with the existing provider and prepared brokers.
`tests/support/coding.rs` drives actual retained requests, files and processes on
both canonical stores. Use `scripts/test-integration.ps1`, `scripts/test-p1.ps1`
and `scripts/test-provider.ps1`; native recovery/lifecycle checks remain relevant
when changing ownership or unknown-outcome behavior. See the
[qualification report](../evaluations/p2-canonical-coding-loop.md),
[response boundary](p2-response-tool-boundary.md), [prepared files](p2-tools.md)
and [native processes](p2-process.md).
