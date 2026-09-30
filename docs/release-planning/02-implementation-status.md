# Internal beta implementation status

Started September 30, 2026 from source `b0d84a2df6e5fa7ed3163a3b1920fcf620623213`.
Contract: [release plan](00-release-plan.md). Decisions: [ADR-072](../adr/072-internal-windows-beta.md).
The owner's request covers implementation through internal-beta readiness and
manual-test preparation. Publication and paid execution remain separate gates.

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

The separate BETA-09 editor refusal runner now covers actual restricted mode,
uninitialized/wrong-data/unselected-root failures and unsaved drafts invalidated
by typing, undo or close/reopen. It binds the final native/setup/VSIX bytes and
independent harness hashes, allowing only explicitly observed trust/controller
records while preserving task/accounting state. Seven editor contracts and a
real canonical preservation guard passed; both editor targets compiled with
Rust 1.95. Independent review corrected background-context timing and error
classification. These are runner checks; final installed observations remain
unrun, and successful reviewed apply still requires a live task binding.

## Current environment limitations

The independent installed-helper runner covers exact installed PDF, spreadsheet,
MCP pagination, catalog authoring and loopback browser helpers while binding the
native payload and dependency bytes before and after. Four focused contracts
passed, including changed catalog/resource bytes and redirected dependencies.
Two native regressions passed for sync-root exclusion and owned process-tree
supervision; the final installed test remains ignored until final artifacts exist.
Python and Playwright prerequisite inventories passed without dependency downloads
or browser/helper execution. This prepares BETA-09 evidence and does not complete
its final-artifact or clean-host rows.

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
