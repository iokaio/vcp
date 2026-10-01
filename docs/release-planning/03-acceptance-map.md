# Internal beta FR/I/U acceptance map

September 30, 2026. Owning item: **BETA-09**, the
[owner/product row](00-release-plan.md#5-required-beta-qualification-matrix).
**Final-beta disposition: not run; owner acceptance not recorded.** This is an
evidence join, not a score, approval, or completed qualification matrix.

[ADR-073](../adr/073-manual-testing-candidate.md) permits the focused owner
manual-testing handoff before this full acceptance join is complete. It does
not change any historical or final-beta result recorded below.

The [task ledger](../plan/20-traceability.md#functional-requirement-coverage)
owns FR-01–17 and I-01–19; the
[acceptance specification](../plan/16-test-fixtures-and-acceptance.md#acceptance-scoring-construction)
owns U01–U09 assertions and human judgments. The
[historical scorecard](../evaluations/p8-release-scorecard-2026-09-22.md#requirement-and-invariant-join)
is the complete earlier FR/I disposition, supplemented by the
[gap report](../evaluations/p8-acceptance-gaps-2026-09-22.md) and
[unapproved owner packet](../evaluations/p8-owner-acceptance-package-2026-09-22.md).
[ADR-042](../adr/042-owner-directed-p8-closure.md) closed P8 for development
progression; it preserved failed, unknown and unrun observations and did not
approve publication. Current beta scope comes from
[ADR-072](../adr/072-internal-windows-beta.md), with the
[implementation ledger](02-implementation-status.md) recording subsequent work.

## Evidence carried forward and changes requiring requalification

Historical evidence retains its own source, artifact, fixture, environment and
outcome. Reuse below means retaining component evidence, independent oracles and
known counterexamples for the stated reason. No current artifact equivalence has
been established, so none supplies a final-beta `pass`. Applying a dated runtime
result to the beta requires an explicit unchanged-input/dependency argument;
otherwise rerun the affected case under the
[invalidation rule](../plan/16-test-fixtures-and-acceptance.md#reproducibility-and-evidence-invalidation).

| Current boundary / supporting record | Reuse reason and beta consequence |
| --- | --- |
| [BETA-02/03/06/07/10](02-implementation-status.md): effective imported profiles, provider setup/renewal, registered launcher, strict VSIX and user guides | Source/native regressions prove their recorded cases and supply final-run assertions. New executable selection, setup, imported authority and client behavior require final installed CLI/editor parity and a clean-host walkthrough. Synthetic provider responses do not prove live admission. |
| [P10 hooks](../evaluations/p10-01-hooks.md) and [configuration imports](../evaluations/p10-02-configuration-imports.md) | Reuse explicit-authority, no-replay, unchanged-source and crash oracles: beta does not relax these contracts. BETA-02 changes effective-profile consumption. The later [SH close-out](../research/skills-upgrade-plan.md#sh-close-out-2026-09-30) reports a local hook status/pause test failure. The [current diagnosis](02-implementation-status.md#hook-responsiveness-regression) isolated unrelated fixture setup exceeding the readiness deadline; the corrected source test passed on default SQLite with all original control deadlines. This does not supply final installed evidence. |
| [Munarium refresh](../evaluations/sp-02-munarium-refresh.md) | Reuse recorded gate/source compatibility and scoped-reader regressions: canonical governance and accounting were preserved. The changed reader/cache dependency still needs final-engine inference, index/reopen, corruption and resource observations; the refresh's model-assets case was ignored. |
| [SP skills](../research/skillsplan-new.md#work-items-and-delivery-ledger) and [SU/SH changes](../research/skills-upgrade-plan.md#sh-close-out-2026-09-30) | Reuse package hashes, scoped tool/materialization and focused helper regressions as boundary evidence. Resource roles, discovery, helpers and guidance changed after P8; old prompt/quality results cannot establish current usefulness. Retain SH's untested hook-origin payload, skipped AES dependency case, synthetic Excel fixtures and detection limits. The current [proportionate skills policy](../research/skillsplan-new.md#development-and-acceptance) supersedes blanket comparative campaigns. |
| [Earlier editor qualification](../development/editor-packaging.md#qualification-record), [current final-artifact runners](../development/beta-candidate.md), [recovery/startup handoff](../development/beta-qualification-handoff.md) | Reuse protocol, preservation and failure oracles; formats remain unchanged. Earlier VSIX bytes and same-source successor fixtures do not prove this beta or a distinct supported upgrade. Compiled runners, synthetic fixture preparation and process-supervision tests are prerequisites, not installed observations. |

## Owner-suite join

Every row below has final-beta status **not run**. The FR/I references cover all
functional requirements and invariants when combined with the distribution row
below; IDs retain their ledger definitions.

| Suite and FR/I join | Supporting historical disposition and reusable evidence | Final-artifact / live / human observations still required |
| --- | --- | --- |
| **U01 analysis** — FR-01/04/05/11; I-01/08 | [September 23 campaign](../evaluations/p8-approved-campaign-2026-09-23.md#owner-observations): both store attempts reached the request limit without a final answer. Retain source-preservation and truth-set oracles, plus these failures. | Freeze current held-out architecture fixture, source revisions, finding/unsupported-claim thresholds and cost/time limits; run integrated routing, memory and skills on final bytes. Human review of explanation and architecture fit remains absent. |
| **U02 review** — FR-07/12; I-06/12/18 | Same campaign: both reviews passed automated controls; human review remained pending and inaccurate asides were recorded. [P7 review/generation](../evaluations/p7-qwen38-reasoning-budget-2026-09-22.md) supplies bounded comparative context, not a current quality score. | Current seeded/benign review, visible attributed children, zero workspace writes, complete cost evidence and human usefulness/severity judgment. Editor stale-draft refusal runners do not establish successful review/apply. |
| **U03 generation** — FR-07/12; I-06/12/18 | September 23: SQLite task remained incomplete; Files retained its failed original gate, with a separate audit identifying a harness lookup defect. Both supplemental feature checks passed; original outcomes stay intact. Reuse integrated-parent and preservation oracles. | Final held-out cross-module change with existing staged/unstaged/untracked and unsaved work; current-parent verification, reviewed application and recovery observations. Human architecture-fit review and declared quality/budget limits remain required. |
| **U04 encrypted handoff** — FR-10/14; I-03/07/13/15/19 | [P8 recovery/security gaps](../evaluations/p8-acceptance-gaps-2026-09-22.md) retain local crypto/restore and earlier two-Windows evidence. Reuse exact record/ledger/key and unsafe-activation assertions because format compatibility is unchanged. Independent-machine handoff was skipped. | Final compatible encrypted restore on both stores, independent-machine/copy-return observations, complete divergence/corruption and ciphertext-only publication evidence. New synthetic local fixtures cannot substitute for the skipped machine case or physical full-volume evidence. |
| **U05 history/pruning** — FR-04/06/15; I-07/08/09/17 | [Packaged history/security follow-up](../evaluations/p8-history-security-followup-2026-09-22.md) and scorecard retain full-history, typed-secret, compaction/reopen/revocation/purge checks. Reuse selector/protected-reference and retained-copy oracles; beta does not authorize broader deletion. | Final retention/export and sensitive-surface audit, stale preview and recall checks across both stores; owner review of history visibility, cleanup explanation and backup-copy limits. |
| **U06 interruption/delegation** — FR-06/08/10/12; I-02/03/10/11/18 | [P7 interruption](../evaluations/p7-interruption-exploration-2026-09-22.md) and [September 23 interactive observation](../evaluations/p8-approved-campaign-2026-09-23.md#live-interactive-observation) retain dispatch/effect and unknown-liability evidence. Reuse no-blind-replay and child accounting oracles. The later hook test failure was resolved by isolating its readiness fixture; current default-SQLite source coverage is recorded separately from installed qualification. | Final same-process pause/resume, close/kill/reopen, child stop, reconciliation and visible progress with declared latency limits. Launcher console coverage cancels a paused chooser; it does not prove interruption of paid work. Recheck lifecycle hooks on final bytes across both stores. |
| **U07 routing/optimization** — FR-02/03/09/17; I-04/05/14 | [P6 disposition](../evaluations/p6-completion.md): deterministic baseline/selected policy remain available; rejected/unqualified optional advice stays disabled. Reuse eligibility, reservation, attribution and rollback assertions. [Later renewal](../evaluations/p8-allowance-package-2026-09-23.md#authorized-renewal-outcome-and-distribution-follow-up) failed with unresolved liability; the consumed proposal grants no retry. | Current live provider/catalog evidence and bounded authorization before paid work; matched declared owner comparisons with all root/child/evaluator costs and unknown charges. Human quality/cost/latency assessment and current policy inspection/rollback remain open. |
| **U08 skills/MCP/hooks/imports** — FR-11; I-01/02/11/14 | Current SP/SU/SH and P10 records above supersede the older catalog/configuration assumptions. Reuse focused discovery, hash, authority, schema-change, delayed-effect and helper preservation oracles. A shipped skill grants no tool or installation authority. | Final installed catalog/helpers, current effective import restrictions in CLI/editor, relevant hook cases and controlled MCP identity/schema/cancellation observations. Record real helper prerequisites/failures and authoring warnings. The prepared helper target is not execution; axe, real Excel and other unrun variants remain explicit. Owner ecosystem-guidance review remains open. |
| **U09 local memory** — FR-05/16; I-07/08/14/16 | [Native CPU evidence](../evaluations/p8-native-qualification-2026-09-22.md), scorecard and Munarium refresh supply real-inference/scoped-query oracles. Reuse these assertions because local-only, authorized-source semantics remain required. Current MiniLM acquisition verifies assets only. | Final offline build/query/reopen and ANN-versus-exact checks on declared corpus/truth sets; missing/corrupt assets and dependency failures; independently observed production network denial. Record CPU/memory/latency/recall envelopes; a development workstation does not establish minimum hardware or clean-host support. |
| **Distribution join** — FR-13, plus FR-01; I-14 | [BETA-04/05/08](02-implementation-status.md) deliver strict provenance, production-component notices and candidate/evidence tooling. Reuse source reconstruction/license records for their exact pinned components and packaging regression oracles. Historical archive success and green PR checks identify different scopes. | Exact successful native/setup/VSIX pair, complete notices/inventories, clean installed walkthrough and distinct supported upgrade/rollback. Failed candidate attempts retain failure status; unsigned internal scope is explicit. BETA-11 owner decision and distribution authorization remain separate. |

## Closing a row

Attach source and final native/setup/VSIX digests, environment/runtime versions,
store, fixture/truth-set and configuration identities, command, expected result,
actual result, interventions, costs and retained uncertainty. Record `pass`,
`fail`, `not run` or an explicitly authorized `excluded from declared support`
per variant; never convert a historical skip into a current support exclusion.
Predeclare applicable quality/sample/resource thresholds, keep executable results
separate from human judgments, and preserve failed attempts. Clean Windows,
network denial, independent-machine recovery, full-volume evidence and owner
review remain visible gates under ADR-072; ordinary reproducible defects require
repair and affected reruns. Only the owner supplies acceptance and publication
authorization after the exact evidence packet is reviewable.
