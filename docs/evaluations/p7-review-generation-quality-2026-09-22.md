# P7-05 review and generation quality retest

This increment corrects evidence-reference grading and retests the production
review/child guidance with `openai/gpt-5.6-luna` and `qwen/qwen3-coder-next`.
The [earlier paired campaign](p7-live-qualification-2026-09-22.md) remains intact,
including its failed reviews, Qwen request-limit failure and unresolved charges.

## Evidence grading and guidance

The review adapter exports canonical tool evidence for the examined task. The
grader resolves opaque artifact IDs only after checking retained bytes and digest,
task/session/workspace scope, source path, content hash, workspace binding and
returned source range. Exact source paths remain valid citations. A source-looking
substring, foreign task, stale version or partial artifact cannot establish a
comparison. Finding usefulness still requires manual assessment.

The report separates detected defects, fully qualified findings, false positives,
unqualified findings and duplicates. Correct reproduction without supported
change causality still fails the formal gate. Duplicate findings cannot pass.
Offline regrading of the original Luna artifacts yields:

| Historical arm | Detected | Qualified | False positives | Unqualified | Gate |
| --- | ---: | ---: | ---: | ---: | --- |
| Single-agent review | 2 | 1 | 0 | 1 | Failed |
| Child review | 2 | 2 | 1 | 0 | Failed |

The private receipt `artifacts/p705-historical-review-regrade.json` binds the
original plan, rubric, state, transcript and evidence hashes without changing
those artifacts or making provider calls.

Production helper revision 2 requires contract-valid triggers, explicit base and
current comparison, benign controls and clear static-versus-executed evidence.
Both review arms receive that same guidance. Isolated children receive their
actual process restrictions and shared request ceiling, source-reuse and patch
format guidance, and the requirement to leave unavailable checks for current
parent verification. Neither guidance contains fixture names or expected answers.
The final guidance also distinguishes omitted defaults from invalid explicit
values, and the navigation tools show valid numeric/null argument examples.
Strict argument validation and host-controlled completion remain unchanged.

## Frozen comparison

Fresh plans retain the original review sources/rubric and generation fixture,
16-request ceiling, 4096 output-token limit, 900-second deadline and zero transport
retries. Both models use their independently qualified provider snapshots. Each
stage reserves USD 4 within the existing USD 100 campaign ceiling; all earlier
unresolved exposure stays reserved. Claimed plans are never replayed.

## Retained intermediate observations

The first guidance build passed both Luna review arms and all 44 Luna generation
checks (seven generation requests). Manual review found one mistyped artifact ID
in the child tax finding; its readable path, quoted code, reproduction and base
comparison were correct. That formal pass is retained with the citation limitation,
not described as wholly faithful artifact attribution.

Qwen's original `parasail/bf16` endpoint returned upstream shared-pool HTTP 429
on the second review request. Its generation retest reached one valid two-file
patch, reread both changed files, attempted an unavailable process and recorded
incomplete child verification before another HTTP 429 on request 14. This was
not request-limit exhaustion. The retained patch still accepted primitive options
and explicit null discount values, so it does not establish generation acceptance.
Neither interrupted run is treated as free or passing.

A fresh public endpoint catalog and two fixed conformance probes qualified
`novita/fp8` for the same Qwen model, with exact endpoint attribution, data-collection
denial, no provider fallback and joined generation charge receipts. Its initial
real review pair failed on invalid string-valued numeric tool arguments. The
final navigation descriptions clarify integer/null arguments without coercing
malformed calls. This endpoint change is explicitly part of the later comparison;
the failed Parasail and first Novita runs remain separate observations.

Novita repeated the invalid nullable-number arguments after the description
clarification. The next candidate, `streamlake`, passed the same conformance and
receipt join but returned upstream HTTP 429 on the first actual review request.
A final review attempt on the original Parasail endpoint again returned HTTP 429
on request two. Basic echo conformance therefore does not establish readiness for
the complete VCP tool surface or sustained task traffic.

## Final review observations

Both final-binary Luna review arms qualify two defects with no false positives,
unqualified findings or duplicates. Baseline used five requests (37.432 s,
USD 0.007540); the child used four (49.991 s, USD 0.006904).

The child initially failed automated grading because its readable citations used
`file:line, artifact ...`. The parser now accepts comma/semicolon separators
without admitting path substrings or invalid ranges. The separate offline receipt
`artifacts/p705-luna-final-review-regrade.json` preserves that initial grader
failure and binds the corrected result to the original bytes; no model replay was
used to obtain the pass.

Manual inspection confirms useful triggers, consequences, base/current causality
and honest static-evidence limitations in both arms. Each final Luna arm mistyped
one appended artifact UUID; the readable paths, quoted code and base references
remain correct. The frozen path-or-artifact rubric accepts those supported source
citations, while `artifacts/p705-review-manual-audit-final.json` explicitly records
the UUID defects. The passing gate does not certify every appended artifact ID.

Qwen has no passing review pair in this increment. Rate-limited runs and malformed
tool calls are failures, not evidence of zero defects or a passing empty review.
P7-05 remains in progress; these small samples do not establish population-level
quality or speed rankings.

## Final generation and accounting

The final Luna generation run passed all **44/44** independent checks against
the integrated parent in **7/16 requests**, 81.386 seconds and USD **0.012803**.
The staged, unstaged and selected untracked inputs and concurrent human note
remained intact; canonical current-parent verification passed.

Final Qwen generation on Parasail stopped at **request 5**, after four settled
read requests, with an upstream shared-pool HTTP 429. Child and parent source
hashes remained frozen: no edit, integration or parent verification occurred.
Its USD **0.002670** known charge and **0.098651** unresolved upper bound remain
separate. The audit is `artifacts/p705-qwen-final-generation-audit.json`.

All final-binary observations use adapter SHA-256
`5f5afe1a11e4ec5536401a11394e7472ddc800452d2c7b756273991e2f17acb0`.
The final Luna review plan is
`c893a36fa34701244ce7d4386ba1d51ee613f7bd3618ca7119a3a11d3548034e`,
and its generation plan is
`0652af47dccd1b8547565dc879ab22e61baf213de4eef0c93b70c713e81b7519`.
The final original-endpoint Qwen review and generation plans are respectively
`12253aa7c2af0df8f994d6d32bcff51af4346e20f08dcb09f5ce26a83a0a9eb2`
and `6545445e1a4811c9dd6e5e8b3979fc02ec0ddcb4d7dd30f7bce020c62c34e492`.

The complete increment, including every failed stage and both endpoint probes,
costs USD **0.073693 known settled**, with **0.537531 reserved for unresolved
outcomes**. The existing owner campaign now holds USD **0.140466 settled** and
**21.141653 reserved** within its USD 100 ceiling; no fresh run remains prepared.
Private summary `artifacts/p705-live-summary.json` has SHA-256
`5d0249b2e3ca673cdb35899990c3d5dd009012972d7b18e088eeee23410c4373`.
Original plans/results and separate offline regrade receipts are retained.

Remaining acceptance is a passing Qwen review pair and generation result under
the same fixed limits. The code and evidence are reviewable, but this increment
does not close that gate or justify merging it as completed P7-05 qualification.

## Verification

The repository fast gate passed all 17 stages. All 15 focused grader contracts
passed on the qualified Node runtime, including current-parent oracle and frozen
generation preparation. Native checks passed two helper validation tests, 13
child regression cases (two existing opt-in fixtures ignored), two adapter
queue/deadline cases, 11 tool preparation cases and two coding wrapper cases.
Affected Clippy checks passed with existing warnings; changed production Rust
formatting and diff checks passed. Independent code review found no actionable
implementation defects.
The final fast manifest is
`artifacts/p705-delivery-fast/95844618-6008-4fdb-b5cd-27dd0fb76ae6/manifest.json`.
