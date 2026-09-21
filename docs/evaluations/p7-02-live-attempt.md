# P7-02 approved live trial observations

On September 21, 2026 the owner approved the prepared eighteen attempts and
$45 aggregate cap, including the sole generation verification launcher's exact
permission proposal. This supersedes the earlier pending-approval state in the
workflow preparation record. Source revision: `875da8039b267107b01f35b31dd967ec81589646`.

| Approved plan | SHA-256 | Allocation | Observation |
|---|---|---|---|
| Read-only baseline/skill pairs | `9d36819e22aee9df0e78c26762f9dae2d94539364d25317b66ea9361ece8d732` | $40; sixteen attempts | Eight failed; eight not run after continuation |
| U03 generation pair | `f936475020c5144422f60d8f8397387b2de8b15a9a84cf6ade381a37ec1a68e2` | $5; two attempts | Both attempted and failed after continuation |

Both exact plan hashes, current provider profiles and fresh stores validated
before dispatch. The read-only runner then permanently claimed its plan and
submitted the `architecture-normal-v1` baseline request. The captured response
was HTTP 401, `{"error":{"message":"User not found.","code":401}}`.
The retained fifty response bytes have SHA-256
`6faf1b143addca514302d08060df34ba407457f06d73add7ca78af04a158c8f9`.
The response capture is aborted, with authentication/recovery material excluded.

The canonical task paused with one attempt in `reconciliation_pending`, zero
active reservations, **$0 settled and $0.201640 unresolved liability**. Actual
cost is unknown; an authentication error is not substituted for provider charge
evidence. There was no final model answer or usefulness grade. The runner's cost
gate stopped subsequent dispatch and retained the failed case and all fifteen
not-run cases. The separately approved generation plan was not started with the
rejected credential. There were no retries or repeated claims.

## Credential recovery and generation

The first environment lookup ran under the sandbox's separate Windows user and
found no saved alternate. Repeating that lookup in the repository owner's
context found a saved `OPENROUTER_API_KEY` that differs from the stale inherited
value. A read-only authentication request using the saved value returned HTTP
200. No credential value or account metadata is retained in these reports.
The credential blocker was resolved through this existing local mechanism; no
user credential update was needed.

The unclaimed, separately approved generation plan then ran with that saved
credential. Its baseline produced five settled responses and stopped on a sixth
aborted response. Its canonical ledger records **$0.006957 settled and $0.201640
unresolved**, with no active reservation. The generation skill arm had not yet run.
The cart source remained unchanged, and no successful parent verification or
generation usefulness is claimed.

Inspection found an actual `response.incomplete` terminal with
`max_output_tokens`: 5,950 input tokens, 512 output tokens and observed cost
`0.0021275` USD (2,128 microdollars after conservative rounding). Earlier calls
included two invalid patch forms; all five workspace files retained their
original hashes. The independent oracle passed only 6 of 44 checks, so the
generation outcome fails independently of accounting.

The terminal used `"{}"` as the incomplete call's argument placeholder after
an empty argument-done frame. The gateway compared those strings before reading
final usage and rejected the terminal. The fix preserves identity/name checks
and argument equality for completed calls, while allowing non-completed
placeholders to retain valid usage. Incomplete responses still produce no
eligible calls and cannot complete the task. Seventeen provider tests and the
native coding matrix on both stores passed, including 2,128 microdollars settled,
zero unresolved liability, no patch dispatch, a paused task and reopen.
Original capture and ledger records are not retroactively rewritten. All live
attempts continue to use the exact originally approved packaged executable;
the new source fix is supported by these targeted regressions.

At that checkpoint across both original plans, two cases were attempted, sixteen remained not run,
and seven model requests were submitted. Known settled charges are **$0.006957**;
unresolved liabilities total **$0.403280**. Actual aggregate cost remains unknown.
Both failed cases and their full original $2.50 allocations remained reserved;
unused allocations are not recycled into retries.

Local private receipts remain beside the original plans: `execution-claim.json`,
`result.json`, the first case's `stdout.jsonl`, canonical costs/routing/outputs/
context pages, and `retained-response-inspection.json`. A separate
`owner-authorization.json` records the owner's approval without modifying either
hashed plan. The non-secret summary is retained locally at
`artifacts/p7-02-approved-live-observation.json`; credentials and private trial
paths are not published here.

## Remaining work

Preserve the unresolved reserve unless reliable provider accounting evidence
establishes the charge.
The shipped CLI has no charge-import command; an HTTP status alone cannot supply
the provider request identity, final usage and retained evidence required by the
canonical usage-observation API.

Both original plans are permanently claimed and have no continuation operation.
Neither may be replayed or have its failed attempt removed. Any
continuation must preserve the original denominator, liability and approved
aggregate cap and original hash/profile-validity checks. P7-02 remains in progress; no live usefulness,
generation or unavailable-toolchain acceptance is marked complete.

## Continuation within the existing approval

A separate [read-only continuation](../../scripts/evals/builtin-live-continuation.md)
admits only the fifteen originally approved, untouched rows, with the same
executable, fixture, profile, prompt, skill and $2.50 allocation for each. Its
integrity digest is bound to the original owner's authorization receipt; it does
not represent a new owner approval. It retains the original failed row, full
$2.50 allocation, canonical liability, claim and result, and uses exclusive new
continuation/per-row markers. Twelve deterministic controls cover replay, tampering,
expiry, original scope and partial known charges. Another unknown liability stops
dispatch. Original results remain historical; continuation results retain the
full denominator and separate known charges from unresolved liability.

The read-only continuation integrity hash was
`dfaa076d911f87a3fc1de02defd8670850b159dedd62ee76478dc01de7aaa58a`.
It attempted seven additional original rows, stopping at the review/debug
negative skill arm. Its canonical costs include **$0.040413 settled** across
the eight attempted read-only rows and **$0.403280 unresolved**, with no active
reservations. The eight testing and JavaScript/TypeScript rows remain not run.

A separate [generation continuation](../../scripts/evals/builtin-generation-continuation.md)
attempted only the original, untouched skill arm. Its integrity hash was
`d62b01d92670289d58b81a3070c31958922cff2247e1e0b4c47630e0853c76f7`.
Three deterministic controls verify original scope, evidence preservation,
replay refusal and partial accounting. The skill arm also stopped with unknown
liability: **$0.003818 settled and $0.201640 unresolved**. Including the retained
baseline, generation has **$0.010775 settled and $0.403280 unresolved**.

The complete approved denominator therefore remains **eighteen cases: ten
attempted failures and eight not run**, with **$0.051188 known settled charges
and $0.806560 canonical unresolved liability**. Actual total cost is unknown.
Unresolved liability is a reservation, not an asserted provider charge. No
failed case was retried and no unused allocation was transferred. Both
continuations stopped at their accounting gates; original results remain intact.

Independent accounting review verified all ten task scopes against their ledger,
attempt and settlement records, and confirmed empty stores and absent attempt
markers for all eight not-run rows. The non-secret summary is retained at
`artifacts/p7-02-approved-live-final-observation.json`. Review also found that
the read-only continuation's partial-cost diagnostic lacked an explicit scope
check. The corrected helper rejects foreign or absent settlement scopes; three
new regressions cover this. The retained live records all have matching scopes,
so the historical totals remain valid. Historical runner hashes are preserved;
the corrected helper cannot replay either claimed continuation.

## Independent usefulness observations

The independent read-only review checked captured final text against source
hashes, settled provider request identities and the predeclared behavioral
rubrics. Of the eight architecture/review rows, six had complete final text:
one met the semantic rubric, one partially met it and four did not. Two had no
usable final answer. These supplemental grades do not override the eight strict
runner failures; several responses wrapped JSON in XML or prose instead of
returning the required JSON answer.

The architecture negative skill arm respected the fixture's explicit
architecture exception. The normal skill arm found the forbidden dependency and
error convention but gave an unclear ownership correction. Neither review/debug
normal arm identified the seeded defect. Repeated zero-based line bounds and
artifact identifiers used as workspace paths prevented useful inspection. A
negative arm's abstention without reading the relevant source does not establish
a correct review. Local independent reports are retained as
`artifacts/p7-02-readonly-quality-architecture-review.json` and
`artifacts/p7-02-readonly-quality-testing-javascript.json`; the latter confirms
the eight untouched rows and makes no usefulness claim for them.

Independent generation review found that both arms left all five starting files
byte-identical and passed only 6 of 44 oracle checks. Neither implemented the
feature or produced a passed parent verification. The exact requested skill was
present in all four skill-arm request manifests, including the unresolved
request. That final response also ended at 512 output tokens with incomplete
patch arguments; its raw observed cost was `0.00205925` USD (2,060 microdollars),
separate from its canonical unresolved reservation. There is no demonstrated
generation improvement in this pair. The independent summary is retained at
`artifacts/p7-u03-supplemental-ufPnXF/summary.json`.

The invalid read calls motivated clearer feedback for one-based inclusive line
ranges and the returned-byte limit. Bounds, tool schemas and authority are
unchanged. The sixteen originally approved read-only rows and both generation
arms continue to be assessed against their original inputs and executable.

The final review/debug response exposed a second accounting defect: a completed
terminal included valid usage and cost (`0.00120975` USD, conservatively 1,210
microdollars), but its read call omitted required nullable bounds. Schema
rejection correctly prevented dispatch, yet also discarded the usage observation.
Observed usage in a retained response does not retroactively settle the original
canonical ledger. Fixing that separation requires independent billing evidence
without granting authority to the rejected tool proposal.

The provider parser now exposes a separate accounting-only receipt when the
complete terminal and usage validate but tool arguments do not. It still returns
a protocol error and exposes no executable calls or completed answer. Malformed
usage, mismatched identities, duplicate calls and conflicting or partial trailing
frames cannot produce that receipt. The lifecycle retains the raw response and
normalized billing evidence, settles the observed charge and pauses the task;
other individually valid calls in the rejected response also remain unexecuted.
Eighteen provider tests passed, including incomplete placeholders and these
negative controls (`artifacts/p7-02-rejected-tool-usage-models.log`).

The native coding matrix passed on both file and SQLite stores (93.26 seconds).
It proves 1,210 microdollars settled, zero active/unresolved reservations, a
paused task, a fenced current worker, unchanged files and no tool effects even
with a valid sibling patch. Reopen preserves the charge and absence of effects.
The incomplete-response case also retains its 2,128-microdollar settlement.
Clippy for all targets of models and lifecycle completed with existing warnings;
changed Rust formatting and diff checks passed. Receipts are
`artifacts/p7-02-rejected-tool-usage-native.log` and
`artifacts/p7-02-rejected-tool-usage-clippy.log`. These regression fixtures make
no live provider calls and do not change the original live ledgers.

Post-trial repository verification passed all fourteen fast gates
(`artifacts/p7-02-post-live-fast/ac13e883-c6a7-4ee4-82aa-d9c4d77d915e/manifest.json`).
That harness deliberately strips launcher environment variables and skips two
generation-continuation controls. A separate run with the recorded launcher and
Node 26.9.0 passed all fifteen continuation controls with zero skips or model
calls (`artifacts/p7-02-continuation-final.log`). Existing read preparation tests
also passed (eleven tests; `artifacts/p7-02-read-feedback.log`).
