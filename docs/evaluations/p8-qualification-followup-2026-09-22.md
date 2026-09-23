# P8 qualification follow-up — 2026-09-22

Status: in progress; no release acceptance or publication is recorded.
The owner requested correction of fresh restore and retained-history latency,
the prepared $16 interactive observation and six $8 owner attempts ($48 maximum),
missing integrated scenarios, and installation/model-provisioning qualification.
This authorizes those bounded executions and prepared fixture thresholds; it
does not supply the owner's eventual quality judgments or release acceptance.

The [previous production report](p8-production-qualification-2026-09-22.md)
and all earlier receipts remain valid within their recorded scope. Relevant
code changes require a new production artifact and affected checks. No full
matrix rerun is planned. Physical full-volume exhaustion remains a documented
gap; machine handoff remains owner-skipped.

## Restore and startup changes

The new production build completed successfully in 10m42s of Cargo compilation
using Rust 1.95.0, the locked/offline release recipe, static CRT and no qualification
features. Before/after source content identity matched:
`d94586e97d3cb33fe7a587247e6073612af5b37b6b6a761b32d28e53668802b0`.

| Identity | Value |
| --- | --- |
| Build receipt | `artifacts/p8-production-build/ea4900bb-6971-4e30-a96a-1c8b8048759c/build-receipt.json` |
| Package receipt | `artifacts/p8-followup-package/688f4d93-6797-4372-8d01-31a0a5dff7dd/result.json` |
| Executable SHA-256 | `38e3924795c690f5448368ae67340810ef61a0c13b160a45420196164a370e62` |
| ZIP SHA-256 | `c59592648257213f50ec71d1dabf20f2d140b427d700af4f0e00f0ada7b22d6e` |

The exact package passed 16 installation/upgrade/rollback/uninstall commands and
32 assertions. Receipt under system TEMP:
`vcp-production-distribution/fcc701dd-3891-4795-acd3-54f3b1750c58/result.json`.
The affected recovery schedule passed 74 steps across both stores, including
authenticated fresh default restore without sync roots, explicit disjoint/unsafe
sync boundaries, production profile guards, fresh encrypted publication with
independent decryption/signature/inventory checks, and prior-binary compatible
reads. Receipt under system TEMP:
`vcp-followup-recovery-43d0bbf1-ecc6-43fa-a76c-050bd7cd0b16/result.json`.
These are current-workstation checks; neither receipt proves a clean OS or machine
handoff.

Fresh restore previously removed the implicit data-root exclusion before
opening private trust storage. With no existing destination or configured sync
roots, this left an empty exclusion list and the path guard refused to open.
Restore now excludes the mandatory existing plaintext staging directory from
trust/canonical storage. It also retains an explicitly declared sync root equal
to the data directory, which must be rejected. The path guard itself is unchanged.
The affected production recovery schedule now exercises default restore without
sync roots, a disjoint explicit sync root, and overlapping-root rejection on both
backends.

Replay still validates every historical transaction. Record/state capacity
checks count serialized bytes without allocating and sorting another JSON tree;
typed record decoding borrows the retained JSON tree. Digests and persisted bytes
retain canonical serialization. History queries, retention notices and notice
acknowledgements share one canonical store owner instead of reopening and
replaying history for each operation. Replay remains superlinear; measurements
establish the practical improvement and cannot imply a new scaling contract.

Three fresh-process samples per operation on each retained cohort passed all 24
checks. Original fixture inventories and canonical counts remained unchanged.
The timing run followed native builds and other checks, without competing builds.

| Backend / commits | Previous inspect median | New inspect median | Previous history median | New history median |
| --- | ---: | ---: | ---: | ---: |
| SQLite / 512 | 15.197 s | 5.423 s | 30.341 s | 5.260 s |
| Files / 512 | 15.249 s | 5.115 s | 28.429 s | 5.016 s |
| SQLite / 785 | 36.176 s | 12.714 s | 70.563 s | 12.781 s |
| Files / 785 | 36.044 s | 13.057 s | 70.112 s | 13.289 s |

At 785 commits this is approximately 2.8 times faster for inspection and
5.3–5.5 times faster for history. Inspection ranges were 12.470–13.068 s
(SQLite) and 12.948–13.095 s (Files); history ranges were 12.693–12.909 s
and 12.455–13.432 s respectively. Peak working set across each 785-commit
cohort was 82.4 MiB / 78.5 MiB. Receipt and summary:
`artifacts/p8-followup-startup/c5d7b485-95e0-4ea8-aa39-033fe7a48791/`.
These small samples do not establish a p95, sub-two-second startup, or minimum
hardware claim. Full validated replay still takes seconds.

Targeted native restore regression: one test passed with eight CLI invocations
across SQLite/Files. The full `vcp-store` suite passed 76 tests; its existing
real-OneDrive opt-in test remained ignored. This includes persisted JSON/replay
coverage, exact record-size limits, and byte-count equivalence for arbitrary
precision numbers, reserved JSON keys, escapes and Unicode. Store test log:
`artifacts/p8-startup-store-tests.log`, SHA-256
`ba42fa7081eb6dec8e2e85b7b5576b3ae31e56c43672b0d1d6987d8f0a2e1c79`.
Two initial restore test compilations using mismatched cache settings were
interrupted before running tests; the corrected native workspace build passed.
The new executable history regression also passed on both stores in 2.51 seconds:
preview/page reads retain the expected watermark, a due notice is acknowledged
once, and the next listing does not write again. No provider requests occurred.
Receipt: `artifacts/p8-startup-history-execution.log`, SHA-256
`161c864bd9e42cfd4a09981ab47a0e3b69b739232c0c4cac7cbd8277d3facc63`.
Its initial Cargo exact filter matched zero tests; the fully qualified test was
then executed directly from the newly built binary and passed.

## Owner execution controls

The fixture manifest remains
`6a27284e55f440ffbc8580562b415f8cab1157e54b569dde88fbe07ec5c8969b`.
Its self-check reproduces both seeded review defects and passes 37/37 reference
assertions versus 9/37 on the incomplete generation baseline. These are fixture
checks, not live owner scores.

The preparation capsule lacked a paid execution supervisor. The new tracked
`p805-owner-prepare.cjs`/`p805-owner-runner.cjs` bind the exact artifact, fixtures,
profile, catalog and constrained launcher. They enforce one-shot launches,
reserve before dispatch, retain unknown liabilities, and stop on supervision or
accounting uncertainty. U03 requires current-parent canonical verification,
matching source bytes, the hidden feature oracle and preserved user/Git edits.
Human findings/usefulness/architecture judgments remain separate. Independent
review found no blocking issue. Frozen inputs are checked before reservation,
before each row and after every attempted slot; nine owner controls pass.
The delivery run passed all 17 groups:
`artifacts/p8-followup-delivery-fast/9c303171-ea24-45b9-b863-64411a27e181/manifest.json`.
After the separately reviewed retention extension, the affected P8 group passed
36/36 tests, zero skipped:
`artifacts/p8-followup-retention-tests/b05c281e-4ee6-44e4-9270-4170d3aba30c/manifest.json`.
The independent Python oracle controls passed 19 tests. Imported Codex and
Munarium Git-mode/byte checks also passed. These are delivery/harness evidence;
they do not replace the package observations or human acceptance.

A credential-free end-to-end control against the previous package prepared and
validated all six roots, then rejected solely for the missing provider key,
without accepted canonical work or any reservation. It is retained under system
TEMP in `vcp-p805-runner-control-d6c58fdf-eacc-4949-84fe-184cb6ccb67c`.
Paid execution must use a new binding to the rebuilt artifact, not this control.

The authorized six-task campaign uses system TEMP root
`vcp-followup-owner-final-0c03011e-97cc-45ec-a705-8e79c8af842f`, plan SHA-256
`1ffdfd4678c49bf98ec83807313a6a7a6c6c67e01ad6716c28c9dd8294dfc88f`.
No fixture or threshold changes are permitted after dispatch. Each failed
attempt remains in the denominator; there are no automatic paid retries.

All six attempts ran, with no campaign stop and no unknown owner-task charges.
Total recorded cost was **$1.197932**, below the authorized $48 maximum.
Three attempts passed automated controls and three failed completion; this is
not an accepted six-task cohort. Human rubric judgments remain pending.

| Slot | Automated outcome | Task execution time | Recorded cost |
| --- | --- | ---: | ---: |
| U01 / SQLite | Controls passed; human review pending | 174.500 s | $0.218564 |
| U01 / Files | Failed completion | 93.527 s | $0.163826 |
| U02 / SQLite | Controls passed; human review pending | 127.506 s | $0.111754 |
| U02 / Files | Controls passed; human review pending | 175.136 s | $0.182040 |
| U03 / SQLite | Failed completion | 318.953 s | $0.252778 |
| U03 / Files | Failed completion | 257.929 s | $0.268970 |

All three failed rows exited 8, durably paused at exactly 16 settled provider
attempts with `canonical root coding limit requires attention`. There was no
timeout, dollar-budget exhaustion, unresolved effect or reported internal
failure. U01/Files made 17 successful list/read/search calls without reaching a
natural-language completion. U03/SQLite patched the permitted files and recorded
passing canonical `package.json#test` verification, but still reached the request
cap. U03/Files made four rejected execution attempts and recorded no canonical
verification. A partial patch or passing check does not satisfy completed-task
acceptance. `owner-result.json` and each slot's captured stdout,
stderr, costs, context, routing, verification and preservation evidence are
retained under the exact root above. No failed attempt was replaced.

Both U03 runs encountered a setup mismatch: visible instructions and package
metadata prescribe `node --test test/page.test.cjs`, while the restricted
launcher accepts only `--test --test-reporter=tap --test-concurrency=1
test/page.test.cjs`. Rejection of the instructed argument form adds interface
friction; it is not a failing feature-test result. The v3 fixture, launcher,
16-request threshold and original outcomes remain frozen. A corrected launcher
must have its own identity and fresh execution binding before any future run;
the completed cohort cannot be retroactively repaired or relabeled as passing.

The separately identified [launcher correction proposal](p8-owner-launcher-v2-proposal-2026-09-22.md)
accepts both declared argument forms and normalizes them to the same pinned,
permission-fenced Node execution. Two Rust parser tests and 23 native subprocess
controls passed, including hostile argument rejection and environment/filesystem/
network/process permission checks. Frozen v3 fixtures and the bound owner
preparer are unchanged. This future-only proposal neither authorizes provider
calls nor supplies a runnable replacement campaign.

A supplemental read-only check ran the unchanged hidden feature oracle against
both failed U03 workspaces: **37/37 assertions passed on each**. Before/after
workspace inventories, including Git data, and original plan/result hashes
matched. No model attempts were repeated. This establishes feature behavior in
the retained patches, but neither task completed; U03/Files also lacks canonical
parent verification. Both original task outcomes therefore remain failed.
Receipt under system TEMP:
`vcp-u03-supplemental-oracle-66f239f7-e7b8-433f-bb5e-8907c4725f54/result.json`,
SHA-256 `1d8eca5b67704877b87f9c0b069f88591993bfb7072761ef21d5ac7d8652dd52`.

Read-only advisory review found the seeded boundary violation and correct
maintained/generated distinction in U01/SQLite, and both seeded defects with
valid reproductions and preserved benign/user edits in each completed U02
answer. U02/SQLite also adds an unsupported fixture-external aside about exotic
string conversions; the owner must assess the zero-unsupported-claims gate.
These observations are advisory, not human scores or acceptance. Hash-bound
review: `artifacts/p8-followup/owner-advisory-review.json`.

## Interactive observation

The first authorized trial failed before pause acknowledgement after one
canonical submitted attempt. The native PTY driver expects one JSON object per
line; the harness mistakenly sent pretty-printed multiline controls. Its first
pause control caused the driver to exit, ending the CLI before durable pause.
Receipt under system TEMP:
`vcp-followup-interactive-d729b288-eeaf-4b7d-b09c-8cf4efd8babc/result.json`.
Plan SHA-256:
`490de300a8435ee0a87b90caeb9946570ee0bbd81f07fddf7f1298ac393a2e57`.
Task: `d96a05c0-f6f7-41c6-9e9b-a354215fe229`.

The charge remains unknown; the campaign conservatively retains the entire
$16 reservation. This is a failed observation, not a product pause/resume pass.
The shared compact JSONL encoder now covers normal controls and both output-limit
termination paths. Offline regression tests use the actual native PTY driver and
a harmless Node terminal with the provider credential removed. Pause/resume/exit,
termination and fragmented framing pass. The registered P8 group passed 27 tests,
zero skipped; log `artifacts/p8-interactive-framing-group-tests.log`, SHA-256
`b84dbed29bef7fad3b628d3899f161b5addcd4346a729067314d5c558770090e`.
These transport controls do not establish production interactive qualification.
A separate additional $16 trial was requested; no paid retry is implied by the
fix or by elapsed time awaiting a response.

An additional closed-pipe review found that asynchronous stdin errors could
bypass the original harness's final receipt handling. The shared writer now
handles callback/stream failures once and suppresses repeated termination
writes; a real pending-write/child-exit regression and the actual PTY controls
pass (five targeted tests, zero skipped). No production executable changed.
An initial closed-pipe fixture failed to induce the OS condition; that failed
test was retained, and the corrected fixture exercises the actual pipe closure.
The fresh unexecuted retry plan is under system TEMP in
`vcp-followup-interactive-fixed-6e04de90-740d-47fc-a998-86f33bf3d477`, SHA-256
`d0133deb79cf6d4d81088380d5fa2d2fba4aedb5a1c0f67dcb9aca4761e66053`.
Preparation made zero model calls and no reservation; validation must recheck
the qualified profile's expiry before any subsequently authorized execution.

## Post-owner history and retention

The zero-provider integration runner independently decoded each accepted owner
root, including failed tasks, and compared CLI pagination against all **2,803
task event IDs**. It recovered **101 complete response/stdout/stderr artifacts**
through bounded CLI ranges and matched their canonical lengths and hashes.
Notification-only defaults and those reads preserved canonical state. Every
original data/workspace inventory remained byte-identical. Optimizer and skill
commands supplied inspection/setup evidence only, not live routing or activation.

The initial report under system TEMP is
`vcp-followup-integrated-ee688898-0291-4f0c-aab1-46893781a127/result.json`, SHA-256
`1379b43143c1e85f5b9a5df9b5eae225a7a2aff53fc48a3d915be30a6e91491d`.
Its status is partial because the original oracle conservatively rejected all
Claim-stored documents, including internal ingestion bookkeeping. No failed
read-only observation was counted as a pass.

The reviewed oracle extension permits only recognized sole-task ingestion jobs
and cursors without governed results. Unknown claims, memory/advisory lineage,
other-task scope and unsupported dependencies remain rejected. It derives exact
selected/protected IDs from independent canonical state before product previews.
A retention-only follow-up revalidates the original inventories, canonical cut,
history/default-policy command receipts and every complete raw-output byte hash,
then binds those prior observations throughout the new run. This avoids repeating
successful history/output checks. Three parallel batches use separate disposable
roots, make no provider calls and preserve the original owner evidence. Launch
receipt: `artifacts/p8-followup/owner-retention-launch-ee688898-0291-4f0c-aab1-46893781a127.json`.

## Fresh model provisioning

The packaged provisioner acquired ten pinned MiniLM
files into a new private directory and a separate Verify invocation passed.
Neither an existing model directory nor the developer model cache supplied the
assets. Package provisioner SHA-256:
`6c24cec9e8a8e21de485ad85b03eda4db1efb8cd2885402c486965c5d56721f1`;
specification SHA-256:
`ba5fd8384a05e519fb44b9aa233e94cf82996dbbc15873bdf690cb68465f5c11`.
Both files are byte-identical in the previous and rebuilt production packages;
their hashes were checked against the rebuilt package after acquisition.
Acquisition/verification receipts are under system TEMP in
`vcp-fresh-model-f258828a-15ea-4de0-90ca-fd8441705488`.

The retained native embedding qualification executable
`53f270939bcecdf52a471e6f1bf703055011c1af71c8e5d3b9f1d3a5e5331990`
then passed real CPU reference, batch padding, truncation and model-reopen checks
using those new assets. Its maximum reference/reopen vector difference was
`1.7695128917694092e-7` against the `1e-5` gate. The existing Windows AppContainer
supervisor also passed network controls before/after, blocked-network inference,
and missing/corrupt asset controls. Receipt:
`artifacts/p8-fresh-model-offline/0e1338dc-4593-446d-8734-9641344cd619/manifest.json`.
This is fresh acquisition plus native subsystem inference on the current host;
it does not establish a production CLI memory workflow or minimum hardware.

A separate exact-package production boundary probe verified the package and all
ten newly acquired assets again. Four fresh-root help/refusal controls passed,
with unchanged workspace and zero canonical files. Production `memory` exposes
retained search/inspection/pruning; its search paths disable model loading and
provide no query embedding. The CLI profile has no asset-selection control and
no supported command reaches the internal `build_memory_vectors` and
`embed_memory_query` APIs. Consequently these assets cannot establish production
CLI U09 through the available commands. Adding a trusted asset/build/query
integration is a prerequisite, followed by both-store offline build/reopen,
scope isolation and ANN-versus-exact observations. Those scenarios remain not
run. The concrete scenario and source boundary are retained under system TEMP:
`vcp-u09-production-boundary-978e7b7c-1669-40be-abf4-edc9e230e1b3/result.json`,
SHA-256 `50e5f13c42a8d1ffcb17098165fc903615086ab177045163f1f59e744883367c`.

## Environment and owner gates

Clean Windows installation remains unproven. Hyper-V management is installed,
but VM inventory returned access denied both inside and outside the agent
sandbox. Windows Sandbox was not available. The owner was asked for an existing
clean Windows environment and an authorized execution path; no machine handoff
was attempted.

The prepared owner cohort covers direct U01–U03 on each backend. Populated
cross-workspace memory, visible child review, integrated routing/optimization,
MCP/skills and remaining U05–U09 scenarios require their own exact evidence.
Human usefulness/architecture-fit rubrics and final acceptance remain pending.

## Current owner review record

| Field | Recorded state |
| --- | --- |
| Candidate | Executable `38e3924795c690f5448368ae67340810ef61a0c13b160a45420196164a370e62`; ZIP `c59592648257213f50ec71d1dabf20f2d140b427d700af4f0e00f0ada7b22d6e` |
| Fixture/threshold execution authority | Owner's follow-up request; frozen v3 six-task schedule, $8/task and $48 aggregate, 16 requests/task, no retries |
| Executed cohort | Six attempts, three automated completions, three request-limit pauses; $1.197932 settled |
| Human findings/usefulness/architecture-fit scores | Pending; advisory reading is not an owner judgment |
| Actual owner acceptance decision | Not supplied; release remains unapproved |
| Material open gates | Failed owner cohort, interactive paid observation, incomplete integrated U01–U09 coverage, clean Windows environment, minimum-hardware evidence |
| Explicit retained gap / skip | Physical full-volume exhaustion open; machine handoff owner-skipped |
| Publication | Not authorized or performed |

The original acceptance packet remains historical. This record identifies the
rebuilt candidate actually exercised in this continuation; neither a merge of
the fixes nor passing delivery tests constitutes release or owner acceptance.
