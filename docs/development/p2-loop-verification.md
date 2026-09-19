# Verification in the retained coding loop

P2-05/P2-06 remain in progress. This increment connects the existing native
verification adapter to retained tools and the owning turn driver's completion
decision. [Local qualification](../evaluations/p2-loop-verification.md) passed;
this is not an installed CLI.

`vcp_verify` accepts only a bounded list of canonical same-task citation IDs.
Check definitions, runner profiles, expected tests and scope rationale remain
trusted owner setup through `VerificationConfig`. The wrapper first consumes an
exact one-use call from a completed, captured and accounted response. It selects
the check's applicable instruction scopes before execution; new scopes require
the model to receive refreshed context and reissue the call. Native checks still
pass through the prepared process broker and current authority.
Explicit policy denials naming `vcp_verify` conservatively block the workflow;
dispatch through the `vcp_exec` broker cannot erase that named ceiling.
Context assembly includes the new tool in its conservative read-ceiling checks,
so a matching read denial stops admission before any provider request.

The resulting verification, check receipts and complete output references enter
the canonical tool-call/result pair. A failed check remains visible. Verification
advances the observed task fingerprint; sibling calls from the old response can
therefore become stale. Captured stale calls receive explicit canonical
unexecuted results without native dispatch, preserving complete pairs for a
deliberate source/authority refresh. Such stale calls pause the canonical root.
Verification must be the only call in its model response. Retained async calls
can acquire their execution lock out of response order, so a mixed verification
response produces explicit unexecuted pairs and requires fresh separate calls.

After retained work drains, the owning driver calls `complete_coding_turn`.
It requires an accounted final response, no pending calls, current task authority
and the latest verification observed by this owner. The caller cannot choose an
older passing result. The existing native completion gate rechecks source,
instructions, evidence access, unresolved effects and required checks.
Final-response selection and completion use the same owner/admission lock.

Completion also compares a digest of the workspace's canonical effects against
the verification candidate. Any later native operation, including a completed
read or a failed operation, requires fresh verification. This prevents a later
effect from being hidden by restoring the same source bytes. External transient
edits between observations retain the limitations documented in
[native verification](p2-verification.md).

An accounted final model response can change cost without changing tested input.
After rechecking source, authority and effect identity, completion captures a new
`verification-completion-refresh/1` artifact and immutable verification record
with the current quantitative ledger, accounting certainty and a reference to the previous verification.
The original check report and its accounting observation remain unchanged.

The owning driver still has to invoke the completion API after retained
`TurnComplete`; the installed CLI and complete end-of-turn control flow remain
later work. This increment does not qualify semantic prose coverage, arbitrary
frameworks, live OpenRouter compatibility, compaction, retries or ongoing PTY
interaction. See the [canonical loop](p2-canonical-coding-loop.md) and
[P2 plan](../plan/05-openrouter-and-session-loop.md).
