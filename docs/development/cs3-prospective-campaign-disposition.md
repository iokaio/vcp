# CS-3 prospective campaign — terminal provider/accounting halt

Status: halted September 28, 2026. CS-3 is not complete or merge-ready. No
candidate qualified or was promoted. The owner-approved historical-UI replacement
remains valid, but its prospective frontend block was never opened.

## Frozen execution and observations

Source commit `fd0c1145` froze the approved replacement decision, all four raw
prerequisite gates and the successful qualification build. The 108-slot plan has
SHA-256 `960659d8fe45b930f8a5d5fe00e1723b79f70c80b74bfd7bfa5ae2513704dbf9`.
It resides under the protected private parent
`D:/vcp-private/cs3-approved-prospective-20260928/campaign`. Its model was
`deepseek/deepseek-v3.2`, pinned to OpenRouter `gmicloud/fp8`, with sixteen requests
and USD 0.60 maximum per slot, no transport retries and no replay. Preparation
made zero model calls; subsequent execution consumed seven slots.

The first six slots covered both document-authoring normal tasks across all
three arms. All failed to return a usable canonical final JSON answer. They
settled twelve requests for 9,275 microdollars. Read-only inspection of all three
first-task arms showed valid initial `vcp_list` calls followed by incomplete
`vcp_read` argument JSON ending at `"start_line": `. Concatenated argument deltas
and both terminal snapshots matched exactly; VCP did not discard a complete
streamed call. The endpoint marked those malformed responses complete. These
observations do not establish whether generation or provider adaptation caused
the truncation, and they do not justify replay or inferred candidate benefit.

The seventh slot, `DOC-cs3-boundary-authority-v1--candidate`, submitted its sole
attempt at `2026-09-28T23:27:50.438Z`. Its retained response was a 502-byte HTTP
400 error naming GMICloud and reporting: “The requested model has been deprecated
and is no longer available.” It contained no generation/request ID, usage or
cost evidence. Artifact `efe87d19-4cc7-4bbe-ad73-8a33e6961fd1` has SHA-256
`31bb88f24949b12f8c8070a8030fe12c8013b37beb5b2a199933510be27f6715` and remains
`aborted`, not a completed response. The ledger retained 104,265 microdollars of
unresolved liability with zero active reservation and no overrun. The runner
then wrote its permanent halt and made no further calls.

The [public endpoint catalog](https://openrouter.ai/api/v1/models/deepseek/deepseek-v3.2/endpoints)
still listed `gmicloud/fp8` when checked after the error. That metadata discrepancy
does not establish model-wide deprecation, successful execution or zero charge.
The [generation metadata API](https://openrouter.ai/docs/api/api-reference/generations/get-generation)
requires an observed generation ID; none exists in this response. Endpoint-level
[daily activity](https://openrouter.ai/docs/api/api-reference/analytics/get-user-activity)
is aggregated and cannot independently attribute a charge to this attempt.

## Reconciled inventory, not a liability settlement

The read-only audit authenticated the plan/claim, unchanged source and all raw
prerequisites, exact retained evidence for six settled slots, the seventh
canonical cost ledger, all 108 preserved workspaces and all 101 untouched slots.
It did not modify canonical state or discharge the unresolved amount.

| Accounting category | Microdollars | USD |
|---|---:|---:|
| Six settled comparison slots / twelve requests | 9,275 | 0.009275 |
| Earlier settled qualification pair / two requests | 197 | 0.000197 |
| Total known settled charges | 9,472 | 0.009472 |
| Seventh slot unresolved reservation / one attempt | 104,265 | 0.104265 |
| Known settled plus reserved upper bound | 113,737 | 0.113737 |

The last row is a conservative bound, not an observed bill. The stopped block's
`actual_cost_micros: null` is preserved. Its `observed_attempts: 12` counts only
successfully reconciled attempts; canonical ledgers contain thirteen attempted
requests, plus the two earlier qualification requests outside this campaign.
No funds were reallocated and no paid reviewer, grader, confirmation or retry ran.

The local audit is `artifacts/cs3-approved-campaign-halt-audit.json`, SHA-256
`aee142e98130ec8ca6fb4f4550c61354bcef5add3eee03b18586820596591d67`.
Stopped block SHA-256:
`fbf764423ee6e7aef21ff548e3336e17a7e609294c8f749d37879adba1656818`.
Halt SHA-256:
`fac2f39ae068aca8dddfe7ebebe5eef0da9721078fb753988f696c273649f500`.
The frozen evaluation README retains its preparation-time state; this record
supersedes its no-dispatch status without mutating the source-bound campaign.

## Remaining acceptance and required external evidence

There is no complete eighteen-slot block, so independent blind packets and
dispositions cannot be prepared. The other five skill blocks, prospective UI
artifact grading and all six comparison qualifications remain incomplete.
Existing browser, WEB, adversarial, UI-control and package evidence remains valid;
it cannot stand in for comparison acceptance or the missing historical outputs.

Continuation first needs attributable OpenRouter billing/support evidence for
the failed GMICloud request above, including its actual charge or explicit
no-charge confirmation. HTTP 400 alone is not a usage settlement. The current
CLI exposes inspection but no accounting-settlement command; the canonical
`Host::observe_usage` API requires source-bound usage evidence and an explicit
audited consumer. No ledger was edited directly or settlement invented.

Even after reconciliation, this halted campaign cannot resume or replay consumed
slots. A separately reviewed successor must preserve this halt and all failures,
qualify a working provider endpoint and freeze new assignments, source identities
and explicit remaining dollar/request allocations before any further paid call.
No such successor is currently authorized by this frozen envelope. No acceptance
criterion has been waived and PR #199 remains draft.

## Delivery checks

At `fd0c1145`, 18 focused admission tests, ten synthetic host integration tests,
the registered 80-assertion CS-3 contract run and repository check passed. The
actual locked/offline qualification build passed unchanged-source/toolchain
checks. The [PR delivery run](https://github.com/iokaio/vcp/actions/runs/36497681947)
passed its repository/harness job; optional native Windows qualification and
storage-handoff CI jobs were skipped, not passed. Actual local native receipts
remain linked from the [delivery checkpoint](cs3-delivery-checkpoint.md).
Its downloaded fast-suite manifest reports 23/23 cases passed at
`artifacts/cs3-ci-36497681947/0d7e31a2-a7c8-46e5-9901-a72e49bd119e/manifest.json`,
SHA-256 `6cdc172417a06280a6c04aa13141dea409bb13212ce2f747c7a3d7a97942921b`.
The final documentation check inspected 657 Markdown files and 2,950 relative
links with no errors; whitespace checks passed.
Independent read-only final native/validator review found no additional
high-confidence merge-blocking defect; the accounting and comparison gates above
still block completion.
