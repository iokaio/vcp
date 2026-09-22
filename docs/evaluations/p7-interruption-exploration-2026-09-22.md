# P7 interruption and exploration qualification — 2026-09-22

This follow-up closes the remaining connected P7-04/05/06 cases with a passing
source-bound native gate. Earlier [paired live observations](p7-live-qualification-2026-09-22.md),
[Luna review](p7-review-generation-quality-2026-09-22.md), and
[Qwen 3.8 review/generation](p7-qwen38-reasoning-budget-2026-09-22.md) remain separate
evidence, including every failed run and unresolved charge. Packaged P8 acceptance
and an arbitrary-child-process filesystem sandbox are separate gates.

## Independent child control

A connected two-child fixture exposed a production defect: deliberately pausing
or cancelling one active child retained its unknown charge but also paused the
root, blocking the unrelated sibling. Unfinished response cleanup now recognizes
an independently held child only while the owner remains live, no authority
transition is pending, the exact canonical child is paused/cancelled and its root
is running. It retains the full unknown reservation through the existing ledger.
Unexpected provider disconnects and owner loss still pause the root.

Targeted native tests pass for both stores and both individual controls. They
observe the sibling completing its existing request, the stopped child's exact
liability, and unchanged parent bytes. Negative controls cover unexpected provider
failure, owner loss and late callbacks. Independent review found no material
authority, accounting or synchronization issue.

## Connected fault schedules

The native fixtures add independently observed effects to existing component
coverage:

| Boundary | Observations |
| --- | --- |
| Near-budget race | Two registered children compete for the same root capacity; exactly one request reaches the wire, the loser reserves nothing, and protected verification capacity remains intact. |
| Sibling writes | Disjoint and overlapping assignments use isolated roots on both stores; mixed allowed/forbidden patches fail before effects; parent and sibling bytes remain unchanged. |
| Dependencies | A blocked dependent has no request or reservation. Actual current analysis verification and predecessor completion release its launch; a self-reported result does not. |
| Integration controls | Sixteen pause/cancel schedules cover before and after each of two writes on both stores, retaining receipts, history, accounting and later human edits. |
| Missing receipts | Four supervised process kills occur after native writes but before their receipts. Fresh owners retain unknown outcomes and do not replay the integration or overwrite later human edits. |
| Child write crash | On both stores, the supervisor kills the owner during a registered child's two-file write, before the first receipt. Recovery preserves the first child write, a later human edit, parent/sibling isolation, transcript and exact ledger; it retains the unknown effect without fabricating a receipt or replaying work. |
| Parent verification | Four pause/cancel cases retain completed checks against the integrated parent while denying task completion at publication. |
| Stale results | Steering invalidates a child's former assignment without losing its transcript, observed result or history; no stale integration or new request occurs. |
| Output loss | A noisy child saturates bounded notifications beside a quiet active sibling. A real pipe reader closes, output ownership ends, admission stops, and fresh-owner cursor recovery retains all 32 messages and known/unknown charges. |

Completed native writes may correctly have a succeeded effect even when the
following task-completion boundary is stopped. Tests do not manufacture an unknown
effect merely to make cancellation appear stronger. Qualification-only write
barriers are excluded from ordinary builds and leave native policy/version checks
and receipts in place. Ignored subprocess entry points are exercised by their
supervising tests, rather than counted as standalone passes.
The child-write case terminates the OS owner hosting that registered child's
execution; it does not claim a separately sandboxed child process.

## Broad exploration comparison

CR-03's exploration comparison uses an eleven-file synthetic event-ingestion
repository. Both arms receive production `explore@2` guidance, read-only tools and
the same frozen task. Actual canonical reads must cover every cited source line.
The grader checks active first-visit order, error statuses, duplicate side effects,
inactive modules and honest verification limitations; prose usefulness still
requires manual review.

Two Luna pairs used `openai/gpt-5.6-luna` through
`amazon-bedrock/us-east-1`, with 16 requests per arm, 16384 total output tokens,
360 seconds per response, 900 seconds per task and zero transport retries.

| Pair / arm | Formal disposition | Requests | Seconds | Input tokens | Output tokens (reasoning included) | Settled USD |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| V1 baseline | Failed evidence ranges/output shape | 4 | 50.262 | 16240 | 1879 (328 reasoning) | 0.006948 |
| V1 child | Failed evidence ranges/output shape | 4 | 59.896 | 17758 | 1709 (199 reasoning) | 0.007141 |
| V2 baseline | Passed original and corrected grade | 4 | 48.644 | 16677 | 2369 (549 reasoning) | 0.007715 |
| V2 child | Original flow grade failed; corrected offline grade passed | 5 | 61.907 | 22380 | 1998 (227 reasoning) | 0.008795 |

V2 explicitly requested minimal operative ranges and the inactive-module array.
Its child correctly returned to the caller between dependencies, producing eight
steps. The original grader incorrectly required exactly seven rows although the
prompt did not forbid revisits. The corrected grader allows bounded revisits,
requires the original first-visit order, and validates every line of every range.
Both retained V2 answers pass without another provider call. Original result files
and V1 failures remain unchanged. Manual review confirms accurate behavior,
useful synthesis, examined sources and no claim that executable checks ran.

Keep the simpler baseline for this task: the helper adds 34.2% input tokens,
14.0% cost and 27.3% latency without material usefulness gain. It remains an
explicit helper option; this experiment adds no automatic delegation heuristic.
There was no parent model continuation in the child arm. Its actual parent input
is zero and all 22380 input tokens belong to the child; this does not measure
future parent-context savings. Returned answer sizes are 4034 and 3964 bytes.
Reasoning tokens are a subset of output, never an additional charge or public
reasoning transcript.

The immutable V1 and V2 plan hashes are respectively
`2a9664c485cbce445ebca6e6bdf8d0238f89b57bfb4c333fb4343548eebbf8f1`
and `3514ba7649d31f35abb61fedca4b6002d51111ebc8cdb89485736c3ee0305ba5`.
The adapter hash is
`7ad68b473d3b4b66592b1d3a19b759c6ddae49578228afbdaf2b6f972fb8b87f`.
The reproducible private audit `artifacts/p704-exploration-audit.json`
(`2966dd05af611c56752006c7c0479959987fe0ef666bb99f649f4324e1a5ddea`)
binds the original results, exact grader revisions, canonical read evidence and
response usage for all four arms. Its reproducer is
`artifacts/p704-exploration-audit.cjs`
(`ebf8be5e34a40bfad459e6e3d537e6c8e381ec727b6ecd0cd0b8b40c31632c71`).

Both pairs fully settled at USD **0.030599**. Campaign totals are now USD
**1.278161** settled and **34.127269** reserved within the existing USD 100 cap.
Earlier uncertain requests remain reserved; this increment adds none.

## Final verification

The broader native run passed 61 repository/tool tests and 98 canonical-contract
tests. Its complete host run finished with 141 passes, two failures and nine
ignored entries. The original failing manifest is retained at
`artifacts/p7-completion-qualification/37f489f0-f59b-4dc7-9b76-70d29032a486/manifest.json`
(`87e08cb2f12387b29376a0953be423389edf8bceaa8de9f701a8c4e52d52dee1`).
It is not represented as a passing full suite.

The compaction fixture had not accounted for PR #115's 535 additional serialized
tool-description bytes. The native boundary correctly refused its now-oversized
context. Its fixture-only correction restores exactly that schema allowance,
preserving the same source/history budget and all oversized-request, compaction,
original-history and unknown-liability assertions. The second failure is in the
scripted MCP content/provenance fixture: its inherited 24k prompt allowance no
longer held the complete eleven-request conversation. A 535-byte-only adjustment
still failed and is retained as a failed trial. This fixture now explicitly
qualifies a 32k prompt/40k context envelope, preserving all eleven requests,
approval boundaries, external-data roles, actual effects and artifact provenance.
No production envelope or shared fixture default changed.

All focused follow-ups pass: child-write crash recovery on both stores (6.35 s),
normal/oversized compaction on both stores (185.31 s), and the complete MCP
content conversation on both stores (58.64 s). Their logs are
`artifacts/p705-p706-child-native-write-focused.log`,
`artifacts/p705-p706-context-calibration-focused.log`, and
`artifacts/p705-p706-mcp-content-envelope-focused.log`. The unchanged passing
host cases remain separate evidence; the original failed full-run result is
not rewritten.

The final source-bound delegation/CLI gate passes all seven stages at
`artifacts/p7-final-delegation/2bc1f5c9-b4f5-4fed-8b9a-7cc4565d4a03/manifest.json`
(`59ef747109ff6840a87676c16ed034a3c0255c1d1105216cf1b366c21be77b0d`):
61 repository/tool tests, 98 canonical contracts, 23 connected cases, one
verification-pause case, 68 CLI library tests, seven native terminal cases and
one noisy/quiet output-loss case pass. The last case executes both stores.
All bound source inputs remain unchanged throughout the run. Seven ignored
entries are three supervised crash helpers exercised by their supervisors,
two explicit cloud-path checks, an independent age check and the operator
handoff exporter; the latter four are not claimed as executed here.

Clippy for the lifecycle
and CLI libraries/tests plus the live adapter passes with existing warnings.
The ordinary production library/binary configuration, without qualification
hooks, also checks successfully. The repository fast gate passes all 17 stages
at `artifacts/p7-completion-fast/1bbe25e7-76ab-43f7-9494-4678a523d372/manifest.json`
(`c4bbcc877a389dada354986c9e0acc232587e839c0fd283f9ee812518a90360e`).
The targeted exploration contracts pass 19/19 on the qualified Node runtime,
and the adapter's two tests pass. Ignored entries distinguish supervised crash
helpers from separate explicit long-check, real-model and portability gates;
this run does not claim those explicit gates executed.
