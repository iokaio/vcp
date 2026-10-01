# Internal beta implementation status

Started September 30, 2026 from source `b0d84a2df6e5fa7ed3163a3b1920fcf620623213`.
Contract: [release plan](00-release-plan.md). Decisions: [ADR-072](../adr/072-internal-windows-beta.md).
The owner's request covers implementation through internal-beta readiness and
manual-test preparation. Publication and paid execution remain separate gates.
The [FR/I/U acceptance map](03-acceptance-map.md) joins current changes and
historical dispositions without granting final-artifact or owner acceptance.

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
| BETA-08 | Workflow and evidence delivered | [PR #298](https://github.com/iokaio/vcp/pull/298) merged with routine checks passing. Exact-main dispatch gate, pinned tools, fresh production engine/setup/VSIX construction, separate native qualification target and actual installed synthetic observer smoke. Seven evidence regressions passed, the installed-editor target compiled, and a clean-fixture directory race was corrected and rechecked. Evidence validation preserves failed installation roots and refuses incomplete or changed build records. No final production run yet. |
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
has not been restarted while this revision is reviewed and verified.

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
