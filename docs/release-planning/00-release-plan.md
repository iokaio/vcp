# First Windows beta release plan

Review date: September 30, 2026. Reviewed source: `b0d84a2df6e5fa7ed3163a3b1920fcf620623213` (`main` merge of PR #289).

Status, revised October 1, 2026: **unsigned beta published for manual testing**. The [selected installer/ZIP/VSIX pair and focused installed results](05-manual-candidate.md) complete the manual-testing milestone below. The owner-authorized [GitHub prerelease, Pages deployment and custom-domain HTTPS](../development/beta-publication.md) are delivered. Full qualification remains incomplete.

This document records the original code review and implementation contract. Implementation was authorized on September 30, 2026 for an **internal beta and manual testing**. Current decisions and delivery evidence are tracked in [the implementation ledger](02-implementation-status.md) and [ADR-072](../adr/072-internal-windows-beta.md). The review findings below are historical baseline observations, not claims about later implementation. Neither the review nor the implementation request authorizes paid qualification or publication.

On October 1 the owner approved reducing the immediate milestone after reviewing
the disproportionate effort spent on qualification infrastructure.
[ADR-073](../adr/073-manual-testing-candidate.md) supersedes ADR-072's requirement
to finish the full qualification matrix before beginning owner manual testing.
It preserves runtime safeguards, truthful evidence and the separate publication gate.

The owner subsequently chose GitHub Releases and Pages and explicitly authorized
limited beta downloads under [ADR-074](../adr/074-github-beta-downloads.md).
Implement the [publication workflow](../development/beta-publication.md) using
the existing verified pair without rebuilding it. This extends BETA-08/BETA-11
only for that publication; it does not reopen the deferred qualification work or
grant signing, Marketplace or paid-provider authority. The owner handles DNS.

The owner then created Marketplace publisher `iokaio` and authorized the
[manual-upload preparation](../adr/075-marketplace-manual-upload.md): migrate the
extension identity, build and verify a new pair, publish its GitHub downloads and
finish managed HTTPS. The owner retains the first Marketplace upload. The
[upload guide](../development/marketplace-publication.md) tracks that handoff;
full qualification and paid-provider gates remain unchanged.

## Immediate milestone and stopping rule

Owning items: **BETA-08/BETA-09**, limited to preparation for owner manual testing.
Use the [manual-testing checklist](04-manual-testing-checklist.md) for the handoff.

1. Preserve the unfinished qualification patch separately. Carry forward only a
   demonstrated product or packaging blocker needed for this milestone. Do not
   complete generic process supervision, test-harness hardening or exhaustive
   synthetic campaigns as prerequisites for the first manual test.
2. Select reviewed clean `main` with successful ordinary Delivery checks. Run the
   existing `Internal beta candidate` workflow once with `stop_after=pair` on
   `vcpwin` in `wingroup`, using its configured 16 build jobs. Verify the retained
   source/build identities, pair, inventories, notices and all three artifact hashes.
3. On those exact bytes, outside the checkout in a private unsynchronized fixture,
   check registered installation, launcher/version, supported onboarding and its
   missing-input guidance, VSIX installation and connection, and uninstall with
   unchanged test workspace/data. Cover both stores where these checks create
   state. Record the actual host; developer-host evidence is sufficient for this
   limited handoff and does not establish clean-host support.
4. Provide the exact pair, prerequisites, short manual checklist, focused results,
   known failures and explicit unrun rows to the owner. A useful live task and
   editor interaction are manual observations; credentials and bounded provider
   spend must be supplied/admitted before paid execution. Do not invent a live pass.

Stop implementation when this handoff is reviewable. Record a focused failure
before deciding whether it is a product blocker, harness defect or deferred
qualification issue. Fix a demonstrated blocker at its boundary and rerun its
affected check; rebuild only when candidate bytes must change. Do not begin
another full candidate or broaden testing merely because additional checks exist.

The pair checkpoint may have `selection_status=pass`,
`pipeline_status=incomplete` and overall `status=qualification-required`, with
the later native/installed stages `not run`.
That is the intended outcome at this milestone, not a completed BETA-09 or BETA-11.
Keep the existing full-qualification runner guards intact. A security, permission,
secret-handling, accounting or data-loss defect remains a stop condition.

Deferred from this handoff: clean standard-user Windows qualification, distinct
production upgrade/rollback, exhaustive editor/refusal/history/startup campaigns,
independent-machine recovery, physical full-volume/network-denial observations,
minimum-hardware claims and owner quality scoring. Preserve their actual status
in section 5 and the acceptance map; do not describe them as passed or excluded
from eventual release acceptance. Public release, Marketplace and wider
distribution remain separately authorized activities.

## 1. Scope and release baseline

The owning contracts are [P8 native distribution and acceptance](../plan/15-integration-and-release.md), [P4 editor integration and packaging](../plan/18-deferred-vscode.md), [ADR-012 distribution](../adr/012-clients-and-distribution.md), and [ADR-018 release acceptance](../adr/018-release-acceptance.md). [ADR-042](../adr/042-owner-directed-p8-closure.md) closed P8 for development progression while retaining its evidence gaps and publication gate. P9 and P4 are complete in the [current ledger](../plan/20-traceability.md); they do not need to be implemented again.

Recommended first beta envelope:

- Native Windows x64 CLI and a separately installed `win32-x64` VSIX. Keep the existing local engine/SDK boundary and explicit User-setting selection of the engine.
- A per-user Windows setup executable with uninstall registration, backed by the existing verified payload and side-by-side release mechanism. Retain the portable ZIP for diagnosis and expert use. Select the installer framework during implementation; none is selected by the current repository.
- Downloadable installer and VSIX as one identified candidate pair. Marketplace publication can follow separately; it is not necessary to install a VSIX.
- Qualify and name the actual supported Windows and VS Code builds. Historical evidence covers Windows x64 build `10.0.26200.0` and VS Code `1.138.0`; it does not establish all Windows 10/11 builds or a minimum hardware floor.
- Keep both canonical stores in acceptance where state changes. Local model assets remain an explicit, hash-verified acquisition. Ecosystem runtimes and optional skill helpers need a documented dependency matrix.

Do not add remote/WSL/container support, an engine auto-updater, a background service, arbitrary binary downgrade, or cross-format migration to this increment. A beta label does not waive authorization, accounting, user-data preservation, privacy, or truthful completion requirements. Any narrower product/support claim needs an explicit disposition; a previously skipped check stays skipped until executed.

## 2. What is already implemented

| Component | Evidence in current code | Release implication |
| --- | --- | --- |
| Production native build | [build-production.ps1](../../scripts/build-production.ps1) builds optimized Windows MSVC x64 `vcp` with locked/offline dependencies, static CRT, source inventories, tool identities and a build receipt; rejects qualification features. | Reuse this recipe and prove it against the current dependency closure on the release builder. Successful historical compilation is insufficient. |
| Native payload assembly | [package.ps1](../../scripts/package.ps1), [package-inventory.cjs](../../scripts/package-inventory.cjs) stage an explicit executable, built-in skills, notices, model specification and tools; verify payload hashes and ZIP entries. | There is a real distribution path; do not replace it with a broad checkout archive. |
| Installation lifecycle | [package-install.ps1](../../scripts/package-install.ps1) supports install, upgrade, rollback and uninstall, with owned roots, path/hash validation, disjoint data roots, side-by-side releases and active-pointer replacement. | Preserve these controls when adding setup UI, launch integration and registration. |
| Model and skill assets | [package-models.ps1](../../scripts/package-models.ps1) verifies pinned asset sizes/hashes. [skills.rs](../../src/crates/vcp-cli/src/skills.rs) discovers built-ins beside the executable. | Keep the full payload layout; a launcher must not detach `vcp.exe` from its assets. No ONNX Runtime DLL requirement is established by the CPU Candle implementation. |
| VSIX assembly | [package.cjs](../../src/packages/vscode/scripts/package.cjs) uses pinned VSCE, a Windows target and independent ZIP inventory checks. [stage.cjs](../../src/packages/vscode/scripts/stage.cjs) bundles the actual SDK/schema and allowlisted runtime modules. | Build the existing extension; no separate runtime npm installation is required for users. |
| Editor security/lifecycle | [extension sources](../../src/packages/vscode/src), [SDK local transport](../../src/packages/sdk-ts/src/local.ts), and [editor packaging record](../development/editor-packaging.md) cover explicit executable selection, local authenticated pipes, observer reconnect, controller ownership and guarded edits. | Preserve workspace trust, current-buffer revision checks, no automatic uncertain-command replay, and independent engine/data ownership. |
| Historical package qualification | [Native distribution guide](../development/p8-distribution.md), [September 23 production candidate](../evaluations/p8-allowance-package-2026-09-23.md), and [P4-05 record](../development/editor-packaging.md). | Useful evidence to reuse selectively. Those results identify older bytes; they are not a qualification receipt for this commit or a future signed/repacked artifact. |

The [skills replacement](../research/skillsplan-new.md) and the SU/SH implementation ledgers in the [skills upgrade plan](../research/skills-upgrade-plan.md) are complete. Their earlier review findings must not be copied wholesale into this backlog. Use current helper tests, package integrity and applicable runtime regressions; do not reinstate a universal comparative model campaign.

## 3. Findings and remaining work

Priority **blocker** means required before distributing the proposed usable beta. **Qualification gap** means missing or stale evidence, not a proven implementation failure. Findings marked source review have not been reproduced as runtime failures in this review.

### F01 — Imported restrictions differ between CLI and editor/SDK execution

**Blocker; defect identified by code inspection.** [settings.rs](../../src/crates/vcp-cli/src/settings.rs), around lines 202–225, loads the native import store, validates the base digest and materializes selected preferences. [local/execution.rs](../../src/crates/vcp-cli/src/local/execution.rs), around lines 193–194 and 212–290, instead pins, reads and deserializes the base profile directly. Its execution path does not load the selected `.vcp-imports` revision.

Consequently, imported MCP tool restrictions and deadlines applied by CLI startup are not incorporated by this editor/SDK path. The base-file digest check also does not detect a changed or corrupt import chain. Existing durable-policy checks remain, but they do not replace loading the effective configuration promised by [configuration imports](../development/configuration-imports.md).

Required: share validated effective-profile loading across clients and bind both the base profile and selected imported revision/content to execution admission. Add native CLI/editor-or-SDK parity tests for narrowed tools/deadlines, changed import revision, changed base file, corrupt import state and resume. Refuse stale configuration without dispatch or silently granting broader access.

### F02 — Installation does not yet lead to a supported first task

**Blocker; missing product workflow.** [Profile](../../src/crates/vcp-cli/src/settings.rs) requires workspace binding, trust/autonomy, limits, processes/checks, a provider snapshot and matching captured catalog. `prepare` checks snapshot expiry and exact catalog equality. [CLI execution](../../src/crates/vcp-cli/src/app/execute.rs) also requires the provider credential. [args.rs](../../src/crates/vcp-cli/src/args.rs) has no setup/init or provider-metadata renewal command. The default profile location is selected in [app.rs](../../src/crates/vcp-cli/src/app.rs).

The [distribution smoke](../../scripts/evals/distribution-qualification.ps1) tests missing-profile diagnosis, not a new user's successful setup. [doctor.rs](../../src/crates/vcp-cli/src/doctor.rs) primarily checks path/vault separation. The developer `vcp-provider-conformance` binary is qualification-only and explicitly does not publish a production snapshot/profile; shipping it is not a complete solution.

Required: provide a supported profile/catalog creation and renewal workflow, either a bounded setup command or complete, tested expert-beta instructions with supported tooling. Explain workspace initialization, credential input, budgets, trust, available checks/tools, expired metadata and multiple workspace profiles. Keep credentials out of command arguments, packages, logs and workspace settings. Do not bypass expiry, provider conformance, pricing validation or explicit trust to make onboarding easier. The extension must guide an uninitialized workspace through this workflow instead of relying on developer fixtures.

### F03 — Windows installation is a script, without stable launch integration

**Blocker for the requested installer experience.** [package.ps1](../../scripts/package.ps1) emits `vcp-windows-unsigned.zip`; [package-install.ps1](../../scripts/package-install.ps1) requires PowerShell 7 and records `active.json` under an explicitly supplied root. It does not provide a registered setup executable, Start menu/CLI entry point or installed-app removal entry.

The installer records `data_root`, but [settings::default_data](../../src/crates/vcp-cli/src/settings.rs) uses `%LOCALAPPDATA%\VCP`; the CLI does not consume `active.json`. A user launching the selected executable without `--data-dir` can therefore select a different data location from the installation choice.

Required: build the setup artifact and a stable, safely resolved launch path that preserves the chosen data directory and argument/exit/cancellation behavior. Offer deliberate PATH/shortcut integration; do not silently modify unrelated settings. The extension should retain explicit User consent for engine selection and use the intended native executable and data root after an upgrade. Update the install-root allowlist before adding launcher/uninstaller files; existing uninstall intentionally rejects unexpected entries.

### F04 — Candidate packaging accepts inputs that a release must reject

**Blocker; release-policy gap.** [package.ps1](../../scripts/package.ps1), around lines 60–78, permits an absent build receipt, checks only schema/exit/executable hash when one is supplied, derives architecture from the packaging environment, and writes a literal `vcp-cli/0.1.0` compatibility label. [VSIX packaging](../../src/packages/vscode/scripts/package.cjs) accepts `caller-supplied-unverified` native provenance and records rather than rejects dirty source.

Required: add a release gate around the existing candidate builders. Require reviewed clean source, stable production inputs, the actual target and product version, qualification-feature exclusion, and exact binding between native build receipt, payload assets, VSIX and release source. Reject mismatched or stale build/source/package inputs. Preserve exploratory candidate packaging as such; a successfully assembled ZIP is not a release endorsement.

Record a single candidate identity connecting source commit, toolchains/locks, upstream selections, skill catalog, model specification, store/config/index compatibility, protocol/schema, native executable, installer, ZIP and VSIX digests. Product versions, data formats and wire protocol versions have different meanings and should not be advanced together automatically.

### F05 — Beta version/channel metadata is still qualification-oriented

**Blocker for a traceable beta pair.** Native, SDK and extension labels remain `0.1.0`. [VSIX package.cjs](../../src/packages/vscode/scripts/package.cjs) accepts only `0.1.patch` overrides and calls every override a qualification successor. [stage.cjs](../../src/packages/vscode/scripts/stage.cjs) hardcodes bundled dependency versions. [engine_connection.ts](../../src/packages/vscode/src/engine_connection.ts) and SDK client code contain literal client version strings.

Required: define beta artifact names and versions, derive shipped metadata consistently, and distinguish real releases from same-source upgrade fixtures. Decide whether to retain the exact `engines.vscode` version or qualify an explicit wider range; do not widen it without tests. Add beta release notes, user-facing compatibility/install information and support links to the staged VSIX.

For a Marketplace pre-release, use VSCE's pre-release flag and a numeric `major.minor.patch` extension version; a Rust-style `-beta.1` suffix is not the supported extension version mechanism. Downloadable VSIX distribution does not require Marketplace publication. See Microsoft's [packaging and pre-release guidance](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#pre-release-extensions). Publisher ownership and publishing credentials have not been verified by this review.

### F06 — Shipped dependency notices need an artifact-level audit

**Blocker; distribution inventory gap.** The native assembler copies root `LICENSE`, `NOTICE` and `THIRD_PARTY_NOTICES.md`, plus skill-local assets. [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) links separate source-tree notices and explicitly says the recorded dependency graphs are not a release notice bundle. For example, the PostgreSQL/Snowball attribution files referenced there are not explicitly copied by `package.ps1`.

Required: inventory the actual enabled native dependency graph and shipped native/helper/model/skill components, generate a release notice/license directory and machine-readable component inventory, and verify it is present in the artifacts. Resolve applicable attribution and redistribution requirements for that exact graph. Source-only or development-only dependencies must not be confused with shipped ones. This review does not conclude that every component in the vendored workspace is linked or that a particular license violation has occurred.

Separate build prerequisites from user prerequisites: Node/TypeScript/VSCE/Rust/MSVC are build tools; VS Code supplies the extension host; the current install/model scripts need PowerShell 7; Git, project toolchains, Python packages, browser binaries and other helper requirements depend on the workflow. Inventory those optional dependencies and provide bounded missing-dependency diagnostics instead of silently downloading arbitrary tools.

### F07 — Upgrade/rollback claims exceed what metadata alone proves

**Qualification gap with a hardening requirement.** [package-install.ps1](../../scripts/package-install.ps1) checks compatibility declarations; `StateManifest` is optional. It does not automatically establish the actual canonical-state format. There is no installer-wide operation lock evident in the script. Atomic active-pointer replacement alone does not demonstrate safe competing install/upgrade/uninstall operations.

Required: serialize installer mutations and qualify concurrent attempts, interruption at staging/publication/activation, locked binaries, insufficient disk space and recovery. Test real retained Files and SQLite stores across distinct supported beta builds, not just identical binaries at different paths. Refuse unsupported state or rollback without modifying it. Keep cross-format migrations unsupported unless implemented with validated snapshots and staged recovery.

[package-install.test.ps1](../../scripts/package-install.test.ps1) already exercises many path, lock, ownership, interruption and sentinel cases using a fake executable. Historical production tests also exist. Extend/reuse both; neither substitutes for lifecycle tests using the final beta installer and actual state. The concurrency concern is source-based, not a reproduced race in this review.

### F08 — CI does not produce and qualify a release candidate pair

**Blocker; automation/evidence gap.** [.github/workflows/ci.yml](../../.github/workflows/ci.yml) runs portable delivery/TypeScript checks, helper tests and selected Rust skill tests. Native Windows qualification is manually dispatched. It does not build a production installer/VSIX pair and run actual installed-editor lifecycle qualification as a release gate. Existing evidence retention is short-lived CI retention, not a durable release packet.

Required: a pinned Windows candidate workflow with explicit production build, package inventory, installer and VSIX construction, native boundary tests, artifact installation and evidence retention. Provision the locked dependency cache explicitly before the offline production build. Qualify the production Rust/toolchain choice on current source rather than assuming the older recipe still compiles. Keep test/qualification binaries separate from shipping binaries and bind every result to the tested artifact. Make publication a distinct authorized step after review; do not auto-publish on ordinary PR checks.

### F09 — Current beta needs fresh installed-product and security evidence

**Qualification gap.** [P4-05's record](../development/editor-packaging.md) explicitly identifies dirty source and unverified native provenance; its `0.1.1` update used the same source and engine replacement used the same native build at another path. The 130-version startup probe also preceded its final cleanup refinement. Preserve those results without treating them as current beta acceptance.

Required: install the final bytes outside the checkout on clean native Windows without developer caches/PATH, then qualify the matrix in section 5. Measure first-run/reopen, pause/stop and retained-history behavior against declared thresholds and a recorded resource envelope. Include relevant recent skills, imports, hooks, memory and editor changes in the current evidence mapping.

[ADR-042](../adr/042-owner-directed-p8-closure.md) retains clean-Windows, production network-isolation, minimum-hardware, physical-full-volume and owner-evaluation gaps; independent-machine recovery was skipped. Classify each against advertised beta claims. Required unrun cases remain blockers; explicitly excluded support claims must remain visible. Do not present same-host restore as independent-machine recovery, nor a synthetic disk error as full-volume evidence.

### F10 — Signing, user documentation and final acceptance are unresolved

**Release decision and delivery gaps.** Native packaging declares an unsigned local candidate. No signing identity or public release approval is established by the code. Signing changes executable and archive hashes: the current build-receipt equality checks need an explicit unsigned-build → signed-payload provenance step if signing is introduced.

Recommended public-beta path: sign and timestamp native executables and the setup artifact, verify signatures, then qualify the final signed bytes and publish final checksums. If an explicitly unsigned limited beta is selected, retain that designation and its limitations rather than implying publisher authentication. Keep keys outside the repository and logs. Microsoft's [SignTool documentation](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool) describes signing, timestamping and verification; this review did not verify certificate availability or purchase anything.

The [root README](../../README.md) still says the installable application is not implemented and describes an early P1 status. Replace stale front-door guidance with the actual beta scope, setup/renewal, data locations, upgrade/uninstall, compatible recovery, model/helper prerequisites, troubleshooting and known issues. Provide safe support instructions that do not ask users to upload plaintext histories, credentials or recovery keys. Final owner acceptance and publication authorization remain separate from automated checks.

## 4. Sequenced work items

The `BETA-*` IDs below are release-preparation records. They map to existing owners without changing the historical 56-task closure or claiming existing tasks were never delivered. All were **planned** at review time; consult [current status](02-implementation-status.md) for subsequent implementation and verification.

| ID | Priority / owning boundary | Depends on | Deliverable and acceptance |
| --- | --- | --- | --- |
| **BETA-01** | Blocker; P8-04/P4-05 | None | Record beta channel, target support envelope, artifact/product versions, installer mechanism and signing disposition. Name excluded platforms/claims, required evidence and external publication inputs. |
| **BETA-02** | Blocker; P10-02/P9-02/P4-03 | None | Fix F01 using shared effective-profile loading and revision binding. Native CLI/public execution parity and invalid/stale-import refusal pass on both stores. No unauthorized dispatch. |
| **BETA-03** | Blocker; P2-02/P2-03/P8-04 | BETA-01 | Deliver F02's supported first-run/profile/catalog-renewal workflow. A new user can initialize a workspace, select policy/budget, supply a credential and run the documented task without developer fixtures; missing/expired inputs have actionable errors. |
| **BETA-04** | Blocker; P8-04/P4-05 | BETA-01 | Add release versions, strict build/pair provenance and signing transformation receipts where applicable. Reject dirty/unverified/mismatched inputs in the release path; retain complete hashes and compatibility records. |
| **BETA-05** | Blocker; P0-07/P8-06/P7-02 | BETA-04 | Inventory actual distributed dependencies/assets and supply applicable license/notice files. Verify archive contents, resource hashes, helper prerequisites and absence of secrets/development fixtures. |
| **BETA-06** | Blocker; P8-04 | BETA-03, BETA-04 | Build registered per-user setup, stable launch/data-root handling and installer operation serialization. Pass native lifecycle/path/preservation tests, including a real supported upgrade/rollback pair and interrupted/competing operations. |
| **BETA-07** | Blocker; P4-01–05 | BETA-02, BETA-03, BETA-04, BETA-06 | Build actual beta VSIX with correct versions/channel, bundled SDK/schema, support docs and installer-compatible onboarding. Install and connect using explicit User settings; preserve trust, buffer revisions and observation-only reload. |
| **BETA-10** | Blocker; P8-04/05/P4-05 | BETA-01, BETA-03, BETA-06, BETA-07 | Update README, packaged quickstarts, compatibility/known issues, recovery and support instructions. Stage packaged documents before candidate construction; BETA-09 tests the walkthrough and claims. External release evidence is finalized in BETA-11. |
| **BETA-08** | Manual-candidate prerequisite; P8-01/04/P4-05 | BETA-04–07, BETA-10 | Use the delivered workflow's pair checkpoint to produce one matching installer/ZIP/VSIX and durable identity/evidence packet. Later native/installed stages remain explicitly unrun at this checkpoint. Full automated pipeline qualification remains a follow-up. |
| **BETA-09** | Focused manual handoff now; full release gate later; P8-01–05/P4-05 | BETA-08 | Execute the immediate milestone's focused installed checks and prepare owner manual testing. Keep section 5's broader matrix open for later qualification; this handoff does not complete the work item. Paid/live observations require available access and bounded authorization. |
| **BETA-11** | Publication gate; P8-05 | BETA-09, BETA-10 | Present the exact artifact pair, scorecard, known limitations and checksums for factual owner acceptance. Obtain publication authorization, then distribute those bytes. Marketplace upload is a separate optional delivery destination. |
| **BETA-12** | Next candidate; P3-01/P3-06/P2-02/P6-01 | BETA-03 | Guided CLI setup, added by owner direction on October 2, 2026. Increments: first-contact diagnostics and output, readable results and a readiness `doctor`, per-workspace profile selection, live price estimates, model sets (Qwen 3.8 Max quick test plus vendor and capability-level sets), opt-in Windows Credential Manager key storage, an interactive `vcp setup` wizard with an optional smoke test, renewal, explicit per-role model assignment, and switching between prepared sets. Every paid step shows computed minimums and needs typed budgets and confirmation; no model substitution, retries or pre-filled spending. Scripted and JSONL protocols stay unchanged. |

Suggested delivery order: start BETA-01 and the independently actionable BETA-02; then do onboarding and release identity in parallel. Build installer, notices and VSIX increments on those contracts. Stage BETA-10's packaged documentation before freezing the candidate, and assemble CI/evidence collection before the final qualification run. Any later change to packaged documentation or other payload bytes requires new hashes and affected requalification. One independently reviewable PR per increment is preferable; do not wait until the final installer to fix the runtime parity defect.

## 5. Required beta qualification matrix

This remains the **full release-qualification backlog**. Under ADR-073, completing
every row is no longer a prerequisite for the immediate owner manual-testing
handoff. Record focused evidence in the applicable rows without promoting a
partial observation to a full row pass. BETA-11 acceptance/publication remains open.

Every executed row must name the source, native/installer/VSIX digests, Windows/editor/runtime versions, store, command, expected result and actual outcome. Use `pass`, `fail`, `not run` or `excluded from declared support` explicitly. A passing unit test cannot fill an installed-product row.

| Area | Required observations |
| --- | --- |
| Production build and identity | Build current source without qualification features; validate native target/version, locked dependencies and source stability. Independently verify staged/archived inventory, notices and final checksums/signatures. |
| Clean installation | Standard user on clean supported Windows; no source checkout, developer Node/Rust/MSVC or prepopulated model cache. Exercise spaces/Unicode, custom install/data paths, missing PowerShell where applicable, insufficient space, failed install/retry and actionable diagnostics. Installation must not require hidden developer state. |
| First useful CLI task | Complete supported onboarding and metadata renewal; initialize workspace, explicitly grant trust, configure budget/checks and run a bounded task. Exercise missing/wrong credential, missing tools, expired/catalog-mismatched profile and denied effects without secret disclosure or silent fallback. Live provider calls require separate budget admission. |
| First useful editor task | Install actual VSIX using the final native installation and clean VS Code profile. Connect observer/controller, start a task, inspect progress, pause/resume and apply a reviewed edit. Test restricted mode, uninitialized workspace, wrong engine/data root and incompatible handshake. |
| Cross-client configuration | Imported MCP allowlist/deadline restrictions apply identically in CLI and editor/SDK. Stale base/import revision, corrupt chain and changed policy refuse safely, including resume and reconnect. |
| Editor lifecycle and buffers | Actual restart/reload, changed roots, pending/unknown commands, partial edits, typing between review/apply, undo, dirty buffers and rejected VSIX update. No duplicate effects, silent save/overwrite, implicit control acquisition or replay. Use both stores. |
| Upgrade, rollback, uninstall | Distinct compatible native and VSIX versions with real paused tasks/history/accounting; active owners/locked executables; process interruption and competing installer operations. Refuse unsupported newer state. Preserve byte-identical user workspaces, history, profile/model data, independent keys and vault sentinels. |
| Skills, memory and helpers | Validate shipped catalog/resources and real helper smoke checks from installed locations. Explicit model acquisition/verification, missing/corrupt assets, offline local-memory build/query and applicable helper dependency failures. Observe production network denial where claimed, rather than inferring it from configuration. |
| Trust, privacy and recovery | Relevant path/junction/process, MCP/hook/child, credential redaction, retention/purge, ciphertext publication and tamper/wrong-key cases. Preserve acknowledged records and unresolved accounting liabilities. Include controlled full-volume/kill recovery and independent-machine restore wherever required by the support claim. |
| Performance and usability | Measure startup with retained history, resource use, long checks and pause/termination behavior on the declared hardware envelope. Recheck the recorded 130-version startup scenario. Report measured distributions/sample limits and human interventions; do not infer a hardware floor from one workstation. |
| Owner/product acceptance | Join current FR/I/U01–U09 dispositions to the beta evidence, including analysis/review/generation quality, architecture fit, routing, memory, MCP/skills, delegation and recovery. Reuse unaffected evidence with a reason; owner review is not an automated score. |

Unauthorized effects, ignored user restrictions, silent user-edit loss, acknowledged-record loss, unaccounted work, restricted-context disclosure, false completion, hidden children and plaintext cloud-bound state are stop conditions. Known-issues wording cannot turn such a failure into a passed gate.

## 6. Existing commands and release artifacts

Use the delivered workflow for the immediate milestone, after the selected main
commit's ordinary Delivery checks pass:

```powershell
gh workflow run beta-candidate.yml --ref main `
  -f reviewed_commit=<exact-reviewed-main-sha> -f stop_after=pair
```

Follow [candidate operations](../development/beta-candidate.md) for evidence and
the [manual checklist](04-manual-testing-checklist.md) for the focused handoff.
The lower-level commands below are historical/expert entry points, not evidence
that a complete beta has been qualified. Production release assembly requires
the strict release-mode inputs documented by the current candidate workflow.

```powershell
# Portable contracts; these do not run a complete installed-product campaign.
npm.cmd test --prefix src/packages/sdk-ts
npm.cmd test --prefix src/packages/vscode
pwsh -File scripts/test.ps1 -Suite fast

# Existing optimized native build; produces a unique directory and receipt.
pwsh -File scripts/build-production.ps1 -OutputRoot artifacts/beta-build -Jobs 2

# Substitute the executable and receipt from the SAME successful build.
pwsh -File scripts/package.ps1 -Executable <absolute-vcp.exe> `
  -BuildReceipt <absolute-build-receipt.json> -OutputRoot artifacts/beta-native

# From src/packages/vscode, using that native candidate's result.json.
npm.cmd run build
node scripts/package.cjs --engine <absolute-packaged-vcp.exe> `
  --engine-manifest <absolute-native-result.json> --output <new-vsix-output>
```

The delivered workflow also invokes `scripts/build-setup.ps1` and constructs the
strict beta VSIX and pair. BETA-04/06/07 implementation is already delivered;
do not reimplement the original review's missing setup/version features.

Reuse the existing [distribution smoke](../../scripts/evals/distribution-qualification.ps1), [production upgrade runner](../../scripts/evals/production-distribution-qualification.ps1), [production recovery runner](../../scripts/evals/production-recovery-qualification.ps1), [startup runner](../../scripts/evals/production-startup-qualification.ps1) and [native editor tests](../../src/crates/vcp-cli/tests/local_editor_package.rs). Consult their actual parameters and [distribution guide](../development/p8-distribution.md); native editor tests require explicit qualification inputs and are not covered by `npm test`. Keep plaintext runtime fixtures and recovery material in private, unsynchronized directories outside the repository. Retain only sanitized release receipts in shared artifacts.

The release packet must contain:

- The Windows setup artifact, portable payload ZIP and matching VSIX; final SHA-256 checksums and signature status.
- Build, signing/transformation, payload and compatibility manifests connecting every artifact to one reviewed source selection.
- The actual component/license inventory, applicable notices, skill/model identities and user prerequisite matrix.
- Installation/upgrade/recovery quickstarts, beta release notes, known issues and the exact supported platform envelope.
- The qualification matrix with links to durable evidence, unresolved/excluded rows and recorded owner acceptance/publication decision.

## 7. Validation performed during this review

| Check | Result and limit |
| --- | --- |
| Source review | Inspected build/package/installer scripts, CI, CLI profile loading/onboarding and public execution, VSIX staging/versioning, SDK launch boundaries and relevant tests/contracts. Cross-checked current task ledgers and historical package reports. This was a targeted release review, not an exhaustive audit of all vendored code. |
| Documentation checks | Checked 54 Markdown links; every local target exists. Reviewed the requested file and checked its diff for whitespace errors. No other source or documentation files changed. |
| `node --test src/tests/contracts/package-inventory.test.cjs src/tests/contracts/package-install.test.cjs` | **Passed: 5 tests, 0 failures.** Covers payload inventory/hash/path limits and compatibility declarations. Does not execute the PowerShell installer or prove real-store upgrade behavior. |
| `node scripts/test-runner.cjs --suite fast` | **Failed to start the suite:** `Harness failure: Source hashing command failed: 129`. No suite pass is claimed. Direct Git diff inspection worked; the harness failure's cause was not established or repaired in this documentation-only review. |
| `npm test --prefix src/packages/sdk-ts`; `npm.cmd test --prefix src/packages/vscode` | **Incomplete.** Both stalled during SDK TypeScript compilation without test-case output and were interrupted. Their builds briefly overlapped before coordination, so this is not isolated build evidence or a demonstrated product failure. Rerun sequentially in a working environment. |
| Production build, installer/VSIX assembly, installed editor, clean Windows, fault campaign and live provider evaluation | **Not run.** No new artifacts, signing, model downloads, paid calls, owner acceptance or publication were performed. Historical results are identified separately above. |

The review itself marked no `BETA-*` item complete. Subsequent implementation and delivery are recorded separately in the implementation ledger.
