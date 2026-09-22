# P7 paired live qualification — 2026-09-22

Follow-up: [P7-05 review and generation quality](p7-review-generation-quality-2026-09-22.md)
records the corrected grader and fresh retests. The observations below remain the
original campaign results.

The owner authorized a new campaign capped at USD 100, with only necessary spend,
checkpoint delivery and paired Luna/Qwen testing. Routine future live comparisons
use both `openai/gpt-5.6-luna` and `qwen/qwen3-coder-next`, with independently
qualified provider snapshots and matched frozen tasks. This is an evaluation
policy, not a model default or evidence that either model is universally better.

P7-04/05/06 have implemented delegation, integration, controls and recovery paths.
The results below qualify specific increments; the complete usefulness and
multi-child interruption gates remain open. All failed attempts are retained.

## Review and generation

| Model and case | Observed result | Wall time | Settled USD |
| --- | --- | ---: | ---: |
| Luna single-agent review | Formal gate failed; see finding audit below | 43.514 s | 0.010230 |
| Luna child review | Two seeded defects, one false positive; formal gate failed | 46.283 s | 0.008402 |
| Luna child generation | 44/44 checks passed against the integrated parent | 82.041 s | 0.020804 |
| Qwen single-agent review | Provider HTTP 429 on second attempt; child arm not run | 8.326 s | 0.000666 known |
| Qwen child generation | Stopped before integration; no passing parent result | 123.175 s | 0.014649 |

Luna's baseline review did identify both underlying defects. Its shipping finding
did not establish introduction by the reviewed change; the tax finding supplied
opaque evidence references resolving to the correct base, current source and
contract, which the literal-path grader did not recognize. Correcting that
interpretation still leaves only one fully qualified finding, so the baseline
does not pass. The child review's extra negative-input finding speculated about
behavior outside the accepted contract. Neither review arm passes the formal
usefulness gate; the raw baseline score must not be presented as zero defects
noticed.

Luna generation preserved the staged, unstaged and explicitly included untracked
parent state, retained a concurrent human edit, and passed current-parent
verification after integration. Qwen's failed generation retained canonical
history and settled all 16 attempts. Its failure is not a passing child or parent
quality result. Qwen exhausted the frozen 16-request limit; the 900-second coding
deadline was not the cause. Retained output includes repeated reads, a malformed
patch, rejected child process attempts and irrelevant MCP calls. Its eventual
patch also accepted primitive options contrary to the frozen contract. Increasing
the request limit after seeing this outcome would be a separately labeled
diagnostic, not a replacement result. These few runs do not establish a speed or
quality advantage for either model, or a population-level model ranking.

Qwen review retains a conservative USD 0.098651 unresolved upper bound after the
429. There is no provider request identity with which to prove a zero charge.
Known charges and unknown exposure remain separate in the campaign ledger.

## Terminal controls

The initial live terminal cases used the packaged CLI SHA-256
`0a6285bc130914ed38ed50ed0508f7f2d3b7cdbe1919cf8b0c287aa467436a4d`,
a native ConPTY driver and read-only synthetic two-file workspaces. They preserve
the claimed plan, full terminal output, canonical inspections and two recovery
exports. Inspection and reopening cannot initiate a provider request.

The first valid Luna case observed canonical child pause but requested resume
before the parent pump drained. The CLI correctly rejected it; the harness then
timed out. That run is inconclusive for same-process resume. Its separate
hard-close/reopen case passed with stable exports, paused root and child, and no
active reservation. Manual checks confirmed both frozen workspaces were unchanged.
The failed run retains USD 0.001524 settled and USD 3.982713 unresolved liability;
interrupted in-flight requests are not treated as free.

A fresh run with corrected drain sequencing reached `/resume`, which then
reported `canonical capture/admission fenced; reopen required`. This exposed a
separate lifecycle failure under actual provider interruption. Its hard-close
case passed; the campaign retains USD 0.000739 settled and USD 5.310284 unresolved
across those two cases. Diagnosis found a response callback arriving after pause
had durably aborted the capture and retained its uncertain liability. The absent
writer then unnecessarily fenced the owner.

The correction ignores late bytes only when the exact attempt is already pending
reconciliation with uncertainty, its writer is absent and its provider parser is
absent. It cannot accept new evidence or settle cost. Unknown attempts, current
or settled attempts and real capture failures still error. The native regression
invokes retained normal/error callbacks after cancellation, verifies unchanged
canonical evidence, then resumes paused cases through the same owner and completes a fresh request.
It passed on both stores for pause and cancellation; the separate real request/
response capacity-failure regression also passed and still fences admission.

The rebuilt CLI (`50fef9aa0d598cd3a94c98bfbb2f7b25a07d917e30d9fd10fea50cd73ef07166`)
then completed live pause, explicit resume acknowledgement and ordinary exit in
6.913 seconds. The harness incorrectly expected zero; the CLI's documented exit
7 means unresolved effects, while 8 means durable pause without higher-priority
conditions. The final public result confirmed durable pause and no internal
failure. Its separate hard-close case passed in 5.538 seconds. The original
grader failure is retained with USD 0.001505 settled and USD 5.310284 unresolved;
the exit-contract correction does not rewrite that result.

The final exit grader checks post-`/exit` public JSON against the native driver
code and discovered root, requiring durable pause and no internal failure. It
accepts only the documented 7/8 outcomes with matching uncertainty conditions.
The corrected Luna run passed both cases. The matched Qwen run passed hard-close
but encountered the separate, valid producer-quiescence resume barrier after the
parent had drained. Its failed pause case is retained (USD 0.001018 settled and
USD 0.295953 unresolved across both cases).

Only the two exact transient readiness rejections are now retryable, with a fresh
canonical child-paused inspection after each rejection, at most eight probes and
a five-second deadline. Every other rejection fails immediately. A fresh Qwen
diagnostic passed both cases. No native behavior or authority check changed for
that harness correction, and the original matched failure was not overwritten.

| Passing terminal observation | Pause/resume | Hard-close/reopen | Known settled USD | Unresolved upper bound USD |
| --- | ---: | ---: | ---: | ---: |
| Luna final paired run | 6.783 s | 5.118 s | 0.001454 | 5.310284 |
| Qwen follow-up after readiness correction | 6.129 s | 4.430 s | 0.000490 | 0.295953 |

Each case retained exact workspace bytes, paused root/child state, stable recovery
exports and zero active liability. These are control/recovery observations, not
task completion, an aggregate latency ranking or proof that interrupted requests
were free. Final campaign totals are USD **0.066773 known settled** and
**20.604122 reserved for unresolved outcomes**, with no prepared run remaining.
Earlier campaigns' charges are recorded separately; no additional spend is
needed for this checkpoint.

Earlier setup failures remain accounted for: profile/startup failures before any
attempt reconciled to zero; an invalid child-scope fixture cost USD 0.005292 with
all attempts settled. Claimed plans are never replayed. Harness corrections get
new frozen plans and do not overwrite failed evidence.

## Native regression and evidence

Before the final callback correction, native checks passed: 159 core tests (two existing ignores),
67 CLI library tests (one existing ignore), seven native PTY cases, four
current-parent verification cases, two adapter queue/deadline tests, and three
transitive cleanup dependency regressions. A separate two-case cleanup fault
increment qualifies graceful failure and real process termination before receipt
publication on both stores. Clippy passed for the affected CLI/lifecycle/repository/
engine targets with warnings; no warning-free claim is made.

After the correction, all seven native CLI terminal cases passed again and the
P8 recovery/history matrix passed all 21 executable rows on the rebuilt package.
The final repository fast suite passed all 17 stages; the affected delegation
stage passed again after the last readiness adjustment. Final changed-file
formatting and affected CLI/lifecycle Clippy checks passed. The original failed
upstream inventory check is retained: generated Cargo output was preserved under
ignored artifacts, and the pinned source check then passed.

Private full artifacts remain under `artifacts/` and owner temporary directories.
The owner campaign ledger is `artifacts/p7-p8-owner-campaign.json`; manual review
interpretation, generation reconciliation and terminal reconciliation receipts
are stored beside it. Frozen plan hashes, binaries, source inventories and logs
bind the observations without publishing credentials or private paths.

The remaining P7 acceptance includes a passing live review usefulness comparison,
multi-child/consumer-loss coverage and the remaining integration-interruption
schedule. Executable child processes remain unavailable without a qualified
filesystem sandbox. Distribution and recovery qualification are a separate P8
increment; this checkpoint does not qualify a release.
