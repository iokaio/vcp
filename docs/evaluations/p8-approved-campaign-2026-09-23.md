# P8 approved owner and interactive campaign — 2026-09-23

Status: six owner attempts finished; two passed automated controls, four failed.
Live interactive lifecycle observed with unknown billing liability retained.
Owning work items are P8-01/04/05. P8-05 acceptance and downstream P9 remain open.

The owner approved the previously prepared six-slot owner campaign ($8 per task,
$48 aggregate, 16 requests per task) and one interactive observation (at most two
requests, $16 additional cap). There are no retries. Approval record:
`artifacts/p8-paid-approval-13c7a02a-b383-430c-a0ba-2e006accb23c.json`.

Both immutable plans passed validation immediately before execution. Owner plan
SHA-256: `c65b8d27a532edf18f94e6dc2de5bbe787a03531c4cdab01b78af965fb93b4fa`.
Interactive plan SHA-256:
`10e6507bc1c931415bd630f030011997b62c36125b37f4d0f5a2355c9eed29ba`.
Their paths and package binding are recorded in the
[continuation report](p8-continuation-2026-09-23.md#remaining-execution-gates).
No frozen plan, launcher, production binary or prior result was changed during
execution. The subsequent runner correction preserves the original runner bytes
separately and applies only to future preparations.

Before launch, the shared campaign ledger retained 2,476,093 settled micro-USD and
50,127,269 reserved micro-USD under its 100,000,000-micro ceiling. No launch was
outstanding. Reservations are acquired per attempt; settlement or a retained
unknown upper bound precedes the next attempt. Additional authorization does not
erase old liabilities or bypass the shared ceiling. Owner attempts run first,
followed by the separate interactive observation when the ledger permits it.

Only actual terminal results, canonical accounting and independent grader
evidence may satisfy automated controls. Human usefulness, correctness and
architecture-fit judgments remain pending. Prior unsuccessful trials retain
their original classification. Clean Windows, offline isolation, minimum hardware
and physical-volume coverage still require their separate environments.

## Owner observations

The frozen runner completed all six attempts without a retry. Its result remains
`failed`, with 1,161,854 micro-USD settled and zero new unknown owner-task charges.
Result under system TEMP:
`vcp-p805-memory-v2-f7d50e63-ea32-423e-a223-2ab4afafe61f/owner-result.json`,
SHA-256 `6e8b97845bb9755f0b2e4777734225833937516da8aef724e05ff1f031b8d59c`.

| Attempt | CLI time | Cost (micro-USD) | Result |
| --- | --- | --- | --- |
| U01 SQLite | 72.362 s | 158,780 | Request-limit pause; no final answer |
| U01 Files | 70.491 s | 152,310 | Request-limit pause; no final answer |
| U02 SQLite | 184.326 s | 174,648 | Automated controls passed; human review pending |
| U02 Files | 160.494 s | 194,826 | Automated controls passed; human review pending |
| U03 SQLite | 270.083 s | 239,278 | Request-limit pause; task incomplete |
| U03 Files | 280.673 s | 242,012 | CLI completed; current-source evidence gate failed |

All six retained passing frozen-input and workspace-preservation checks, with no
credential exposure detected on the runner's declared captured surfaces. The
generation outcomes and supplemental feature checks are separate from completion.

Both generated workspaces passed all 37 hidden feature checks in a separate
unpaid supplemental run of the exact frozen grader command. The SQLite task
remains incomplete; the Files task retains its failed current-source evidence
gate. Grading changed no workspace, Git index, launcher or frozen input. Receipts
under system TEMP are
`vcp-p8-generation-supplement-f267199b-4c7b-4d8e-8d8a-9f8da2a8ae93/u03-sqlite-result.json`
(SHA-256 `fb6301d78945946ffb08819aeb041a14e9c6a27a70fa28404c4051d7e07fbce0`)
and `vcp-p8-generation-supplement-f267199b-4c7b-4d8e-8d8a-9f8da2a8ae93/u03-files-result.json`
(SHA-256 `580a28408146517fbd04bbd3887fc70dc4e3806e7afc41e969592ffd6a568034`).

SQLite's retained provider responses requested `--version` and `-e` custom checks
through the restricted Node launcher. Both were correctly refused: the model
had read the exact permitted command in `AGENTS.md` and `package.json`. The
permitted `--test test/page.test.cjs` invocation also ran successfully. These two
refusals do not demonstrate a defect in the qualified v2 launcher.

The Files current-source gate failure was a validator lookup defect. The runner
searched the outputs view for `verification-result/1`; that artifact uses the
evidence channel and is returned by the context view. The already retained
context inspection contains artifact `a8af920c-47e4-4f9a-96d6-1d3225f2ff4b`,
SHA-256 `c5479f7fa62ba4efe3fd7d105532f9597e20407fa5e321faa6c6b4e4ad2639bc`.
Independent audit verified the captured bytes and length, current applicability,
equal before/after manifests and all nine current workspace file hashes/sizes.
Both successful verification records reference that artifact and match the final
completed task's fingerprint. The original slot remains failed; this separate
audit identifies a harness failure rather than missing production evidence.
The follow-up fixes the view selection without relaxing current-parent,
verification, hash or source-manifest checks and without another paid attempt.
The separate audit is under system TEMP at
`vcp-p805-source-audit-33624d00-27c4-4209-b0da-57eeeb7fe0f3/u03-files-source-audit.json`,
SHA-256 `57e2d2a2d0d649cf7e80e0918536d65ea4bda7f98cb6cad556e2d1b50c9365ab`.
The same directory preserves `p805-owner-runner.frozen.cjs`, SHA-256
`c931a057609d936d6b31e4465f9d07fabf06b2ba3e34158c49b31d7e2ff92bcd`,
as the exact original execution script. The original result hash remains unchanged.

Both U01 analysis attempts reached the exact 16-request limit and paused durably
without a final answer. They did not exhaust the $8 task cap or 900-second
deadline. SQLite settled 158,780 micro-USD after 18 successful read/list effects;
Files settled 152,310 after 17 successful read/list effects. Neither retained
active or unresolved liability. Frozen-input and workspace-preservation checks
passed. These remain unsuccessful owner tasks despite successful tool execution.

Both U02 reviews completed and passed automated controls: SQLite at 174,648
micro-USD and Files at 194,826. Supplemental independent static review supported
both reported boundary defects in each answer, found no unsupported defect
findings or important misses in the three-file fixture, and confirmed all ten
frozen workspace hashes for each store. SQLite's aside about JavaScript conversion
differences for `-0`, `null` and generic objects was inaccurate. Files reversed the
out-of-contract Symbol conversion example; `String(symbol)` succeeds while
template interpolation throws. Neither aside was a defect finding. Files also
initially described verification as running checks, then explicitly reported the
empty configured check list and no executable checks. No executable reproductions
were performed by this supplemental review, and it does not replace human
acceptance. Answer SHA-256 values are
`c3de36a659c3b8d1292b5cee2e2758a9362a90ce687db824018432f880cc3246`
(SQLite) and
`f1a9169b744c480efba7bab6e8a655dff35ca027f237eacf195f89832c854402`
(Files).

Investigation of all 32 captured U01 provider requests found no remaining-request
allowance in model-visible context. Multi-call responses and cross-file search
are already supported; the model predominantly chose sequential exploration.
The discovered observability omission is being corrected separately under P6-03
and P2-08, without changing these frozen trials or increasing their cap. It does
not establish that guidance alone or a higher cap would complete the tasks.

## Live interactive observation

The approved observation ran exactly once against the same production package.
Its result is `observed`: canonical submission preceded pause, pause acknowledged
in 29 ms, and no new canonical attempt IDs appeared during the one-second paused
window. After parent drain, explicit resume acknowledged in 67 ms with the same
process and task. Exit completed in 66 ms without forced termination. Fresh
production reopen and canonical recovery export both retained the paused root.
Exit was the explicit `/exit` command; console/window closure was not exercised.
Total runner time was 5.301 seconds. This is one lifecycle observation, not a
statistical latency result or a network delivery/counting oracle.

The structured exit was 7, with `durably_paused` and `unresolved_effect` true.
Both interrupted attempts remain `reconciliation_pending`; actual provider cost
is unknown. The recovered ledger has zero active/settled amounts and 12,985,616
micro-USD unresolved liability. The runner retained the entire
16,000,000-micro-USD reservation.
No zero-cost, complete-accounting or task-completion pass is inferred from the
lifecycle result. Including prior held reservations, the shared campaign now
records 3,637,947 settled and 66,127,269 reserved micro-USD under its unchanged
100,000,000-micro ceiling. No new attempt is authorized by the unspent difference.

Result under system TEMP:
`vcp-p8-interactive-memory-fd2ac140-7b99-49cb-a14c-800734a65597/result.json`,
SHA-256 `dc97116e3b9bafbb010ee0e30747a94d53e04f5f141602963d33587accf4588c`.
The receipt records unchanged frozen inputs, terminal/driver stream hashes,
fresh recovery snapshot hash and captured command outcomes. The earlier failed
interactive trial and its separate unresolved reservation remain unchanged.
Independent read-only audit confirmed the same process identity, paused/running/
paused task revisions and retained liabilities. Recovery snapshot SHA-256:
`dcb691999f7c14d73fabd2e6324cc5a560dc0176aa233d9e54e4335bc370c532`.

## Current-package distribution

The existing production distribution runner was executed once against the current
memory-enabled package and a distinct previous package. All 16 commands and 32
assertions passed in 25.77 seconds: private installation, interrupted upgrade and
retry, compatible state round-trip, locked rollback/uninstall and preserved user
data. This used a fresh private directory on the current host, not a clean OS.
No provider calls, global PATH changes or OS configuration changes occurred.

Receipt under system TEMP:
`vcp-p8-memory-distribution-20260923-10b84930-b691-48ba-9971-e6c7f6ca5cd2/97d38ab8-2d08-4cfa-9715-88e338e840a1/result.json`.
SHA-256: `55436421dd7234d7ed7ccbe761a12cfedc2ff99823e29d7f26aa3338589e2d40`.
The candidate executable is `28c32ddd52c89c4c02b994911162c2b1db229b96f7cf365b20aa6ce1748bf3e3`;
the actual previous executable is
`e5f09a9005f54670197427e707fc6fa30e439d8ce20d25f6d7cf90a8abbbee62`.
Runner/package receipt hashes were unchanged before and after execution, and all
32 captured command-output stream hashes were checked.

## Follow-up validation

All 17 fast-suite groups passed before the view-selection correction; manifest:
`artifacts/p8-approved-campaign-fast/08d8c75d-3987-4ff5-8d74-f4308b4ab0ed/manifest.json`
(SHA-256 `07ee618291e54ba5a239277460aa0f2b83b3e163c4424bbd8977891169e5e09e`).
After that correction, all 12 owner-runner tests passed, including the new
evidence-view lookup and rejection of bad hashes, stale evidence, mismatched
manifests, observation errors, later edits and unreferenced artifacts. Repository
documentation contracts and diff checks also passed. Independent review checked
the recorded lifecycle claims and remaining acceptance boundaries.
