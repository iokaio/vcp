# A/B qualification repair campaign

Work item: CLI A/B qualification maintenance, authorized October 4, 2026.
Keep repository changes local on `fix/a-b-tests`; no push, PR, or merge.
The shared authorization is USD 100 across all campaign attempts, not per
scenario. The supervising agent owns diagnosis, repair, and the decision to
launch the next attempt. The controller never performs speculative paid retries.

```powershell
pwsh -NoProfile -File docs/test-plans/run-ab-campaign.ps1 -Action Status
pwsh -NoProfile -File docs/test-plans/run-ab-campaign.ps1 -Action Run `
  -Scenario A -Mode Full -Vcp D:\local-build\vcp.exe `
  -ProjectPath D:\clitests\ab-campaign-20261004\A `
  -TurnBudgetUsd 5 -MaxAttemptUsd 30 -AllowProcessPublish
```

Use `-Mode DryRun` first, and repeat for B with its separate project directory.
`-ProviderGeneration` selects retained provider metadata explicitly; omitting
it uses the existing launcher's installed-provider discovery and metadata-only
refresh. Every invocation requires an explicit local executable. The controller
retains its SHA256, repository revision and diff hash, authored project/harness
fingerprint, run ID, captured launcher output, scenario scorecard path, failed
gates, and paid stopping conditions. The original scenario logs retain detailed
task, tool, inspection, and accounting evidence.

`campaign.json` is atomically replaced under an exclusive OS lock held only
while reserving or finalizing an attempt. Separate project locks protect each
workspace throughout execution, allowing A and B to run concurrently. Status
reads do not create or modify state. Before launching, the full attempt cap is
durably reserved against the shared ceiling. Only a matching final scorecard with
complete settled accounting can release unused funds. Missing evidence,
timeouts, and unknown cost retain the full reservation and stop new launches.
Active supervised attempts retain their cap while independent projects can
reserve from the remaining balance. A terminated controller cannot forget
potential spend; an orphaned project lock requires reconciliation.
`-Action Reconcile -AttemptId <id>` can recover an interrupted controller when
a matching completed scorecard exists. Incomplete task accounting must be
investigated and reconciled through VCP before a complete scorecard is available;
editing the campaign ledger to erase liabilities is not recovery.

If the original scorecard is incomplete because canonical inspection timed out,
`-Action Reconcile -AttemptId <id> -Vcp <local-candidate> -RefreshAccounting`
can collect fresh accounting after the supervisor has exited. This path inventories
all started/completed native commands and accepted/result JSONL, including paid
tasks omitted from the scorecard by an evidence-collection exception. It refuses
unscoped or truncated dispatch evidence. Each unique task is inspected once using
only `inspect-bundle`, with provider credentials denied; resumed tasks use their
one cumulative ledger. Complete settled accounting, stopped tasks, terminal tool
effects and absent native processes are required before releasing unused funds.

Recovery writes a separate hashed receipt that binds the original evidence and
the new executable/inspection output. The original failed scorecard and verdict
remain unchanged. Other attempts can finish while recovery reads canonical state;
the ledger is reloaded under lock before updating only the recovered attempt.
Permanently quarantined attempts are excluded from this recovery path.

An explicit `-Action Quarantine -AttemptId <id> -RepairNote '<diagnosis>'` can
isolate an exited attempt whose only unresolved effects are provider charges.
This requires a matching final failed scorecard, complete canonical inspection
pages for every launched task, stopped tasks with no agents, no native process
referencing the old data directory, zero active reservations, successful tool
effects, and an exact mapping from unresolved ledger liability to provider
attempt reservations. `task.editing` is editing authority, not an indication
that a process is still running. Missing or ambiguous evidence rejects quarantine.

A failed native process can qualify for financial isolation only when its five
original prepared/start/outcome/stdout/stderr artifacts are complete and
hash-verified, the operation digest and full scope/execution/effect identities
agree, the known exit is nonzero, and the observed process job is empty with
complete output. A checkpoint published before process creation must exactly
match the prepared and final authored inventories, its retained files, and the
current authored workspace. Unknown/cancelled effects, partial output, surviving
processes, altered sources, missing artifacts and ambiguous identities still
reject quarantine. The proof records external effects as opaque under the
original reduced isolation; it does not claim the failed test passed or prove
absence of all external side effects. Permanent workspace isolation, unknown
billing and the full original root-cap allocation remain mandatory. Bounding
revalidates this proof against both original and fresh canonical bundles.

Quarantine preserves the original native state, failed scorecard and unknown
charges. It writes a separate create-new evidence record with source hashes and
initially counts the **entire attempt cap** against USD 100. Reconciliation
cannot refund this campaign allocation, and future campaign attempts cannot use
the old workspace or a containing/contained directory. A fresh independent
project can then reserve from the remaining balance after a diagnosed repair.
This operation does not resume the old task, replay tools, settle charges, or
interpret a provider receipt HTTP 404 as evidence of zero cost.

The explicit maintenance action `-Action BoundQuarantine -AttemptId <id> -Vcp
<local-executable> -RepairNote '<evidence-based diagnosis>'` supersedes only the
unused, never-launched portion of that initial hold. It retains the **sum of the
original native root caps**, including settled spend and all still-unknown bills;
there is no caller-supplied lower amount. This requires the complete native
start/completion inventory, one explicit original budget per dispatched root,
unchanged hashed quarantine sources, and fresh credential-denied canonical
inspection of every root. Missing/truncated/unscoped calls, continuations,
active work, agents, unknown tool outcomes, overruns, child allocations and
zero-priced unknown provider attempts all reject the operation. Fixed-provider
routing's explicit absence of automatic routing decisions is not an accounting
gap. Original and fresh ledgers must agree with each root's original cap.

The project lock remains held throughout; native process checks establish
quiescence before and after inspection, and orphaned campaign work blocks the
action. A separate create-new, hashed proof records the previous allocation and
retained root caps. Publication reloads the campaign under its ledger lock and
requires the original attempt to remain unchanged. The original attempt cap,
quarantine proof, failed scorecard, native ledgers and `billing-quarantined`
status remain unchanged; only `liability_usd` and the new `bounded_liability`
audit field change. A second bound and ordinary reconciliation remain forbidden,
as does all reuse of the quarantined workspace. This is conservative campaign
allocation accounting, not provider settlement or a guarantee against an
external provider charging beyond its admitted reservation (P1-05).

After a failed paid run, inspect its `failure-bundle.json` and scorecard, repair
the responsible project/harness/runtime boundary, run the focused regression,
and invoke again with `-RepairNote 'diagnosis and concrete repair'`. Unchanged
inputs, including the previous run's final workspace, are refused. Per-attempt
limits, reservation admission, authorization, protected-file checks, and stopping
conditions continue to be enforced by the existing harness. Read-only review
uses the configured turn budget: a fixed USD 2 review cap could not admit the
selected provider's roughly USD 2.524 conservative request reservation.

The first controller performs complete scenario reruns. It does not claim stage
checkpoint replay or unaided success after external project repair. Final
qualification requires fresh projects and full A/B scorecards. Concurrent
qualification runs use this same controller and ledger, one invocation per
project, so both full caps are reserved before their respective paid execution.
Automatic stage replay is not implemented here. A stage that still fails after
its bounded repairs stops the scenario before dependent paid work begins.

Each subsequent `Save-Checkpoint` also retains authored source under the run's
`checkpoints/<unique-id>/files/`, including for reused projects whose Git index
and history remain untouched. Its completion `manifest.json` records workspace,
stage message, time, per-file SHA256 values and the exclusion policy. The harness
checks copied bytes against the original manifest and checks the source again
before publishing that manifest; links, escaped paths and concurrent changes
stop the scenario before another paid stage. Incomplete directories without a
completion manifest are not checkpoints. Dependency/build directories retain
the existing exclusions, while `models`, `models-repro` and `reports` are excluded
only at workspace root so authored nested `Models` and `Pages/Reports` survive.
This adds restoration evidence, not automatic stage replay. Checkpoint tests
passed 20 checks; project-context (61), harness (57) and A/B reuse tests also
passed. Earlier runs without these manifests are not retrospectively checkpointed.

Offline checks (no provider calls):

```powershell
pwsh -NoProfile -File docs/test-plans/tests/Campaign.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/BillingQuarantine.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/BoundQuarantine.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/AccountingRecovery.Tests.ps1
pwsh -NoProfile -File docs/test-plans/tests/Harness.Tests.ps1
```

## Initial failures and reproduction evidence

The October 4 T1 failures were ordinary canonical budget refusals. A had settled
USD 0.479877 and B USD 0.518799 against separate USD 3 task caps. Each next
request required USD 2.523880, so the totals exceeded those caps. Both ledgers
had zero active and unresolved liability. The later `owning host closed` reason
came from task cleanup after the budget refusal. These endpoints lack qualified
tokenizer bounds, so admission deliberately reserves the full input capacity
plus cache/output/request bounds (ADR-081); the campaign raises the task cap to
USD 5 within its shared USD 100 ceiling instead of weakening that reservation.

A's independent `spawn UNKNOWN` failure was reproduced without inference on
the same 64-logical-CPU host, using the captured cleared process environment
and a Windows Job Object with the same 32-process limit as VCP. Vitest 3.2.7
defaults to 63 fork workers: the original configuration failed before any test
ran with `errno: -4094, syscall: spawn`. The same test invocation with
`--maxWorkers=2` passed all five tests in three files. The scaffold now records
`maxWorkers: 2`; task prompts preserve it and permit repairing the configuration
in reused projects. VCP's process limit remains 32. Existing nonfatal App test
fetch warnings remained in the successful reproduction.

`tests/VitestWorkers.Tests.ps1` verifies the scaffold, prompt and editable-path
contract; it passed along with `tests/ProjectReuseAB.Tests.ps1`. The temporary
native-job reproduction helper was removed after the check.

The later Haiku B attempt's 1,017-commit history took 129.7 seconds to inspect
with the unoptimized 0.2.17 candidate; its following response-range inspection
timed out at 120.1 seconds. Range reads initially received the same 300-second allowance
as inspection bundles. All 56 offline harness checks passed, including simulated
130-second startup and rejection of incomplete response bytes.

An optimized 0.2.19 candidate inspected a disposable copy of that history in
29.843 seconds (14.938 seconds CPU), with no storage validation changes. Native
JSON comparisons matched schema, kind, watermark, task, agents, all six views
and all eight history pages exactly. The copied SQLite database's SHA256 was
unchanged. Only the copy's workspace descriptor was relocated to its copied
canonical directory; the original evidence was untouched. The local receipt is
`C:\vcp-scenarios\ab-campaign-20261004\benchmarks\b-replay-release19\release19-benchmark.json`.
This demonstrates faster inspection for the measured history, not a bound for
larger histories or completion of the full scenarios.

The optimized B candidate subsequently required 268 seconds to inspect its
3,558-watermark T2 history. Bundle, response-range and metadata-recovery reads now
have a bounded 1,800-second allowance for later stages. Paid provider deadlines,
the overall attempt timeout and all evidence completeness checks are unchanged.
The updated harness passed 57 offline checks; accounting recovery passed 30.

The P1-04 replay follow-up transfers privately owned event and receipt history
between reconstructed states instead of deep-copying it for every commit.
Every commit still receives complete validation and receipt comparison. Public
`prepare`/`replay` retain their unchanged-on-error contract. A restricted prior
record view prevents publication checks from depending on transferred history;
its receipt lookup excludes exactly the newly inserted transaction.

On the same disposable B history, the isolated optimized store-open helper
improved from 17.355–18.470 seconds before the change to 14.546 seconds after it
(about 18% against the nearest 17.665-second baseline). All runs reconstructed
the same complete state digest; the database hash stayed unchanged. The timing
is a local measurement under concurrent workloads, not a scaling guarantee:
full validation on every commit still makes total replay quadratic in retained
history size. The opt-in helper is `tests/replay_validation.rs`; set
`VCP_STORE_BENCHMARK_ROOT` only to a disposable canonical copy.

The focused release-profile gates passed 36 retained tests: 17 library tests
(including invalid committed semantics on both backends, public replay rollback,
and prior-receipt visibility), one atomic-fork test, six conformance tests, seven
controller tests, and five persisted-JSON tests. The environment-specific
OneDrive test remained ignored. Search publication tests now run inside the
library to exercise the private record-view boundary without adding public API.

## Local verification checkpoint — October 4, 2026

The offline sweep after the quarantine review ran every
`docs/test-plans/tests/*.Tests.ps1` file: **23 suites passed, zero failed**.
This includes 33 billing-quarantine checks, 34 campaign checks, 580 blocked
execution/repair checks, paid-dispatch exception accounting, scenario gates,
process isolation, provider metadata handling, and all existing project reuse
checks. No provider inference was performed by this sweep.

The 07:39 UTC checkpoint used
`artifacts/a-b-tests-candidates/0.2.19/vcp.exe`. Direct `--version` reports
`vcp 0.2.19`; SHA256 is
`ed6134cf1125a1a319a163d7cef1201534f24e54f1dfeea74a1675353ca0c888`.
These are executable checks, not installer or VSIX verification.
Tool-guidance validation passed 17 native tests (14 preparation and 3 process);
`node --test scripts/release/local-version.test.cjs` separately passed all 16
version-synchronization tests.

Both initial local-candidate dry runs passed. Subsequent Azure-backed full
attempts stopped on HTTP 429 with unresolved provider billing; both were
quarantined with their entire USD 30 allocations permanently retained. The
original records remain failed and financially unresolved. The subsequent
0.2.17 Haiku A attempt hit its task deadline and then the 300-second inspection
timeout. Metadata-only recovery with 0.2.19 completed successfully and established
USD 3.091995 settled cost. Its original failed scorecard remains byte-identical:
SHA256 `380c05a0f98cbf1da7bd58dd21d8b6c520b162630ee3d2179e476aa309815290`
matches the separate `accounting-recovery.json` receipt. Recovery changed only
campaign financial accounting; the scenario verdict remains failed.

At the 07:39 UTC checkpoint, fresh 0.2.19 A/B attempts using owner-permitted
direct Anthropic Haiku were running in `A-retry2` and `B-retry2`:
A attempt `20261004-073910-A-ee5fcd0a`
has a USD 17 cap; B attempt `20261004-073302-B-27859ff8` has a USD 18 cap.
At this checkpoint, permanent quarantine holds total USD 60, prior settled
attempts total USD 4.250331, and live allocations total USD 35. The combined
commitment is USD 99.250331 against the USD 100 authorization. Full end-to-end
qualification remains pending; dry-run passes and recovered accounting do not
establish scenario success. Live evidence is retained under
`C:\vcp-scenarios\ab-campaign-20261004\campaign.json` and its attempt directories.

The subsequent bounded accounting-recovery addition passed 30 focused offline
checks, plus the 34 campaign and 33 quarantine checks. Its original-dispatch
inventory also correctly identifies the one scoped task in A's actual failed
attempt `20261004-070007-A-4091b8eb`, despite the timed-out inspection.

At the 08:28 UTC checkpoint, the new optimized local candidate is
`artifacts/a-b-tests-candidates/0.2.20/vcp.exe`, with recorded actual version
`vcp 0.2.20` and SHA256
`85d4bfce00a4622d9c270022eb158fda9471591d632e65d92c2c944688fe59ee`.
Its build receipt records exit 0 and the verified executable identity; the source
guard passed, as did 16 version-synchronization tests and nine native SDK schema
generation tests. No installer or VSIX was produced for this native candidate.

Fresh B Full attempt `20261004-082826-B-d31ab3bc` ran in `B-retry3` with
0.2.20 and a USD 14 cap, then stopped at the T2 task budget. Its failed scorecard
has complete accounting of USD 5.465820 (T1 USD 1.300567; T2 USD 4.165253).
The previous 0.2.19 B attempt remains failed after its
context-capacity stop, with complete settled cost of USD 4.627954. A 0.2.19 attempt
`20261004-073910-A-ee5fcd0a` subsequently stopped with a failed scorecard:
the first T2 UI repair completed natively and passed its application gates, but
the old 300-second inspection timeout left accounting unknown and incorrectly
triggered a second repair. The newly tested stopping rule applies to subsequent
launches; that older process retained its original harness behavior. After it
exited, metadata-only recovery with 0.2.20 established complete settled cost of
USD 4.650881 and released the unused allocation. The original failed scorecard
remains unchanged (SHA256
`3c799c67ca6bb6aab2d08e521d4a09c31cac7d820bd578ca8999b63b84cc5489`);
the separate recovery receipt records all five inspected task scopes.

The ledger at this checkpoint retains USD 60 permanently for the two billing
quarantines and USD 18.994986 for accounted failed attempts:
**USD 78.994986 committed**, leaving USD 21.005014 of the USD 100
authorization. Neither scenario has a full passing run.

The complete current offline harness suite passed **27 of 27 scripts** after
the T5 reconciliation integration, including the 84-check deadline-reconciliation
suite. Each script ran in its own PowerShell process; no provider inference was
used. Per-script exit codes and logs are retained under
`artifacts/a-b-tests-candidates/offline-harness-20261004-final/`, with the summary
in `results.json`. This supersedes the earlier combined-sweep checkpoint without
changing the real scenario verdicts.

At the 09:30 UTC checkpoint, both original billing quarantines have received
the separately reviewed `BoundQuarantine` assessment. Each now retains its
entire original native root cap of USD 5; billing remains unknown, the original
USD 30 scenario caps and quarantine proofs remain unchanged, and both workspaces
remain isolated. The two new proof hashes were verified against the campaign
ledger. Their paths below are relative to
`C:\vcp-scenarios\ab-campaign-20261004\attempts\`:

- A: `20261004-064322-A-606f1231/bounded-quarantine-a2d370e706a04958a44ca072fc17df77/bounded-liability.json`
- B: `20261004-064322-B-6f08cb9b/bounded-quarantine-614c50fa9f034395aaaf8a547a32644e/bounded-liability.json`

Both current attempts use the actual 0.2.21 executable, SHA256
`acce4e0f104bbbe1add8512f3b7f362c4bce5ae718c04dc1770bb1132a28f90f`.
A attempt `20261004-091143-A-5646b84f` reuses `A-retry2`, rerunning every stage
and gate under a USD 12.40 cap. T1 passed after repair, T2 passed, and T3 labels
is in progress; settled stage cost so far is USD 1.166473. B attempt
`20261004-093021-B-48537c98` has started with Sonnet in fresh `B-retry4`, after
its scaffold-only dry run, with a USD 30 task cap and USD 55 scenario cap.
The campaign now commits **USD 96.394986**: USD 18.994986 accounted spend,
USD 10 retained quarantined root caps, and USD 67.40 active allocations.
Neither full scenario has passed.

There are now 28 offline suite files. The prior complete 27-suite sweep remains
the combined-run evidence; the new bounded-quarantine suite independently passed
61 checks, and recovery (30), original quarantine (33), and campaign (34) checks
were rerun successfully. The Inventory profile suite also passed after adding
the A/B prompt reminder that `executed: false` requires review and repeating the
command. No new combined 28-suite sweep is claimed.

## P2-08 bounded history projection

The 0.2.19 B T2 task `406ada30-7863-4c67-b1ee-ca5d64f4f18f` paused on
canonical context capacity after 52 provider responses, below its 96-request
limit. It made 58 tool calls: 32 reads, 21 patches, three builds and two directory
listings. Nine instruction-scope refreshes, three patch errors and one oversized
read consumed extra calls. Builds exposed duplicate DTOs, a missing EF InMemory
reference, then missing test imports. No test run had succeeded.

Existing compaction was active, but its accumulated previews still left a
192,197-byte serialized request above the unchanged 191,296-unit conservative
input allowance (200,000 context minus 8,192 output and 512 margin). P2-08 now
keeps six complete recent pairs and previews for the newest six compacted pairs;
older pairs retain tool/call identity, original artifact references and omission
counts. Mandatory current facts, every original artifact and every source
hash/revalidation remain unchanged. The versioned algorithm is
`bounded-tool-pair-previews/2`.

Offline replay reconstructed the exact 58-pair history and matched its retained
SHA256 `29f973b2bd23f9b2baf041ae2bcec3d7536ac0e1b1f96d734ddba3ccf0c4312c`.
Native projection revalidation reread all 116 original artifacts. Replacing only
the exact provider-encoded historical-summary message reduces the same request
to **146,132 bytes**, leaving **45,164 units** of headroom. Summary bytes fell
from 52,546 to 20,323. This is a local replay measurement, with zero provider
calls and no native-history mutation, rather than an end-to-end pass.

`cargo +1.95.0 test --locked --offline -p vcp-context --lib --tests --target
x86_64-pc-windows-msvc --target-dir D:/code/Github/vcp/artifacts/codex-target -j 8`
passed all 14 tests (five compaction and nine assembly/handoff tests). The new
96-pair regression bounds both call-argument and result excerpts, preserves
complete recent pairs, and rejects tampering with fully omitted sources. Local
replay evidence and reproducible Rust helper are retained at
`artifacts/a-b-tests-candidates/context-replay-20261004.json` and the adjacent
`.rs` file; the temporary integration-test copy was removed after execution.

The subsequent 0.2.19 A UI repair exposed a separate harness boundary: native
execution and application checks passed, but the 300-second canonical inspection
timed out and the repair loop treated its failed `inspect-*` gates as application
defects. The loop now stops on incomplete required inspection or unknown cost
before admitting another repair or advancing, including after its last allowed
repair. Original gates and possible-spend holds remain intact for read-only
evidence recovery. `InspectionRepair.Tests.ps1` reproduces the former unnecessary
dispatch and passes 39 checks after the fix; genuine application failures still
receive bounded repairs. Existing campaign (34), blocked-execution (580),
accounting (38) and paid-dispatch accounting suites also passed. The longer
metadata timeout reduces recurrence but does not replace this stopping rule.

A/B's intentional T5 deadline now has a separate, conditional billing-recovery
path for the forthcoming native `tasks reconcile-cost` command. It preserves the
original exit 7 and evidence, requires a new mandatory reconciliation proof and
unchanged JSONL framing, and permits only the existing same-task continuation
after scoped receipt accounting agrees with a fresh canonical inspection showing
zero active/unresolved liability and no unknown tools or active agents. Three
metadata polls are the bound; an unsupported command or unresolved receipt stops
with the full hold. Ordinary repair stopping remains unchanged. The acceptance
contract is explicit in [CLI test plans](cli-test-plans.md); this path does not
claim a completed real T5 run.

`DeadlineReconciliation.Tests.ps1` passed 84 offline checks, including a real
harness continuation/finalization flow with an inert native boundary: original
exit 7/cost null retained, USD 1.50 supplemental settlement plus USD 0.50 resumed
cost, final scenario and campaign accounting exactly USD 2.00. Failures include
unknown receipts, unsupported commands, wrong scopes, malformed flags/frames,
unknown effects and missing pages. Existing blocked-execution (580), accounting
(38), inspection-repair (39), campaign (34), paid-dispatch accounting, scenario
gates and Inventory profile suites also passed after integration. No provider
inference was used for these checks.

The optimized 0.2.21 executable reports `vcp 0.2.21`, SHA256
`acce4e0f104bbbe1add8512f3b7f362c4bce5ae718c04dc1770bb1132a28f90f`.
Its actual `tasks reconcile-cost` command exited 0 on the already-settled,
paused historical A task `33838a34-25b3-4e54-95ef-38b50eab3e91`. The child
environment contained only a synthetic provider credential; all original
attempts were settled, so there were no receipt requests or inference. The
one-frame JSONL result preserved the scope, paused state, ledger revision and
USD 3.091995 settlement, with no observations and `resumed: false`. The original
failed scorecard hash also remained unchanged. This qualified the actual
production emitter, not just a hand-constructed mock.

Replaying those exact result bytes through the deadline helper passed six
additional checks against explicitly synthetic original-stop and fresh-inspection
fixtures. The original exit-7 diagnostic and same-task-only eligibility remained
intact. Native output and replay proof are retained under
`artifacts/a-b-tests-candidates/0.2.21/deadline-shape/`; the reproducible helper is
`artifacts/a-b-tests-candidates/qualify-deadline-native-frame.ps1`.
This is output-shape qualification, not a real T5 continuation or scenario pass.

The 0.2.21 A attempt `20261004-091143-A-5646b84f` remains failed: 82/88 required
checks passed and all USD 5.103498 is settled. T1 and T2 ultimately passed. T3
used all 96 requests, including 80 reads and three identical malformed final
patches, then paused before verification. Its eight API tests passed, but the
unfinished UI edits broke typechecking and compilation. No application files
were repaired outside VCP. Subsequent task prompts ask for small completed edits,
focused rereads after patch errors, and early verification; the next A attempt
uses the already authorized Sonnet model.

The 0.2.21 B attempt exposed two independent false failures. PowerShell returned
`application/problem+json` bodies as byte arrays, which the harness previously
cast to decimal text; UTF-8 decoding now preserves the actual error fields.
Eleven real loopback HTTP checks and the existing scenario-gate suite pass.
Native .NET verification also mistook the ASP.NET warning `Failed to determine
the https port for redirect.` for a failed test despite all twelve named tests
passing. The parser now recognizes failed/skipped test-result syntax, retaining
terminal failure and complete-count checks; all 25 tool tests pass. These fixes
do not alter retained scorecards or the already running B controller. The
separate HTTP 201 responses for invalid product data and zero-quantity movement
were genuine generated-application defects, subsequently addressed by VCP's
first repair task.

That B attempt (`20261004-093021-B-48537c98`) finished failed at 47/52 required
checks with USD 13.175679 fully settled. The first repair reached its task budget
admission limit; the controller correctly blocked a second repair. Compilation,
migrations, seeded data, and movement validation passed, while two generated API
tests failed after validation was enforced. B's T2 prompt now explicitly carries
forward the T1 domain rules and requires successful-request fixtures to satisfy
them. The two remaining HTTP error-field failures used the old loaded decoder.

Local candidate 0.2.22 reports `vcp 0.2.22`, SHA256
`b7826b52540f36cdaaf57542ee07d392e4214650707e087fdb2a3c167b7e220e`.
Its source guard, 25 tool tests, 16 version tests, nine protocol tests, twelve
synchronized source version fields, and 7,940-file Codex provenance check pass.
No installer or VSIX was produced. The actual native command version was checked;
the executable has no PE FileVersion/ProductVersion resource.

Retry provenance is explicit: reused-project mode had disabled Git checkpoints,
so no T2 A or T1 B commit existed. New clean detached clones use A's actual older
T1 commit `2c15168cb1d1c932b55baeae0d0090b6ced2b99b` and B's scaffold commit
`74e053e1d325a063c1b9eba77062aa1c4ad4f965`. Their original projects remain intact.
The 0.2.21 Sonnet dry runs passed 12/12 and 14/14 required checks respectively,
with no inference. Source/tree hashes and clone paths are retained in campaign
`receipts/a-sonnet-checkpoint-provenance.json` and
`receipts/b-sonnet-checkpoint-provenance.json`.

The 0.2.22 B Full attempt `20261004-101636-B-24a8ba4c` finished failed at
49/52 required gates, with USD 9.607512 fully settled. All HTTP contract gates
passed, including the previously misdecoded UTF-8 problem JSON. T2 stopped at
its native budget boundary; five generated test failures came from creating a
new InMemory database name inside the options callback, so requests did not
share the fixture database. Its verified T1 source checkpoint contains 38 files;
manifest SHA256 is `d3577381cd1ab12b55420a205a667d57ce076cfaad6b17de87085353cd4260dd`.
No application source was repaired manually. Neither Full scenario has passed.

`run-ab-campaign.ps1 -Action Repair` now permits one separately reserved native
repair task using `-SourceAttemptId`, `-SourceStage`, and `-RepairPromptPath`.
The source must be an exited, accounted Full attempt in the same project; unknown
effects, unresolved campaign accounting, and quarantined workspaces remain blocked.
`MaxAttemptUsd` must equal `TurnBudgetUsd`. The existing project lock and USD 100
ledger cover the repair, including full retention on missing or uncertain evidence.
Source scorecard, stage profile, and prompt hashes are retained and rechecked.
Original checks, affected paths, tools, effects, and process contracts remain;
only run-owned scratch directories, the explicitly selected qualified provider,
and bounded execution settings change. There is no automatic repair retry.
Successful native completion, current fingerprint-bound verification of every
original check specification, complete canonical accounting, and an immutable
source checkpoint are required. Its `repair-pass` verdict is distinct from Full
acceptance; the original failed evidence and Full baseline gates remain unchanged.

Offline repair qualification passed 40 focused checks, plus Campaign (34),
AccountingRecovery (30), BillingQuarantine (33), BoundQuarantine (61), and the paid
dispatch accounting regressions. Both actual retained A/B source bindings passed
hash validation. B's actual registered dotnet process reported `10.0.204` with
exit 0 under its freshly relocated, filtered scratch environment. This check ran
only `--version`, with no inference or application edits. Independent review
found no remaining blocker; paid repair and subsequent Full results are pending.

The targeted repairs subsequently passed all 14 required gates: B
`20261004-105252-B-b017070e` settled USD 0.523357 and A
`20261004-105253-A-ea7e4fb4` settled USD 1.842072. Their retained workspace diffs
show no protected-file changes; their historical scorecards remain unchanged.
Review then identified that future Repair runs also need independent protected-file
checks. Required gates now compare the fixed A/B protected paths before dispatch
and after completion. Retained Full baseline hashes take precedence; for older
runs, verified source checkpoint copies supply the hashes where available.
Conflicting or corrupt source hashes fail closed. When an old source run retained
neither, the repair explicitly records a current-before-only limitation and still
requires unchanged bytes through repair. A changed/deleted protected file cannot
produce a repair checkpoint. The 20 focused protected-file checks and existing
40 targeted-repair checks passed. Read-only qualification confirmed B's protected
settings against its original Full checkpoint; old A's absent source hash was
reported explicitly. No historical result was retroactively requalified.

The following concurrent 0.2.22 Full runs stopped at T2 despite passing the
independent application checks reached so far. A `20261004-105655-A-0cafc6eb`
received a provider `server_error` without observed cost, so its USD 20 campaign
reservation remains held pending authoritative reconciliation. B
`20261004-105658-B-6a5729a0` settled USD 0.724921 but paused on context capacity:
one verbose test invocation exceeded its output capture limit and occupied roughly
139 KB as a call/result pair. The compactor preserved it in the most recent six
complete pairs; the independent test run subsequently passed all twelve tests. Neither
result is a Full pass. Native fixes are being qualified for the next candidate.

A separate zero-cost probe found an uncovered original T3 requirement: persisted
tasks without `labels` returned no default array, and filtering them returned HTTP
500. The labels gates now seed a legacy task in isolated runtime data and require
`labels: []` in both list and item responses plus successful filtering. T3 also
requires uniquely named Vitest tests for sort selection and label-chip events;
the JUnit gate handles Vitest's nested `describe` prefixes and rejects missing,
duplicate or skipped cases. Fourteen focused checks and the existing scenario
gate suite passed, followed by independent review. These gates strengthen the
original contract; application repair and Full qualification remain outstanding.

Candidate 0.2.23 reports `vcp 0.2.23`, SHA256
`78b498c836b0c4b50b6672de99f41ca952e785cdf151dfc9622c73bb5c9958d9`.
It includes the oversized recent-pair compaction fix and missing-cost terminal
response identity recovery. The exact retained B request projection fell from
232,720 to 98,063 bytes against capacity 191,296, with all fourteen original source
hashes revalidated. Twenty-one focused tests, sixteen version tests, nine protocol
tests, source guards, twelve synchronized version fields and 7,940-file provenance
verification passed. No installer or VSIX was produced.

The actual 0.2.23 metadata-only reconciliation command now discovers A's pending
attempt `e59f7f47-8373-40cb-903d-32d4ca4e2649`; its observation remains `unknown`
because the provider has no usable receipt. Exit 7 correctly retains USD 0.841960
unresolved alongside USD 0.341261 settled in that task, with zero active liability.
The original scorecard and inspection hashes remain unchanged; no inference or
resume occurred. The campaign reservation has not been released on this evidence.

The later quarantine audit permanently isolated that A workspace and retained both
original USD 5 task caps. Fresh canonical inspections, complete launch accounting,
and hash-bound proof of the terminal failed test process established a USD 10
bound; only the USD 10 reserved for unlaunched stages was released. The missing
provider charge remains unknown. Concurrent Full attempts
`20261004-112858-A-219e5038` (USD 14 cap) and
`20261004-112859-B-aaee869f` (USD 16 cap) then started on 0.2.23, bringing total
settled costs, quarantined caps and active reservations to USD 99.972025. A uses
an independent copy of all 25 hash-verified checkpoint files; no original native
history was copied. Both runs passed T1 and T2.

A then stopped at its T3 request bound without application edits: task
`4d363f94-28b6-4aeb-9810-10890aa51a22` settled USD 4.056476; the complete
attempt settled USD 5.080842 and failed 5 of 89 required gates. The independent
checks confirmed missing named UI interaction tests and legacy-label filtering
returning HTTP 500. Complete accounting released the unused reservation; total
campaign liability became USD 91.052867 with B still active.

Retained A requests show a seven-file reread cycle against a six-pair recent
window; the original objective remained intact. Exact zero-cost native replays
qualified a capacity-aware twelve-pair candidate: A used 155,611 bytes, the older
B capture 174,366, and the oversized-output B capture 97,819, all below their
191,296-byte input capacity with source hashes revalidated. This candidate keeps
the configured projection when the larger window does not fit and leaves routed
provider selection unchanged. Local build and live qualification remain pending.

B `20261004-112859-B-aaee869f` also reached its T3 request bound. Its ten-file
increment compiled, and all independent SQL Server/API/Razor runtime gates passed,
but the test suite reported thirteen passes and two failures: the Details fixture
requested product 1 instead of its inserted product, and the Create fixture failed
antiforgery rather than testing validation. T3 settled USD 4.749912; the complete
attempt settled USD 6.239471 and passed 79 of 82 required gates. Both attempts are
fully accounted; total campaign liability is USD 81.292338, including USD 20 of
quarantined caps. Focused repair prompts preserve the original requirements and
protected source hashes; neither attempt is recorded as Full acceptance.

Candidate 0.2.24 is built and reports `vcp 0.2.24`, SHA256
`6c6eacb13d8eb433a408a185f132126e0d5691f172ecd489f03cb83bfa3db613`.
The six focused checks include exact retained replays, real wide/narrow-capacity
HTTP owner flows across both stores, continuity/reopen/unknown-cost preservation,
and hook eligibility. Sixteen version tests, nine protocol tests, twelve
synchronized declarations, provenance and source guards passed. No installer or
VSIX was produced. Targeted A repair `20261004-121226-A-8900110e` and B repair
`20261004-121253-B-c1f5ad67` each reserve USD 5, bringing campaign liability to
USD 91.292338. Both passed preflight and protected-file checks; repair and Full
qualification results remain pending.

A repair `20261004-121226-A-8900110e` subsequently passed all sixteen required
repair gates, settling USD 1.741007. Its native contract covered the nine API
tests; separate UI/typecheck/build commands had invocation errors, so those were
not claimed by the repair. Full A `20261004-122008-A-c7c92a2e` then independently
passed baseline API tests, Vitest, typecheck and build, and its T1 API gates. It
reserves USD 9 with USD 3 task caps and remains active.

B repair `20261004-121253-B-c1f5ad67` settled USD 1.379737 but failed native
completion: its fifteen tests passed, then it finalized after an
`executed:false` instruction refresh without repeating verification. The failed
verdict is preserved. Total liability after both repairs was USD 84.413082;
with the active A Full reservation it is USD 93.413082. B will require Full
qualification, including independent checks of the identified Razor contract gaps.

The 0.2.24 Full A T3 task `37f11700-e274-4537-bfe3-9c2b34174bca` completed
successfully at USD 0.938990 with no source changes. Every independent T3 gate
passed, including the new legacy-label and UI interaction checks. Its actual
admitted requests prove the retention fix engaged: all eleven published
projections retained twelve pairs while the saved configured policy remained six.
Projected inputs ranged from 113,625 to 137,611 bytes against capacity 191,296;
request/manifest hashes matched, no parts were excluded, and all referenced source
hashes and lengths verified. Full A continues at T4; this is not a final pass.

B's independent Razor gates now test valid zero price/reorder values and isolated
invalid SKU, overlong name and unknown supplier submissions, requiring specific
nonempty field errors and no insertion. Low-stock fixtures exercise equality and
above-threshold balances, with exact API/table SKU parity and ordering. Twenty-four
focused checks plus the scenario/profile suites passed; independent review caught
and corrected a nested-HTML field-message parser issue before a paid run. The T3
prompt explicitly states these existing contract requirements.

Full A passed T4, including unchanged protected regressions, at USD 0.932414.
T5 task `281a22c7-f1ed-4e55-937f-a513b48b674b` then reached its real short
deadline while receiving a provider tool call. The partial call was not applied;
the original exit 7 and unknown-cost evidence are retained. Metadata-only
reconciliation subsequently obtained the exact generation receipt for attempt
`843b02d6-6a27-45f1-a00f-5cb83092cc7d`: USD 0.034369. Native exit 0 reported
USD 0.622850 settled, zero unresolved/active liability, state paused and
`resumed:false`. The controller is validating that proof before same-task resume.
No deadline or accounting rule was relaxed.
