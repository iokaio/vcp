# Internal beta implementation status

Started September 30, 2026 from source `b0d84a2df6e5fa7ed3163a3b1920fcf620623213`.
Contract: [release plan](00-release-plan.md). Decisions: [ADR-072](../adr/072-internal-windows-beta.md).
The original request covers implementation through internal-beta readiness and
manual-test preparation. Subsequent limited publication authority is recorded
below; paid execution remains a separate gate.
The [FR/I/U acceptance map](03-acceptance-map.md) joins current changes and
historical dispositions without granting final-artifact or owner acceptance.

## Current work — signed candidate and qualification

BETA-07: the owner requested the next extension identity `iokaio.vcp`, replacing
`iokaio.vcp-local`. The package name, archive names, installed-editor checks and
current setup guidance now follow that identity. Historical candidate filenames,
hashes and public listing records below retain their original identity.
The next qualified VSIX must be uploaded as a **new extension** under publisher
`iokaio`, not as an update to the existing `vcp-local` entry. Existing users must
disconnect and uninstall the old extension, install the new one, and reconnect;
retain native data and User settings. Extension-local state does not migrate.
No new Marketplace upload or removal of the old listing is recorded here.

On October 1 the owner authorized the remaining signing and qualification plan
under [ADR-076](../adr/076-signed-beta-qualification.md), selected Qwen 3.8 with
a new **$40 total** provider budget, and confirmed a clean Windows machine is
available for guided testing. This supersedes the earlier instruction to stop
after the limited handoff; historical failed, deferred and unrun evidence below
retains its original disposition.

BETA-04/06/08 signing integration is **in progress** for native `0.2.0-beta.2`
and SDK/VSIX `0.2.2`, using the existing Azure `ioka-llc-signing` account and
`WritingForgePro` Public Trust profile. The channel requires signatures from
Ioka LLC on the engine, launcher, installer and uninstaller, with immutable
unsigned-build provenance and new final-byte hashes. No signed production
candidate, new live task, clean-host pass or final owner acceptance is claimed
by the preparation below. The public unsigned beta.1 remains
available under its original release record.

The dedicated secretless GitHub identity is configured for the main-only
`native-signing` environment, with signing permission scoped to the existing
certificate profile. Pinned tool acquisition and an actual Azure signing smoke
on a disposable copy of the prior launcher passed: publisher, durable identity
EKU, timestamp, Windows trust and preservation of the original executable image
were verified. Original compiler bytes were unchanged. Mutating an adjacent
signing-tool dependency was refused before signing.

A pinned Inno 6.7.3 compilation fixture also passed the real setup/uninstaller
signing callbacks, including final setup hash equality and both Windows trust
checks. It was not installed and is not a qualified production artifact. Private
evidence: `artifacts/signing-launcher-smoke/evidence/row.json` and
`artifacts/beta-delivery/signing-inno-result.json`. The candidate workflow must
still prove its OIDC login and final production packaging.

BETA-08's [candidate run 36923513140](https://github.com/iokaio/vcp/actions/runs/36923513140)
failed before building or signing: a PowerShell argument-rejection subprocess
reached its 60-second deadline without diagnostics. The unchanged-source
[portable retry 36926183367](https://github.com/iokaio/vcp/actions/runs/36926183367)
passed that group, then hit the distribution and orchestration groups' separate
60-second deadlines. Its missing-Node test also failed near its 15-second child
deadline, but the terminated reporter omitted the assertion detail. These remain
failed observations; the exact hosted slowdown is not established. Original
packets are retained under `artifacts/beta-delivery/signed-candidate-failure-36923513140/`
and `artifacts/beta-delivery/portable-retry-failure-36926183367/`.

The focused scheduling repair caps distribution at four test-file workers and
keeps its 60-second envelope. Both groups use TAP to retain failure diagnostics
without depending on an end-of-run summary. Orchestration's whole-case envelope
is 120 seconds: ordinary local/hosted runs already took about 52 seconds and its
supervision checks intentionally include two ten-second drain failures. Every
inner deadline and assertion remains unchanged. A local distribution comparison
passed in 22.1 seconds with default workers and 27.2 seconds with four; the cap
bounds concurrent filesystem and PowerShell work, not a demonstrated speedup.
Comparison evidence is `artifacts/beta-delivery/portable-concurrency-diagnostic-1790889783028/`.
With the changed registry and physical Node 24.10.0, registered distribution
checks passed (109 passed, one existing editor-input skip) in 29.0 seconds;
orchestration passed all 22 tests in 51.5 seconds. The existing harness and
repository validation group also passed. Their manifests are retained under
`artifacts/beta-delivery/portable-scheduling-verification-5d82a99b/`.
These local checks do not convert either failed hosted run into acceptance.

After a verified signed pair exists, complete the full candidate pipeline and
applicable independent installed checks, then guide the owner through the clean
host and human observations. Keep all new provider probes and task variants
within one reserved-and-reconciled $40 ledger; do not reuse old campaign grants.
The [acceptance map](03-acceptance-map.md) remains open until actual evidence and
owner judgments close its applicable rows.

## Current milestone — October 1 scope revision

The owner approved a smaller milestone under
[ADR-073](../adr/073-manual-testing-candidate.md): one verified artifact pair,
focused installed checks and an owner manual-testing handoff. Full BETA-09 and
BETA-11 remain incomplete. The [revised plan](00-release-plan.md) and
[manual checklist](04-manual-testing-checklist.md) define the stopping rule.

The unfinished 27-file qualification increment is deferred, not delivered. Its
source is preserved in local Git stash
`3649a6182231916c4448345b380785ba9478ca92` and a hash-verified copy under
`artifacts/beta-delivery/deferred-qualification-96cae379c90546bea362adfa7cef221c/`.
It includes the native peer, broader capture/readiness changes and direct editor
qualification execution. Its local contract passes and nine-target compile do
not erase the new process test's 7-pass/1-fail outcome; later native groups were
not run. The stopped startup-only compile and prior failed candidates retain
their outcomes. No product defect has been established by that new file-release
assertion. None of this deferred patch is included in the manual-candidate branch.

The limited milestone is complete: [PR #324](https://github.com/iokaio/vcp/pull/324)
delivered the scope revision; its reviewed main commit `f81b2c50` passed ordinary
Delivery checks and [candidate attempt 2](https://github.com/iokaio/vcp/actions/runs/36868151228/attempts/2)
passed through `pair` on `vcpwin` with 16 build jobs. All 88 retained checksum
entries and the independent pair/payload checks passed. Focused developer-host
installation, offline onboarding, installed observer/setup-guide checks on both
stores, uninstall and retained-file checks passed with no model calls or forced
process cleanup. The [handoff record](05-manual-candidate.md) identifies exact
artifacts, local evidence, corrected invocation/provisioning failures and unrun
rows. Full BETA-09/BETA-11 remain open. That handoff stopped qualification
implementation; it did not authorize publication.

## Subsequent publication authorization

The owner explicitly selected GitHub Releases and Pages under
[ADR-074](../adr/074-github-beta-downloads.md), abandoning the Azure proposal.
[PR #326](https://github.com/iokaio/vcp/pull/326) delivered `publish-beta.yml`,
strict candidate preparation, draft/hash-verified prerelease publication and the
Ioka-styled Pages index. Seventeen focused tests, real 88-file packet preparation,
page layout/accessibility, repository checks and ordinary CI passed.
[Run 36888330245](https://github.com/iokaio/vcp/actions/runs/36888330245) published
the unchanged October 1 pair as
[`v0.2.0-beta.1-1dba45922e0c`](https://github.com/iokaio/vcp/releases/tag/v0.2.0-beta.1-1dba45922e0c)
and deployed Pages successfully. GitHub confirms all five public asset digests.
The owner-created CNAME resolves correctly and GitHub validates the domain;
custom-domain certificate provisioning and HTTPS enforcement completed on October 1.
Normal TLS validation returned HTTPS 200 and HTTP redirected with 301 to HTTPS.
The [publication guide](../development/beta-publication.md) records actual results.
Pages deployment is restricted to `main`.
No Azure resources were changed. This bounded BETA-08/BETA-11 follow-up does not
complete full qualification, authorize paid calls or reopen the deferred patch.

## Marketplace preparation

The owner created publisher `iokaio` and requested the BETA-07/BETA-08/BETA-11
follow-up under [ADR-075](../adr/075-marketplace-manual-upload.md), retaining the
first upload as a manual action. The new package uses `iokaio.vcp-local`, with
explicit migration from the old extension ID, Marketplace listing metadata and
the existing pre-release/platform/provenance contracts. [PR #328](https://github.com/iokaio/vcp/pull/328)
and exact-main Delivery checks passed. [Candidate 36894802050](https://github.com/iokaio/vcp/actions/runs/36894802050)
passed through `pair`; all 88 packet checksums, native and VSIX inventories verified.
Focused native installation, five offline onboarding cases and actual installed
VSIX observations on fresh Files/SQLite fixtures passed with no provider calls or
forced cleanup. The [Marketplace handoff](../development/marketplace-publication.md)
identifies the exact upload file, hashes, private evidence and unrun qualification.
The owner completed the manual upload and the
[public Marketplace listing](https://marketplace.visualstudio.com/items?itemName=iokaio.vcp-local)
is live. Post-upload installation results are recorded in the handoff; Marketplace
availability does not complete full qualification.

| Item | Status | Implementation and evidence |
| --- | --- | --- |
| BETA-01 | Complete | [PR #290](https://github.com/iokaio/vcp/pull/290) merged. Channel, versions, pinned installer, unsigned disposition, support exclusions and external gates recorded. Repository/harness, skill helper and Rust delivery checks passed. No release qualification claimed. |
| BETA-02 | Runtime fix delivered | [PR #293](https://github.com/iokaio/vcp/pull/293) merged with routine checks passing. Shared effective-profile loading binds base and import revision/content at launch, start and resume. Both stores passed 24 stale-selection and 12 actual MCP allowlist/deadline CLI/public-client scenarios, five import tests and existing coding parity. Synthetic loopback providers; final installed cross-client rows remain separate. |
| BETA-03 | Production workflow delivered | [PR #294](https://github.com/iokaio/vcp/pull/294) merged with routine checks passing. Explicit bounded provider setup/receipt renewal, create-only private workspace profiles and offline preflight. Six production provider and two onboarding tests passed, including credential reflection, retained liability, expiry and catalog matching. No paid calls; final live/clean-installed walkthrough remains not run. |
| BETA-04 | Native increment delivered | [PR #291](https://github.com/iokaio/vcp/pull/291) merged with routine checks passing. Strict build/package gates bind clean reviewed source, production target/features/version, tools and assets; pairing checks final bytes. Version-derived VSIX dependencies and native-generated schema source binding were corrected. Production build and strict VSIX integration remain candidate gates. |
| BETA-05 | Inventory increment delivered | [PR #292](https://github.com/iokaio/vcp/pull/292) merged with routine checks passing. Locked Windows normal/build graph: 997 packages, 509 retained license texts and 11 original source archives. Registry archives/extracted bytes and pinned Git caches verified. Seventeen focused tests pass; compiler-observed components and final archive/helper checks still require production artifacts. License provenance limitations remain explicit. |
| BETA-06 | Implementation delivered | [PR #295](https://github.com/iokaio/vcp/pull/295) merged with routine checks passing. Registered per-user setup, hash-bound stable launcher and installed data-root selection. Native script tests passed concurrency, abandoned-owner recovery, retained Files/SQLite format checks, WAL refusal and preservation; compiled Inno fixture passed nine lifecycle cases. Launcher integration and installation metadata tests each passed 3/3. Final shipping lifecycle, console cancellation and a distinct supported upgrade/rollback pair remain candidate gates. |
| BETA-07 | Implementation delivered | [PR #296](https://github.com/iokaio/vcp/pull/296) merged with routine checks passing. Strict beta VSIX binds original native build evidence and freshly compiled locked SDK/editor tools; setup guidance uses explicit User settings and the resolved installed engine. SDK 35/35 and editor 163/163 passed, followed by eight focused release/package regressions, candidate-output preservation and stale SDK/extension output checks. Review fixes reject ancestor compiler fallback, redirected compiler output and missing original native evidence. Final installed candidate and clean-host rows remain not run. |
| BETA-10 | Documentation and staging delivered | [PR #297](https://github.com/iokaio/vcp/pull/297) merged with routine checks passing. Current README, installation/onboarding, recovery, known issues and safe support instructions. Native ZIP stages four source-bound guides and identifies the entry point. Eleven inventory/provenance tests, actual Windows ZIP/document hashes, repository links and PowerShell example parsing passed. Debug assembly proves staging only; final clean-installed walkthrough remains a BETA-09 gate. |
| BETA-08 | Production pair built; installed qualification incomplete | [PR #298](https://github.com/iokaio/vcp/pull/298) merged with routine checks passing. Exact-main dispatch gate, pinned tools, fresh production engine/setup/VSIX construction, separate native qualification target and actual installed synthetic observer smoke. Seven evidence regressions passed, the installed-editor target compiled, and a clean-fixture directory race was corrected and rechecked. Evidence validation preserves failed installation roots and refuses incomplete or changed build records. [PR #311](https://github.com/iokaio/vcp/pull/311) added bounded stages and separate checkpoint/full-pipeline status; the selected portable checkpoint passed all 24 groups. The larger-runner production pair, installed-native harness failure and focused repairs are recorded below; no completed artifact-pair qualification is claimed. |
| BETA-09 | Runner prerequisites verified; final matrix not run | [PR #299](https://github.com/iokaio/vcp/pull/299) merged with routine checks passing. Strict package/installed-payload validation, distinct-version guards, final-byte 130-version startup/cancellation runner and private recovery-fixture preparation. Five package/startup/root-boundary tests passed; startup and editor targets compiled. Existing synthetic recovery generator passed both stores with independent keys, tasks/claims and retained accounting. Historical P8 private inputs and an eligible distinct prior production candidate were unavailable; their evidence is not claimed. Final candidate execution, clean host, real upgrade, independent-machine/full-volume and live/owner rows remain open. |
| BETA-11 | Not ready | Owner acceptance and distribution authorization depend on a qualified candidate. |

## Final installed lifecycle runner increment

BETA-06/08/09 now include final-artifact console cancellation and installed editor
lifecycle runners. Console coverage is eight paused-task chooser cases across
both stores, both console signals and direct/launcher execution. Editor coverage
adds actual reload, restart, incompatible/missing engine refusal and rejection
of a truncated final VSIX, with all canonical state and installed files checked.
Independent review corrected initial VSIX inventory binding and bounded
descendant supervision before acceptance. Twelve focused editor/evidence tests
passed; isolated native console signal mechanics and bounded editor process
supervision each passed. Rust 1.95 compiled both targets. The supervision test
observed natural child completion and rejected lingering children and excessive
output; its log is `artifacts/beta-delivery/editor-lifecycle-supervision-final.log`.
The startup benchmark uses the same hidden process helper and compiled after
the correction; four package/startup contract tests passed. Two disk-cleanup
tests passed, including nine refusal cases. The builder records actual volume
capacity and removes only its verified production Cargo target after pairing,
preserving copied artifacts and build evidence before the qualification build.
Final installed bytes have not yet been exercised, and none of these source
checks fills a matrix row.

## Candidate attempt and tool discovery

[PR #301](https://github.com/iokaio/vcp/pull/301) merged with all delivery checks
passing. The first [candidate attempt](https://github.com/iokaio/vcp/actions/runs/36784737646)
selected reviewed main `8f220fb10ba01cc57fab601153a80433bf2f6248` and failed before
compilation: PowerShell discovered both hosted Node installations and passed
their combined paths as one executable. Candidate runners now select the first
application on PATH, and initial discovery occurs inside the recorded source
stage. A Windows regression exercises all affected lookups with two applications
of each name. The downloaded failure packet and its checksums are retained under
`artifacts/beta-delivery/candidate-failure-36784737646`.

Independent preparation of the hash-pinned VS Code archive also found its actual
commit-prefixed runtime directory. Editor tool resolution must bind that layout
to the supported version and full commit before constructing CLI paths. Neither
discovery correction changes the selected tool versions or waives a gate.
Both discovery tests, all three layout tests (including the actual pinned
archive and nine refusal cases), and eight evidence regressions passed.
PowerShell parsing, JavaScript syntax and independent re-review passed.

[PR #302](https://github.com/iokaio/vcp/pull/302) delivered those corrections with
all required checks passing. The [candidate retry](https://github.com/iokaio/vcp/actions/runs/36786623642)
selects reviewed main `5138f9360359285dee7b534165a7b5d15bcf7ab9`.

That retry passed source validation, pinned editor verification, SDK/editor tests
and most fast cases, but `cs3-webapp` and `p8-distribution` returned failure before
production compilation. The retained packet contained the harness summary but
omitted its separate child logs. Candidate execution now writes those logs below
its own output root; the collector retains only validated manifests and exact
attempt stdout/stderr names, with hashes and sanitization, excluding private
fixture trees. Ten evidence regressions passed, including a real failing harness,
interrupted capture, changed/missing logs and private-file exclusion. Independent
review found no remaining issue. Both failing groups passed locally through the
same sanitized harness and physical Node 24.10.0; no speculative test fix or gate
waiver was applied. All five retained retry files passed checksum verification
under `artifacts/beta-delivery/candidate-failure-36786623642`.

[PR #305](https://github.com/iokaio/vcp/pull/305) delivered that evidence fix with
all required checks passing. The next [candidate attempt](https://github.com/iokaio/vcp/actions/runs/36790344329)
selected `937cb80a79b7ad90ae3abc3174f01b2084193acb` and again stopped in the same
two contract groups before compilation. This time all 52 retained files passed
checksum verification, and the child diagnostics identify copied temporary
fixtures failing the existing redirected/noncanonical-path guards. The hosted
temporary-directory spelling was not recorded, so an exact inherited alias is
not claimed. Local Windows observation confirms that DOS short paths can differ
from both .NET's full path and Node's physical path. Candidate preparation must
select an ordinary canonical temporary directory before constructing test or
private qualification roots, while preserving the guards themselves.
That selection now happens inside the recorded source gate. Three tool-selection
tests passed, including real child/private-root selection through an owned
junction and refusal of missing/file temporary roots. Both actual sanitized
harness groups failed under the reproduced junction and passed after selection:
WEB 36/36 and distribution 51 passed, with its existing prepared-editor-archive
case conditionally skipped because the general harness excludes that input.
The candidate runs that pinned-archive case separately. Syntax, diff and
independent review passed. This reproduction does not assert the exact unrecorded
host alias. Its receipt is `artifacts/beta-delivery/candidate-temp-repro.json`.

[PR #307](https://github.com/iokaio/vcp/pull/307) delivered that correction with
all required checks passing. The next
[candidate attempt](https://github.com/iokaio/vcp/actions/runs/36792491264)
selected `6d78939578ae94a5a800f90087eba5d48724e737`. Starting this run does not
establish a production build, passing pair or completed matrix row.

That attempt passed source/provisioning, the actual pinned editor archive,
SDK/editor tests and the Windows fast suite. It recorded the inherited
`C:\Users\RUNNER~1\AppData\Local\Temp` spelling and selected the ordinary
`C:\Users\runneradmin\AppData\Local\Temp` directory. The optimized Rust 1.95
production build passed in about 64 minutes with qualification features absent.
Native packaging then refused the compiler-observed Git dependency
`tokio-tungstenite`: Cargo emitted a version-only fragment (`#0.28.0`) while
the notice matcher expected a named fragment. Cargo documents
[both package-ID forms](https://doc.rust-lang.org/cargo/reference/pkgid-spec.html).
The matcher now accepts the version-only form only for the exact Git URL's
package name, retaining source/query, pinned revision, package/version and unique
graph membership checks. Registry sources and local workspace IDs also require
exact matches. Eight focused tests passed, along with the adjacent provenance
and evidence contracts (25 passing tests in total). All 1,251 retained compiler-artifact
records replayed successfully across the 997 graph packages; changed Git revision
and duplicate membership replays refused. The replay fixture derives the graph
from the unchanged, receipt-bound Cargo inputs and relocates local metadata to
the recorded builder root. The packet did not retain the complete original
inventory/metadata, so this is resolver evidence, not a reproduced production
package. Its receipt is `artifacts/beta-delivery/notices-compiler-replay/result.json`.
All 63 failure-packet files passed checksum verification
under `artifacts/beta-delivery/candidate-failure-36792491264`. Build/source logs
and receipts were retained, but no native ZIP, setup, VSIX or artifact pair was
produced, and the copied compiler outputs were absent from the packet. No final
installed or acceptance row is complete from this attempt.
[PR #309](https://github.com/iokaio/vcp/pull/309) delivered the package-ID
correction with all required checks passing.

The separate BETA-09 editor refusal runner now covers actual restricted mode,
uninitialized/wrong-data/unselected-root failures and unsaved drafts invalidated
by typing, undo or close/reopen. It binds the final native/setup/VSIX bytes and
independent harness hashes, allowing only explicitly observed trust/controller
records while preserving task/accounting state. Seven editor contracts and a
real canonical preservation guard passed; both editor targets compiled with
Rust 1.95. Independent review corrected background-context timing and error
classification. These are runner checks; final installed observations remain
unrun, and successful reviewed apply still requires a live task binding.

## Installed helper qualification

[PR #304](https://github.com/iokaio/vcp/pull/304) delivered the independent
installed-helper runner with all required checks passing. It covers exact installed PDF, spreadsheet,
MCP pagination, catalog authoring and loopback browser helpers while binding the
native payload and dependency bytes before and after. Four focused contracts
passed, including changed catalog/resource bytes and redirected dependencies.
Two native regressions passed for sync-root exclusion and owned process-tree
supervision; the final installed test remains ignored until final artifacts exist.
Python and Playwright prerequisite inventories passed without dependency downloads
or browser/helper execution. This prepares BETA-09 evidence and does not complete
its final-artifact or clean-host rows.

## Hook responsiveness regression

[PR #306](https://github.com/iokaio/vcp/pull/306) delivered this test correction
and the acceptance map with all required checks passing. BETA-09 reproduced
the hook test failure retained by the SH close-out. The
session-start status/pause case passed; the completion case failed before its
hook began. A bounded diagnostic observed completion-hook start after 41.48
seconds, beyond the fixture's 35-second readiness limit, following unrelated
patch and Node verification work. Its status/pause/process-stop assertions passed.

The final test fixture removes that unrelated work and exercises a report task's
real completion hook. It preserves the original 35-second readiness, 10-second
control and 75-second outer deadlines, the real 55-second hook, held native
process handle and paused-state checks. Exact model-request counts and unchanged
source are additionally checked after terminal exit. The focused Windows test
passed 1/1 in 40.64 seconds on default SQLite; formatting, diff and independent
review passed. This is a test-only correction, not evidence for both stores or
final installed hook behavior. Logs are retained under
`artifacts/beta-delivery/hooks-responsiveness-{current,readiness-diagnostic,isolated-fixture}.log`.

## Candidate progress and timeout evidence

BETA-08 follow-up exposes each static stage identifier, UTC start/end time and
recorded result in the workflow console, outside redirected detailed logs.
The run receipt is saved before each announcement. Commands, private paths and
child output remain in their existing logs. The build-and-test step has a
330-minute timeout within the 360-minute job limit, leaving nominal headroom
for evidence collection and upload. Checkout and tool setup consume part of
that difference; runner loss or exhausted job time can still prevent retention.
The existing `always()` collection/upload conditions and native/test acceptance
deadlines remain unchanged. The already-running candidate uses its original
workflow; these changes apply to subsequent runs.
Thirteen focused tool/evidence contracts, PowerShell parsing and independent
review passed. No hosted timeout was induced. Step/job limits follow
[GitHub's workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstepstimeout-minutes).
[PR #308](https://github.com/iokaio/vcp/pull/308) delivered the progress/timeout
change; its required PR checks and subsequent main checks passed.

The fourth attempt also exposed a secondary collector error: packaging had not
produced `receipts.native`, but build validation tried to read it unconditionally.
The collector now verifies the fixed original `vcp.exe` and `vcp-launch.exe`
siblings independently, then checks native and pair bindings when those records
exist. Verified byte buffers are retained under `build-output/` only after source,
dependency, compiler and log checks pass. These are unpackaged diagnostics;
complete pair/stage requirements still govern production-identity acceptance.
PDBs, caches, target trees and private fixtures are excluded from this retention.
Fourteen evidence contracts passed, including absent final receipts, changed or
missing originals, redirected ancestors and a mismatched native receipt. Syntax,
diff and independent review passed. This prepares future failure capture and
does not recover the missing executables from the completed fourth run.
[PR #310](https://github.com/iokaio/vcp/pull/310) delivered this diagnostic
retention change; its required PR checks and exact-main checks passed.

## Canceled fifth candidate and smaller checkpoints

[Candidate run 36800517348](https://github.com/iokaio/vcp/actions/runs/36800517348)
selected reviewed source `629f2a9cdb14b3d0a6d8200e260e432e17a2221d`.
Its first attempt stopped in portable contracts: `p6-live-runner` and
`p6-bootstrap-profiles` exhausted their existing 30-second limits with no output,
and `p7-delegation` exhausted its 60-second limit after partial output. There was
no reported product assertion failure. The exact focused cases subsequently
passed locally under the original sanitized harness and unchanged limits:
11/11 live-runner cases, 4/4 bootstrap cases, and 25 delegation cases with four
existing conditional skips. This supports one unchanged retry, not a claim that
the runner image caused the timeouts. The local review is
`artifacts/beta-delivery/candidate5-timeout-review.json`.

Attempt two passed source selection, provisioning and portable contracts. The
owner canceled it during production compilation after 64 minutes 28 seconds in
that stage. The earlier completed production build took 64 minutes 22 seconds.
The canceled log contains 1,250 compiler artifacts across 997 package IDs,
including `vcp-launch.exe`, but no completed `vcp.exe`, build-finished event or
build receipt. The endpoint disk observation still had about 149 GB free on D:
and 31 GB on C:. All 55 retained checksums verified. Logs establish substantial
work, but lack per-output timestamps and CPU/memory observations: they cannot
establish a stall, remaining link time or a need for a larger runner. The packet
is retained locally at
`artifacts/beta-delivery/candidate-cancelled-36800517348-2`.

The BETA-08 revision replaces the single long workflow step with eleven named,
individually bounded steps. Dispatch defaults to `portable-contracts`; explicit
`production-build`, `pair` and `installed-editor` selections advance through
successively larger prefixes. A successful prefix is recorded separately from
full-pipeline completion and final manual acceptance. Each continuation checks
the preceding ordered successful stages, reviewed source, roots, scope and
log/receipt hashes; compiler/editor environment selection is restored in each
new process. A per-output lock refuses concurrent stage owners and atomic saves
preserve the preceding ledger on interrupted writes. Canceled/failed stages
cannot be resumed or reused through this interface.

Cargo progress now records phase, elapsed/quiet seconds, output bytes and
completed artifact count, while preserving complete compiler lines. Owned
Windows Job accounting supplies CPU seconds and peak committed memory for a
later capacity decision. Pre-receipt source/dependency/progress diagnostics are
retained as unverified evidence and cannot establish a successful build. Stage
limits total at most 325 minutes inside the existing 360-minute job; setup and
best-effort evidence retention consume the remainder. No Cargo concurrency,
runner size, fresh-target rule or acceptance gate is relaxed. The full candidate
was held until this revision was reviewed and verified.

Windows verification passed through the registered sanitized harness on Node
24.10.0: `beta-orchestration` passed 20/20 cases in 37.8 seconds and
`p8-distribution` passed 65 cases in 11.7 seconds, with its existing explicit
prepared-editor-archive case skipped because that input was not supplied to the
harness. The candidate still invokes that archive check separately with pinned
inputs. Manifests are under `artifacts/beta-delivery/staged-candidate-checks/`,
runs `e85d80f5-e8c6-49bf-adb2-35bc629cb5d5` and
`07c94c24-e2aa-42e2-9758-633f2469e802`, respectively. Sixteen repository/harness
unit tests, repository links/inventories, four PowerShell parses and whitespace
checks also passed. Independent review led to regressions for changed final-stage
logs and for a successful Cargo exit followed by a failed supervisor pipe drain;
the latter now records child and supervisor exits separately. These are synthetic
orchestration checks, not a rerun of the production build or installed matrix.

[PR #311](https://github.com/iokaio/vcp/pull/311) delivered the revision with
required PR and exact-main checks passing. The owner then resumed the plan.
The [portable checkpoint](https://github.com/iokaio/vcp/actions/runs/36810890848)
on reviewed source `17e8b4493d1a35a801c83c66eb623b2861c0941c` passed all
24 contract groups, including the three groups that had timed out in the earlier
attempt. Independent local verification matched all 54 packet checksums, the
exact source and the three successful selected stages. Its `selection_status`
is `pass`; `pipeline_status` remains `incomplete`, all eight later stages and
the manual matrix remain `not-run`. This is a successful checkpoint, not a
qualified artifact pair. The review receipt is
`artifacts/beta-delivery/checkpoint-36810890848-review.json`.

The subsequent [full candidate](https://github.com/iokaio/vcp/actions/runs/36811810217)
selected that same reviewed source and `installed-editor`. The owner canceled
it during production compilation and provisioned a larger Windows runner.
[PR #312](https://github.com/iokaio/vcp/pull/312) merged after all required checks,
and the exact-main [Delivery run](https://github.com/iokaio/vcp/actions/runs/36818215531)
passed. The [replacement candidate](https://github.com/iokaio/vcp/actions/runs/36818503347)
selects `6c14e17c5690f8f474f26f537c972ba58fbb5580` and `installed-editor`.
GitHub assigned it to `vcpwin-1000004982` in `wingroup`, with 16 Cargo jobs.
Starting this run does not complete the production, installed-product or manual
acceptance rows.

## Final installed configuration refusal increment

BETA-09 adds a separate package-bound installed-engine runner for six unchanged
CLI/start/resume controls and 24 stale-configuration refusals across Files and
SQLite. Controls must exhaust an intentionally unaffordable synthetic budget
without provider attempts, reservations, settlements, effects or MCP dispatch.
Refusals preserve canonical state and acknowledged history, allowing only
validated controller-lease changes. Invalid base/history also exercises CLI
and reconnect refusal. The runner validates the registered installation and
complete payload before/after, explicit qualification/Node/PowerShell hashes,
private unsynchronized output and natural process-tree completion. It retains
private diagnostics on failure and does not install or uninstall the product.

The source-only shared matrix passed all six controls and 24 refusals in 32
seconds with zero loopback requests. Existing positive import parity passed all
12 combinations in 82 seconds with unchanged assertions and deadlines. Its
synthetic MCP peer now uses the selected PowerShell 7 host: debug hashing the
89.9 MB Node executable three times could consume about 17 seconds and crowd
the existing 20-second RPC watchdog. Two measured hashes took 11.28 seconds;
source inspection identified the third. The actual replacement peer completed
initialize/list in 384 ms, within the unchanged one-second import deadline.
Production process pinning remains unchanged. Earlier timeout observations and
the incomplete 300-second baseline diagnostic remain retained, not passing
evidence. Compile and smoke receipts are under
`artifacts/beta-delivery/import-refusals-preparation/` and
`artifacts/beta-delivery/import-peer-smoke/`.
The existing 24 stale-import cases also passed in 14 seconds. All three observed
test processes finished naturally with empty owned Jobs. The consolidated
source-only receipt is
`artifacts/beta-delivery/import-refusals-preparation/peer-repair-validation.json`.

Seven runner contract tests passed, including exact PowerShell binding and
environment restoration. Independent review identified and closed explicit tool
propagation in the native hooks/import wrappers. These are source and runner
checks; the final installed 6+24 matrix remains unrun until the replacement
candidate produces a verified passing packet. The runner does not qualify live
provider execution, OS network denial, a clean host or full BETA-09 acceptance.

## Owner-provisioned Windows build capacity

The owner requested cancellation of [run 36811810217](https://github.com/iokaio/vcp/actions/runs/36811810217)
and replacement on the newly provisioned GitHub-hosted `vcpwin` runner in
`wingroup` (Windows Latest 2025, 16 cores, 64 GB RAM, 600 GB SSD). GitHub confirmed
the old run as canceled; no completed artifact pair is claimed from it.
All 60 retained packet checksums verified locally. Its last Cargo snapshot
records 3,751 elapsed seconds, 7,246 Job CPU seconds, four logical processors
and 9,802,227,712 bytes peak committed memory. That averages about 1.93 CPU
cores during the observed interval; it does not establish a stall or predict
the larger runner's completion time. The retained local review is
`artifacts/beta-delivery/canceled-36811810217-review.json`.

BETA-08 now selects that runner for the candidate, native Windows qualification
and storage handoff workflows. Each sets `CARGO_BUILD_JOBS=16`, and every build
wrapper receives the same explicit job count instead of the previous two-job
limit. The candidate accepts up to 16 jobs, matching the production builder.
Linux delivery coverage, isolated production/qualification targets, installed
test ordering, deadlines and acceptance gates remain in force. Replacement
qualification must select the newly reviewed main commit and pass its Delivery
checks before dispatch. No speedup or final qualification is claimed until that
run supplies evidence.

Eighteen existing candidate/state/storage/repository contract tests passed.
Repository validation checked 702 Markdown files and 3,154 relative links with
zero errors. PowerShell parsing and actual parameter binding accepted 16 jobs
and refused 17. Actionlint passed both workflows with the explicit custom
runner label declared, and independent review found no hidden build resource
cap or lost job-count argument. These checks validate the workflow change;
execution on the new runner remains a separate observation.

The replacement [run 36818503347](https://github.com/iokaio/vcp/actions/runs/36818503347)
selected reviewed source `6c14e17c5690f8f474f26f537c972ba58fbb5580` and ran on
`vcpwin-1000004982` in `wingroup`. Portable contracts passed. Cargo completed
the optimized engine and launcher in 19 minutes 25 seconds with exit zero;
the production stage nevertheless failed after 20 minutes 22 seconds because
the supervisor still observed an owned descendant ten seconds after its
broker exited zero. Forced cleanup emptied the Job. The packet does not
identify the remaining process, so its identity is not inferred from timing.
The last measurement records 16 logical processors, 8,666.5 cumulative Job
CPU seconds and 16,067,354,624 bytes peak committed memory over 1,176 elapsed
seconds. This is about 7.37 CPU cores on average, not full 16-core utilization
throughout compilation. Source, dependencies and toolchain remained stable.
All 67 retained packet checksums verified locally; the review is
`artifacts/beta-delivery/candidate-36818503347-review.json`. Packaging and
installed qualification did not run, and no passing artifact pair exists.

BETA-08's bounded local investigation reproduced the same child-zero,
broker-zero supervision failure with a tiny `/Zi /FS` C compilation. The
pre-cleanup snapshot identified the owned MSVC PDB server and `vctip.exe`.
A fresh PDB endpoint plus `_MSPDBSRV_=-shutdowntime 0` made the PDB server exit
naturally; the telemetry helper remained. These observations use local MSVC
14.50, whereas the failed hosted build used 14.51. They establish a reproduced
mechanism, not retrospective process identities for the hosted run. The retained
receipt is
`artifacts/beta-delivery/msvc-pdb-lifetime-d4bbc220-eb60-46a6-8f16-93afa64547b7/result.json`
(SHA-256 `82098b291332791fc8f820cf51dc0a1b248d327a6359330140434cd3c5e4ad61`).

The repair adds bounded pre-cleanup process and pipe diagnostics, preserves the
original production failure reason, and explicitly manages the selected MSVC
services. Its only planned termination is the pinned, owned telemetry service
after successful compilation and drained output. Unknown survivors and ordinary
forced cleanup remain failures. A short Windows compile/link and supervisor
contract job checks this behavior on `vcpwin` before another full candidate build.
The corrected local compile/link smoke passed with both compiler and broker
exit zero, all four object/executable/PDB outputs present, explicit termination
of the selected owned telemetry helper, no generic forced cleanup and an empty
Job. The one-second supervised probe retained
`artifacts/msvc-supervision-smoke/0a0a7fb6-b9c1-4b3c-be45-8578ccb93e3a/receipt.json`
(SHA-256 `daa6677fe833230ea21a3084b1db34d59cef3a4288791a33da4852d607ceea58`).
Independent review corrected an initial premature-failure race so that ordinary
descendants retain the existing ten-second natural-drain window.
The initial combined registered group passed all 28 cases within its unchanged
60-second limit, but took 59 seconds locally. Its retained manifest is
`artifacts/beta-delivery/build-supervision-validation/e8da9816-9922-43f7-9c5c-f33fed7a97e1/manifest.json`.
The six service-policy cases now have a separate `beta-msvc-services` group;
both groups retain their 60-second limits and remain in the fast suite. Both
registered groups passed, retained under `artifacts/beta-delivery/build-supervision-groups/`.
Forty-two focused provenance/evidence/state/stage checks, four PowerShell parses,
JavaScript syntax checks, actionlint, whitespace review and repository validation
(702 Markdown files, 3,155 relative links) also passed.

The new hosted check in [run 36823577754](https://github.com/iokaio/vcp/actions/runs/36823577754)
passed all 15 supervisor/service contracts and the real compile/link using
MSVC 14.51 on `vcpwin-1000004992` in `wingroup`. The contract process took
52 seconds; its workflow step took 62 seconds including startup. The real probe
recorded successful child/broker exits, an empty Job, explicit pinned telemetry
termination and no generic forced cleanup. Its downloaded receipt is
`artifacts/beta-delivery/windows-supervision-36823577754/e8d9136e-d2c4-4ff1-8958-fc77ea3b7eec/receipt.json`
(SHA-256 `7b48529460e8cd6825aa89d809daa9b2c440a3797bb7a74b4b8eb4d03d3903ac`).
The replacement candidate remains a separate observation; this repair alone
does not complete BETA-08 or BETA-09 acceptance.

The replacement [candidate run 36824747405](https://github.com/iokaio/vcp/actions/runs/36824747405)
used reviewed source `65654704de6aacb4beb7ccf923d6b9d58a7a23ba` after its
successful main Delivery checks. Its assigned runner was `vcpwin-1000005002` in
`wingroup`, with 16 Cargo workers configured for both compilation stages.
Production build/verification passed in 22 minutes 57 seconds, followed by native
packaging, setup, VSIX and independent pairing. Native qualification compilation
and boundary tests passed in 10 minutes 31 seconds. The production supervisor
recorded explicit cleanup of its pinned owned telemetry helper, successful
child/broker exits, an empty Job and no generic forced cleanup.
The downloaded packet verified all 91 checksums and has no artifact-validation
failures. Its exact pair is
`c3022ef9924640fc949a74430555219056a0a3a3eecd2b0d09e2097c214c3a88`.

Installed-native qualification failed before the console matrix: its Node
payload verifier exited unsuccessfully, with raw stderr retained only in the
hosted runner's private directory. Local reproduction against the unchanged downloaded payload
found that the console test passes a Rust-canonicalized Windows extended-prefix
script path that Node 24.10 rejects with `EISDIR`. Changing only that script
argument to its ordinary absolute spelling passed with the same extended-prefix
Node executable, payload path and restricted environment. Ordinary and
extended-prefix payload-directory validation also passed. The repair changes
only the script argument and verifies canonical equivalence; shipping engine,
installer and VSIX bytes are unchanged. The failed pipeline and unrun installed
editor stage remain explicit. Review and reproduction pointers are retained in
`artifacts/beta-delivery/candidate-36824747405-review.json`.

Focused local validation of that repair passed all eight actual installed
console cases (both stores, both console signals, direct and launcher), including
strict payload validation before and after. It exposed a second harness race:
Inno completed its final cleanup about half a second after the uninstaller
process returned. The smoke now observes both registration and program-root
disappearance for at most ten seconds, retaining failure and data-preservation
checks. A fresh actual install/configure/uninstall passed for both stores with
the sentinel and exact preference bytes preserved. The original failed run and
failed local wrapper receipts remain failed. Local evidence pointers are
`artifacts/beta-delivery/candidate-36824747405-console-continuation.json` and
`artifacts/beta-delivery/candidate-36824747405-uninstall-regression.json`.

The same extended-prefix issue was reproduced in PowerShell-to-Node launches.
Four editor/helper script arguments now use canonical-equivalent ordinary paths.
The editor observer now also preserves its installation on failed observations,
matching the native and editor-lifecycle runners. Three console regressions,
six shared interpreter-path regressions and all 25
evidence contracts passed, with targeted Rust compilation and independent
review clear. These checks do not claim installed-editor qualification. Compile
and test receipts are retained under `artifacts/beta-delivery/console-node-path/v2/`,
`artifacts/beta-delivery/script-path-refresh-18260191-74ff-4eba-ba27-bbae9753332c/`
and `artifacts/beta-delivery/uninstall-observation/`.

[PR #316](https://github.com/iokaio/vcp/pull/316) delivered those repairs with
successful PR and main Delivery checks. A focused installed-editor observation
on the unchanged pair then failed before installation: its canonical fixture
data path reached Inno's directory page as `\\?\C:\...`, which the page rejects.
The ordinary data path was 201 characters, below that page's length limit.
The same namespace prefix also conflicts with the editor's local-file URI
handling and later data-selection comparisons. The qualification boundary now
passes ordinary, canonically equivalent workspace and data-directory arguments
while preserving canonical fixture identities and rejecting redirected directory
inputs before conversion. The installed helper's engine argument receives the
same treatment after its existing path checks, preventing mismatched registration
comparisons and namespace-prefixed Node module paths. All nine focused path
regressions passed across the three freshly verified test targets, with natural
empty Jobs. Independent review is clear. Receipts are retained under
`artifacts/beta-delivery/directory-path-refresh/v2/`. The original failed
observation pointer is preserved in
`artifacts/beta-delivery/runner-history/editor-directory-b0071873-79c9-4f49-9917-fec9856b0b6f/`.
The corrected arguments passed setup's input checks; a second local attempt
then rolled back when the deeply nested diagnostic root caused an Inno notice
file rename to exceed the Windows path limit. That failure remains recorded in
`artifacts/beta-delivery/runner-history/editor-root-bbe26468-227d-44c7-9674-8669d4aee587/`.
The shorter-root observer retry and installer path-limit handling are separate
observations; no installed-editor pass is claimed by the argument repair.

[PR #317](https://github.com/iokaio/vcp/pull/317) delivered the directory-argument
repair with successful PR and main Delivery checks. The shorter-root observer
installed the native candidate and VSIX, then failed because VSCE scanned a
capture file exclusively held by the runner inside its working directory.
All three private driver manifests now list only their runtime files; actual
VSCE packaging reproduced the old locked-file error and passed with exact
archive inventories and preserved excluded evidence. Secret scanning stays on.
A separate real PowerShell probe showed that the scrubbed environment omitted
`PATHEXT`, preventing native invocation even by absolute path. The runner now
supplies a fixed `.EXE` baseline while retaining credential isolation.

The failed attempt's installation was retired only after matching its recorded
registration, launcher, engine/data selection, full payload and naturally
quiescent process tree. The complete retained workspace/data fixture was
byte-identical after uninstall; original failure evidence remains preserved.
The retirement pointer is
`artifacts/beta-delivery/candidate-36824747405-editor-retirement.json`.
Focused repair evidence is retained under
`artifacts/beta-delivery/editor-driver-fix/`; a fresh both-store observer retry
remains separate from these regression and retirement checks.

[PR #318](https://github.com/iokaio/vcp/pull/318) delivered the driver packaging
and fixed child-environment repairs with successful PR and main Delivery checks. The
focused observer retry then passed against the unchanged production pair for
both Files and SQLite, including the canonical paused-history assertions and
successful uninstall. Its pointer is
`artifacts/beta-delivery/candidate-36824747405-editor-observer-regression.json`.
This is a development-host diagnostic, not a passing candidate pipeline. The
original pipeline remains failed, and its setup logs still report a post-install
selection mismatch despite exit code zero. That installer verification boundary
requires repair before a fresh production candidate can qualify.

The remaining mismatch was reproduced with an actual hidden PowerShell child:
its OEM decoder corrupts the launcher's UTF-8 JSON paths. Verification now scopes
UTF-8 decoding to that invocation and restores the prior encoding, retaining
the exact candidate, archive, engine and data-root comparisons. The installer
also derives its application-root limit from the exact staged integration
inventory, including temporary filenames, and refuses an over-limit root before
ownership or activation mutations. The current maximum is 207 UTF-16 units.
Because pinned Inno catches post-install callback exceptions, a failed
verification now explicitly returns `1001` while preserving recovery state.
All twelve compiled synthetic installer cases passed, including installation at
the computed boundary, refusal one unit beyond it, post-verification failure,
recovery and data-preserving uninstall. The supervised process tree completed
naturally with no forced cleanup. Seven focused path/encoding regressions passed;
the distribution group passed 83 tests with its separately provisioned editor
archive check skipped. Repository checks and independent review also passed.
Evidence is retained under
`artifacts/beta-delivery/setup-boundary-40c9dee7cf7d4b64b1e313886b9ccd2f/`,
`artifacts/beta-delivery/installer-verify-encoding/` and
`artifacts/beta-delivery/setup-verification-distribution-check/`.
These synthetic checks do not qualify final production artifacts. Earlier
failed installer observations remain preserved.

[PR #319](https://github.com/iokaio/vcp/pull/319) delivered the installer repair
with successful PR and main Delivery checks. A focused lifecycle probe on the existing
production pair then reached the installed-extension inventory check, which
rejected VS Code's `.vsixmanifest` file as unexpected. The failure pointer is
retained under
`artifacts/beta-delivery/runner-history/editor-metadata-a1bc8c77bdae4dbb8fa86d7569355221/`.
Its native installation was retired after exact ownership, selection, payload
and natural process-quiescence checks. The retained workspace/data fixture was
byte-identical after uninstall; its separate retirement pointer is
`artifacts/beta-delivery/candidate-36824747405-editor-lifecycle-retirement.json`.
The installed extension and original failure logs remain available for diagnosis.
The inventory checker now maps the exact `extension.vsixmanifest` archive entry
to its installed filename and verifies its complete hash and length. It rejects
destination collisions, missing or changed metadata and unrelated extra files.
Four focused contracts passed, and a read-only check of all 59 files in the
retained installed extension passed against the original VSIX receipt. Evidence
is retained under `artifacts/beta-delivery/installed-editor-metadata/`; the fresh
both-store lifecycle observation remains separate from these inventory checks.

The next lifecycle attempt passed the installed-file inventory, then exposed a
second harness mismatch: the shared installer returned the launcher's extended
engine path after validating its ordinary equivalent, while the lifecycle guard
compared it to an ordinary path. The shared return now uses the already validated
ordinary engine path; launcher/schema/hash/data checks are unchanged. The failed
attempt and byte-preserving retirement pointers remain under
`artifacts/beta-delivery/runner-history/editor-engine-path-2ab0f34424db4a1ab27df45a91d6bd74/`.
The metadata repair was delivered in
[PR #320](https://github.com/iokaio/vcp/pull/320) with successful PR Delivery checks.
All 27 release-evidence contracts passed for the engine-path handoff repair,
including both path spellings, wrong engine/data selection, changed binaries and
retained-data checks. Repository checks and independent caller/lifecycle review
passed. Evidence is retained under `artifacts/beta-delivery/installed-engine-path/`.

A separate BETA-08 evidence review found that native qualification's final
output drain was absent from the console mirror, and private editor runner
failures exposed only temporary paths in the retained stage log. The collector
now retains the exact root-level native qualification log through its existing
sanitizer and ordinary-path checks. Editor observations emit a bounded summary
of supervision, output lengths/hashes, fixed markers and allowlisted result
fields before their existing assertions. Raw private output, stores, keys and
fixture paths remain excluded. Neither change alters installed-product
assertions or the selected candidate's shipping source.
Twenty-three evidence contracts passed. Both affected editor test targets
compiled, each passed four diagnostic regressions, and the existing natural
process-tree completion regression passed. Refusal cases cover malformed,
oversized and redirected captures, preserved existing diagnostic destinations,
and a real failed child whose synthetic private strings remain excluded from
public output. Independent review is clear. Compile and regression receipts are
retained under
`artifacts/beta-delivery/editor-evidence-refresh-80e6787c-9055-42de-8d56-cb274cfb6671/`;
previous test binaries remain preserved. These are harness checks, not an
installed-product pass.

## Native preflight before rebuilding the candidate

The ordinary engine-path handoff repair was delivered in
[PR #321](https://github.com/iokaio/vcp/pull/321); its PR and exact-main Delivery
checks passed. A fresh development-host lifecycle run against the original
verified pair from run `36824747405` then passed all ten observations across
Files and SQLite, including reload, restart, unsupported protocol, truncated
update, missing engine and reconnect behavior. Both installations were removed
after payload, process-tree and retained-data checks passed. The private result
is bound by `artifacts/beta-delivery/candidate-36824747405-editor-lifecycle-regression.json`.
This closes the focused harness regression, not final candidate or clean-host
acceptance; the original candidate pipeline remains failed.

The fresh [candidate run 36839649846](https://github.com/iokaio/vcp/actions/runs/36839649846)
selected exact source `238fbf8531ede792925603c31785efbcb997793d` after successful
main Delivery checks. On `vcpwin`, its production build passed in 15 minutes
48 seconds; native ZIP, setup, VSIX and pair verification also passed. Native
qualification compiled successfully in 4 minutes 11 seconds, then failed one
configuration-parity observation: the delayed imported MCP case returned
`The pipe is being closed. (os error 232)` instead of its expected deadline
reason. The other three parity tests passed; installed native/editor stages
were not run. No timeout was increased or assertion relaxed.

All 91 downloaded packet files passed independent checksum and inventory
verification, with zero evidence-validation failures. The verification receipt
is `artifacts/beta-delivery/candidate-36839649846-download-verification.json`.
Artifact integrity does not change the failed pipeline status. The unchanged
positive import test subsequently passed locally under its original limits,
so that pass alone does not explain or close the hosted failure. Investigation
and focused native checks precede another full production rebuild.

[PR #322](https://github.com/iokaio/vcp/pull/322) delivered the explicit
`native_boundaries_only` Delivery dispatch with all required PR checks passing.
It requires the exact main selection and successful push checks, uses `vcpwin`
with 16 jobs, and combines lifecycle duplex plus the four CLI targets in one
Cargo selection before the package lifecycle fixture. Separate concurrency
preserves ordinary Delivery runs. Five workflow contracts passed, including
actual selection refusals and incomplete/changed evidence rejection; an isolated
two-package Cargo fixture verified the combined selection. The source-only
preflight does not build production artifacts or replace candidate acceptance.

A deterministic native transport regression reproduced the same Windows error
232 on the unchanged runtime after its supervisor had recorded `process deadline
elapsed` and the owned Job had emptied. The natural peer-exit control retained
its ordinary OS error and passed. Duplex reads already consulted the recorded
stop reason; writes and flushes omitted that check. The repair gives admitted
writes the same authoritative stop-reason check while retaining raw IO errors
when no reason is recorded. It preserves lifecycle admission priority, write
cancellation cleanup and all existing limits. All nine corrected duplex tests
passed, including deadline closure, a backpressured write, natural peer exit,
sealed admission and partial-write cancellation. Independent review found no
blockers. The rebuilt CLI parity target also passed all four nonignored tests:
the twelve allowlist/deadline cases, stale-configuration refusals, the six
source controls and twenty-four source refusals, and coding parity. Scoped
Rust formatting and repository checks passed. Baseline and corrected evidence is retained under
`artifacts/beta-delivery/import-pipe-investigation/`.

The exact production setup from failed run `36839649846` separately passed a
31-second development-host diagnostic: Unicode app/data paths, registered
launcher/engine selection, real Files and SQLite preferences, successful
uninstall and unchanged retained data. Setup reported successful installation;
the test left no registration. All 109 bound artifact/tool/harness inputs were
unchanged. The pointer is
`artifacts/beta-delivery/candidate-36839649846-installer-regression.json`.
This verifies the repaired installer on these bytes; it does not change the
pipeline failure or claim console/editor, clean-host or final qualification.

## Host and remaining external observations

The development workstation is Windows `10.0.26300.0` with development tools and
caches. Results from it cannot fill the clean-Windows row. GitHub authentication
works through the host keyring outside the restricted execution sandbox. No
provider calls, public release or signing operation has run. Explicit MiniLM
acquisition and independent verification passed for all ten pinned files
(91,578,299 bytes), retained outside the repository for final-engine tests;
no inference has run. The local receipt is
`artifacts/beta-03/minilm-acquisition.json`.

Candidate review corrected failure-log discovery to recognize the setup builder's
32-character GUID directories and the production builder's hyphenated GUIDs.
Eight evidence tests passed, including exclusion of private and nested files.
The final Windows fast-suite attempt on source `351160fb` exposed a workstation
Node junction rejected by existing plain-path checks; the eleven affected
provider-runner tests passed with Node's ordinary filesystem path. That attempt
also timed out in the skills group. The complete rerun with the ordinary Node
path passed, including that group, without changing any timeout or security check:
`artifacts/tests/0acbc416-2a24-4b1c-9e39-3305925be6aa/manifest.json`.

The original source-hashing failure was reproduced and traced to the restricted
sandbox's different Windows owner: sanitized child environments omit the injected
Git ownership exception. The harness suite passed when executed as the repository
owner, without changing global Git trust. Its receipt is
`artifacts/tests/46511575-b894-436b-a2d9-01e20505b242/manifest.json` (local generated
evidence). The earlier interrupted TypeScript builds remain historical observations
until fresh checks establish current status.
