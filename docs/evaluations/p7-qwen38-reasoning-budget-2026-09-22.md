# P7-05 Qwen 3.8 and reasoning output allowance

The owner requested Qwen 3.8 on OpenRouter in place of Qwen3 Coder, then authorized
adjusting token budgets for reasoning models. This follow-up preserves the
[original quality retest](p7-review-generation-quality-2026-09-22.md), including
Luna's passing results and every failed Qwen3 Coder attempt. It does not qualify
the old model by substituting results from a different model.

Final checkpoint: the [360-second review pair](#360-second-review-pair) passes
strict live grading in both arms. Together with the 44/44 current-parent generation
result below, this closes the selected Qwen quality retest. Broader P7 interruption
and recovery qualification remains open.

## Exact provider and initial results

A fresh catalog selected `qwen/qwen3.8-max-0902` at `alibaba`. Two fixed synthetic
tool-call/continuation probes passed, with exact provider/revision attribution
joined to authenticated generation receipts. They cost USD 0.002232. No fallback,
transport retry, reasoning-effort override or data-collection permission was
enabled. Echo conformance alone is not proof of task quality.

The initial USD 0.50 probe reservation failed before dispatch because this
endpoint's conservative input bound exceeded that allocation. The fresh probe
used a USD 12 ceiling, within the existing USD 100 owner campaign. Its two actual
request reservations were USD 6.397576 each and were released on settlement;
reservations are not charges. The pre-dispatch failure remains recorded at zero
cost.

Both review arms then failed with the original 4096-total-output-token allowance:

| Arm | Requests | Seconds | Settled USD | Final response |
| --- | ---: | ---: | ---: | --- |
| Baseline | 9 | 165.964 | 0.094980 | 4096 reasoning tokens, no answer |
| Review child | 4 | 240.938 | 0.086690 | 2604 reasoning + 1492 other output tokens; incomplete JSON |

Both read all seven source/reference files with valid tool arguments. Their
terminal responses report `max_output_tokens`; neither produced a complete
gradable review. All charges settled, with no unknown liability. These are output
budget failures, not provider rate limits or evidence of a correct empty review.
The metadata-only audit `artifacts/p705-qwen38-review-audit.json` has SHA-256
`ec58c8d504759a62e974c2ab78b788fc6233a828f516e8fa4139c2a9e2abf5b2`.

The review plan SHA-256 is
`f740ec037126c7cdb02dc2ac68ba931e2bd00f811620f814eeb95592121b09e0`.
It uses the original final adapter
`5f5afe1a11e4ec5536401a11394e7472ddc800452d2c7b756273991e2f17acb0`.

Generation passed **44/44** independent checks against the integrated parent in
**14/16 requests**, 456.706 seconds and USD **0.246074**, still using 4096 output
tokens. Parent verification was canonical and current; staged, unstaged and
selected untracked inputs and the concurrent human note were preserved. Child
verification remained incomplete; the independent current-parent verification
passed. Its plan
SHA-256 is
`c41a351f9043386e905e6fce54bf7eac2aa3b4d824b52831d0706dcfb3841433`.
An earlier synthetic generation workspace failed Git ownership validation before
plan claim or adapter dispatch, cost zero, and remains recorded separately.

## Explicit allowance change

Trusted profiles now accept an explicit `output_tokens` of 1..16384, still clamped
to the qualified provider maximum. Omitted values retain the 4096 default. This
counts total output, including reasoning; there is no separate answer quota and
no change to reasoning-effort qualification. Host admission and settlement still
account for the full selected output bound.

The P7 preparation gate accepts the same explicit range and rejects a request
above provider capacity before creating a trial. Frozen P6 smoke limits remain
unchanged. New Qwen plans select 16384 total output tokens, 16 requests, 900 seconds
and zero transport retries. Fixtures, rubric, strict tool validation and
current-parent verification are unchanged. Prior plans are never replayed.

Review stages reserve USD 32 (USD 16 per arm, with USD 12 allocated to the child),
and generation reserves USD 16 (USD 12 child allocation). These conservative
reservations fit the existing USD 100 campaign and the new USD 6.492808 per-request
bound. The larger output allowance is an explicitly changed evaluation condition;
results must not be described as the original 4096-token comparison.

The first larger-output review used the existing 120-second provider response
timeout. Request eight hit that timeout after seven settled requests costing
USD 0.053864. Its USD 6.492808 unknown-charge bound remains reserved; there was no
completed answer, and the child arm was not dispatched. The plan remains intact.

The owner then selected **180 seconds per response**. That change accepted
an explicit `provider_timeout_seconds` of 1..180, no greater than the task
deadline. Omitted values remain 120 seconds. The existing lifecycle also clamps
each request to the remaining configured coding deadline and preserves it
across retries. The Qwen retest explicitly selects 180 seconds with 16384 output
tokens, while its 900-second overall deadline and zero-retry policy remain intact.

## Retest and verification

The first larger-allowance review used adapter SHA-256
`79e6559d374ec0e2df3057cdead54b048ab355c12fe449ea6dd48b6779e0f485` and plan
`c3bdd4106abe0a7bf83a2489d61c2dafaa2afb0b8f3a036e311b20eaac6e3576`.

With the explicit 180-second timeout, both review arms completed. Baseline used
nine requests, 396.953 seconds and USD 0.172286; the child used seven requests,
244.226 seconds and USD 0.115218. Both identified the two supported defects with
correct base/current causality. All charges settled.

The baseline exposed another grader mismatch: historical prompts did not specify
numeric JSON scalar types for expected/actual, and the answer gave correct values
as `0 — explanation` rather than numbers. Explicit historical offline regrading recognizes a
canonical unsigned safe integer followed by an explicit dash and nonempty
explanation. It still independently recomputes both values, requires a real
discrepancy and retains the numeric-argument, source and causality gates. Wrong,
ambiguous, unsafe and malformed values fail regression tests. No model-authored
code is executed or guessed from prose. The default live grader remains strict:
new answers must use numeric reproduction values and a plain JSON object. The
historical compatibility option never applies implicitly to a live run.

The separate offline audit `artifacts/p705-qwen38-180-review-audit.json` binds
the original and corrected grader identities and raw retained evidence. Baseline
then qualifies both defects with zero false positives. The child remains a formal
failure because it wrapped the answer in a Markdown fence; the parser still
requires a plain JSON object. Its manually useful findings also overstate the
tax defect's affected range: fractional results below 0.5 already round correctly.
The baseline's advisory rounding recommendation likewise overstates its generality
before acknowledging floating-point limits. Neither caveat is hidden by regrading.

That plan is
`e10db0ab3eeebbd6abae030f0a5d4c703ecdbf33b4bd25c2d0b2d163659fee93`,
using adapter
`45c3cfa1d29d139c98483125b35ac4be35d5a0260df49f7ed46a9afb98839c2b`.
A fresh plan made the response instructions explicit: plain JSON without
fences, actual typed reproduction values, and explanation in the descriptive
fields. This changes the prompt's format guidance, not source fixtures, numeric
oracle, evidence requirements or budgets. The new plan is
`6dde38e7b50374d30f37c9d6ab7ce6818329e73375896ada3a133ceaffe38867`.
The baseline stopped at request nine when its response exceeded **180 seconds**.
Elapsed arm time was 321.736 seconds; eight settled requests cost USD 0.089566,
and the interrupted request retains a USD 6.492808 unknown-charge reservation.
There was no final review answer, and the runner stopped before dispatching the
child arm. Neither the 16-request limit nor the 900-second task deadline was
exhausted. No historical failed result is overwritten.

The metadata-only audit `artifacts/p705-qwen38-format-review-audit.json` binds the
frozen runner, current strict grader, plan, adapter and retained canonical evidence.
Its SHA-256 is
`ef062044b7e1a7e4c51920c2082452c395044962246ed807c97b4948e077312b`.
It confirms unchanged fixture bytes and the absence of a gradable final answer.

At this 180-second checkpoint, Qwen 3.8 generation was qualified for this fixture,
but its paired review gate remained open. Trials stopped at the owner's selected
response limit, and the PR remained draft; neither useful
manual findings nor a historical baseline regrade substitutes for a passing pair.

The Qwen 3.8 follow-up has USD **0.860910** in known settled charges and USD
**12.985616** reserved for two unresolved requests. Across the whole owner
campaign, known settled charges are USD **1.001376** and reservations are USD
**34.127269**, within the USD 100 cap. Unknown charges are not treated as free.
The retained `artifacts/p705-qwen38-summary.json` has SHA-256
`bddedb6b41ee830540897307b535ee7d411dd0f566fd5e6dd80224fd4aa2cc81`.

The four native settings tests, three provider-timeout regressions (both stores),
one provider-retry regression (both stores) and two adapter tests pass. The 16 grader
contracts, nine profile-runner contracts and six generation contracts pass on the
qualified Node runtime. Affected Clippy checks pass with existing warnings.
Native tests cover
the unchanged default, explicit upper limit, provider clamping and invalid bounds;
preparation tests cover frozen allowances and rejection before dispatch.

After the final historical-mode restriction, the repository fast gate passed all
17 stages at
`artifacts/p705-qwen38-final-fast/cc5caa50-b12a-4b72-b653-6921782e4fec/manifest.json`.
The 16 focused grader contracts also passed after that restriction. Independent
final review found no material issues in the strict grading boundary, reported
results, accounting totals or retained artifact hashes.

## 360-second review pair

The owner requested a fresh pair at **360 seconds per response**. CLI settings,
lifecycle admission and the preparation gate now accept explicit values in
1..360; the default remains 120 seconds. Explicit values cannot exceed the task
deadline, and each admitted response is still clamped to the remaining configured
coding deadline. This trial retained 16384 total output tokens, 16 requests per
arm, 900 seconds per task and zero transport retries. The prompt, strict grader,
fixtures, source-evidence requirements and numeric oracle did not change.

The fresh plan SHA-256 is
`66589dc230119c22ba8ee3f29162286ef26fc7cb94cceac7f2d0eef10cd76af6`,
using adapter
`8348224682453ffa9e6b83748309fe957c27f534623373d83a90762536d2505c`.

| Arm | Strict grade | Requests | Seconds | Settled USD |
| --- | --- | ---: | ---: | ---: |
| Baseline | 2/2 defects, zero false positives | 9 | 312.297 | 0.140654 |
| Review child | 2/2 defects, zero false positives | 7 | 222.027 | 0.105532 |

Both answered with plain JSON and numeric reproduction values. No historical
annotation normalization or fence removal was applied. Both have zero unqualified
or duplicate findings. All USD **0.246186** settled, with no new unresolved charge.

Manual review independently confirms actionable triggers, consequences and
introduced/pre-existing causality. Both arms read all seven source/reference
files; all 18 readable citations resolve to observed canonical reads and frozen
source hashes. Static review is disclosed honestly without claiming executable
checks. Each arm clearly separates one advisory suggestion from its two defects.
The baseline overstates the tax defect as affecting every fractional result,
although fractions below 0.5 already agree with half-up rounding. The child states
the correct affected range but mistakenly calls 77.5 a non-half example. Their
primary numeric reproductions are correct; these wording limitations are retained
and do not invalidate the supported, actionable findings.

The metadata-only audit `artifacts/p705-qwen38-360-review-audit.json` binds the
strict grader, raw answer digests, canonical evidence, source integrity and
accounting. Its SHA-256 is
`d7cbfc86ef635eddeb1c3403eeebb6bfe890e742764de243222cfca75577ab91`.
Independent final review agrees with the quality disposition. This closes the
selected review/generation refinement, not broader P7-05 interruption/recovery
acceptance. A small passing pair does not establish a population-level model
ranking or guarantee future completion at this timeout.

The Qwen 3.8 follow-up now totals USD **1.107096** known settled and USD
**12.985616** reserved for earlier unresolved requests. Whole-campaign totals are
USD **1.247562** settled and USD **34.127269** reserved, within the USD 100 cap.
Earlier failures and reservations remain intact. The new summary
`artifacts/p705-qwen38-360-summary.json` has SHA-256
`157c53cc06b07a1a3005d004a65531e91db3099e9110e37a9d5a34526b773cad`.

For the 360-second change, nine profile-runner contracts, four native settings
tests, three provider-timeout tests (both stores), one retry regression (both
stores) and two adapter tests pass. Adapter build, affected Clippy (existing
warnings), changed Rust formatting and diff checks pass. All 17 repository fast
stages pass at
`artifacts/p705-qwen38-360-fast/86db5e11-9979-4edd-9271-57a1b520bc9b/manifest.json`.
