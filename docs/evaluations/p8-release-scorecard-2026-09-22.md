# P8 release readiness scorecard — 2026-09-22

**Release disposition: incomplete; owner acceptance not recorded.** This P8-05
preparation increment closes two missing native command receipts, reconciles
P8-01–04 acceptance, and supplies an [owner packet](p8-owner-acceptance-package-2026-09-22.md).
It does not complete P8-05. The [gap report](p8-acceptance-gaps-2026-09-22.md)
is the remaining-work inventory. Machine handoff is excluded from this
continuation by owner direction and remains not run for release acceptance.

## Candidate and evidence identity

| Identity | Value |
|---|---|
| Delivery source | `c963b751b862d4ff84c7df6c4f998c44df6559a3` (merged PR 117); receipt runs reuse unchanged qualification checkout `b1c08a21a7e9756fd4d3cbbe79cc4928e7653412` |
| Matrix source content SHA-256 | `9333232971c926d8e001f721ad2ca73b7d825679ebb0cd63c6f15020b1cd994b` |
| Package build provenance | `74da9bb23c76297f349d986ee73be61c81126d48` plus recorded dirty source/build receipt; commit alone does not identify these bytes |
| CLI SHA-256 | `afdd9e0011601c059d82f6f1cc264b59e5a2627f20db36774c701ce5734f4bbd` |
| ZIP SHA-256 | `f4045381457ddc84e1c32ff4108622523dba1e6851d779628ed2eb42101064b7` |
| Package inventory receipt SHA-256 | `8cd0f1135e63837501b37f9bdbe313383dc347b8d0bc4b9589a33a5190e64956` |
| Native test binary SHA-256 | `836e9c7e7500ee73062d45713415bba03a4d026437310a48c5d1befbc10fc0a1` |
| Candidate kind | Unsigned Windows AMD64 debug qualification build; qualification feature and loopback test facilities enabled; production release build/signing remain separate |
| Observed host | Windows 11 Pro 26200, x64, NTFS, PowerShell 7.6.6; Threadripper PRO 5975WX, 137,295,024,128 bytes RAM |
| Native tools | Rust 1.95.0, MSVC 14.44.35207, two Cargo jobs; Node 24.21.0, Git 2.43.0.windows.1 |
| Local assets | MiniLM revision `1110a243fdf4706b3f48f1d95db1a4f5529b4d41`, ten verified files / 91,578,299 bytes; independent age v1.3.2 |

The [current-machine report](p8-local-qualification-2026-09-22.md) binds build,
inventory, model and fresh-profile installation receipts. Matrix and build
source digests have different scopes and are not interchangeable. The receipt
join verifies identical product/package bytes; the only source change from the
original broad matrix is the documented history-fixture correction. Historical
results retain their original source identities. No artifact was rebuilt here.

## Complete receipts for the two remaining executable cases

The supervisor ran from `2026-09-22T22:08:33.6958883Z` to
`2026-09-22T22:09:46.4239382Z`: **72.728 seconds**, exit 0, no timeout against a
300-second ceiling. Only the two selected cases ran; no full matrix rerun or
paid model call occurred.

| Case | Observed outcome | Command wall time |
|---|---|---:|
| `p8-03-native-mcp-content-conversation` | Pass; exact test observed, exit 0; both-store conversation roles, selectors and cached-artifact provenance | 63.593 s |
| `p8-01-malformed-native-executable` | Pass; exact test observed, exit 0; unattended launch failure preserves unknown dispatch evidence | 5.838 s |

The commands are `cargo test --locked --offline -p vcp-lifecycle --features
qualification --test canonical_host <exact-test> -- --exact`, launched in the
retained Codex workspace through `scripts/evals/p8-native-command.ps1` with
Rust 1.95.0. Exact test selectors are respectively
`mcp_content_coding::coding_content_selectors_roles_and_cached_artifact_provenance_reach_real_provider_loop`
and `verification::native_verification_preserves_uncertain_dispatch_intent`.

Fresh receipt directory:
`artifacts/p8-local-supplements/e8636c5f-561b-4169-92ac-caf2d3e5b190/`.
Its `result.json` SHA-256 is
`14b3b0516c09a08355ce8f442f7dbb005e644ce298ba10a29e3777537c7187a9`;
`matrix/manifest.json` SHA-256 is
`1053aefde241cf091fb76aa86e7e6f127a9a61ba1f13b4ad3e462951399a193e`.
Every selected stdout/stderr/command digest was verified. The runner's own
manifest is incomplete because unselected rows remain not run, as intended.

The superseding aggregate is `artifacts/p8-receipt-closure/result.json`, SHA-256
`733a133fb8c5c7cdfd5f3db0fc7719e5fafa2f16ed2a07b71a6e9a901a1c2e78`.
It carries each of the 47 case IDs, source receipt and receipt hash. It joins
36 passes from the broad matrix, eight from the earlier supplement and these
two passes: **46 pass, zero selected final failures, one not run**. The remaining
`p8-03-packaged-encrypted-recovery` case requires independent-machine recovery.
The aggregate remains **incomplete**. Earlier failures, manual interventions and
interrupted receipts are preserved, including the original 44-pass disposition;
this is not a clean 47-case campaign or a complete P8 acceptance claim.

## First-release task closure

The [authoritative task ledger](../plan/20-traceability.md) supplies each task's
owner, exact dependencies and historical evidence links. Its P8-05 transitive
closure contains **56 tasks: 51 complete and five in progress**. Deferred P4,
P9 and P10 contribute 12 excluded tasks. The repository contract checks graph
closure, owner uniqueness, architecture dependency equality and requirement
inventory; this validates documentation consistency, not runtime acceptance.

| Included task IDs | Count | Recorded disposition and evidence |
|---|---:|---|
| P0-01–09 | 9 | Complete; [prerequisite dossier](p0-06-handoff.md) and task-specific source/native records in the ledger |
| P1-01–06 | 6 | Complete; [foundation acceptance](p1-completion.md) |
| P2-01–08 | 8 | Complete; [coding-loop acceptance](p2-completion.md) |
| P3-01–06 | 6 | Complete; [CLI](../development/p3-cli.md), [terminal](../development/p3-terminal.md), inspector/history/continuation records and historical [two-Windows recovery](../development/p3-onedrive-qualification.md) |
| P5-01–10 | 10 | Complete; [integrated memory](p5-08-integrated-memory.md) and ledger-linked native boundaries |
| P6-01–05 | 5 | Complete; [routing/optimizer disposition](p6-completion.md), including rejected and disabled optional advice |
| P7-01–06 | 6 | Complete; ledger-linked skills/MCP, [workflow](p7-02-completion.md), [review/generation](p7-qwen38-reasoning-budget-2026-09-22.md), [delegation/interruption](p7-interruption-exploration-2026-09-22.md) |
| P8-06 | 1 | Complete; [upstream maintenance rehearsal](p8-upstream-maintenance-2026-09-22.md) |
| P8-01–04 | 4 | In progress; 46 passing executable matrix cases plus explicit [contract gaps](p8-acceptance-gaps-2026-09-22.md) |
| P8-05 | 1 | In progress; this scorecard and owner packet prepared; final integrated owner runs and approval absent |

Historical completion does not prove final installed-package integration. The
separate [Markov and workflow follow-up ledgers](../plan/20-traceability.md#markov-follow-up-readiness)
still govern M5/M6/M8/M9 and required packaged refinements. Disabled/rejected M4,
deferred M7 and deferred M10 must retain those dispositions; no optional model
advice is enabled by this scorecard.

## Requirement and invariant join

These rows join every FR/I ID to the ledger's suite ownership and the release
evidence/gap disposition. **Partial** means passing component evidence exists,
but the complete current-package acceptance remains open. Suite registration is
not execution: `src/tests/registry.json` registers fast harness contracts, while
`scripts/evals/p8-qualification-manifest.json` registers the 47 native cases.
The aggregate above selects actual command receipts; U suites below remain
separate final acceptance. The gap report specifies required unmapped variants.

| Requirement | Suites / current evidence | Release disposition |
|---|---|---|
| FR-01 | E01/E09/R01; P1/P3 CLI and packaged smoke | Partial; final installed integration |
| FR-02 | E11/R05/U07; P2/P6 live provider qualification | Partial; final U07 |
| FR-03 | E12/E19/U07; P1/P6 profile/accounting | Partial; final U07 |
| FR-04 | E13/M01/M05/U05; P5 governance and native matrix | Partial; final U05 |
| FR-05 | M02–M06/E20/U09; P5 CPU search and matrix | Partial; final resource/quality envelope |
| FR-06 | E17/U05/U06; packaged history/long-check evidence | Partial; final owner transparency |
| FR-07 | E05/E07/E15/U03; P2/P7 preserved edits/integration | Partial; held-out U03 |
| FR-08 | E04/E08/E10/U06; native and packaged stop checks | Partial; per-environment timing/integration |
| FR-09 | E12/U07; P1/P6/P7 actual admission/accounting | Partial; final root-cost join |
| FR-10 | E10/E14/M02/M07/M08/U04/U06; matrix recovery | Partial; broader faults; U04 not run |
| FR-11 | E02/E03/E16/U08; packaged skills/MCP and native content | Partial; complete declared toolset matrix |
| FR-12 | E12/E15/U02/U03/U06; P7 delegation | Partial; final integrated owner scenarios |
| FR-13 | R01–R08; source reconstruction/notices/P8-06 | Maintenance rehearsal passes; final distribution identity still open |
| FR-14 | M01/M08/U04/I-19; local crypto/restore | Partial; independent-machine case not run |
| FR-15 | U05/E17/M05; packaged history and P5 pruning | Partial; full final retention/export audit |
| FR-16 | U09/M03/M04; real local CPU fixtures | Partial; declared final corpus/resources |
| FR-17 | E19/U07; P6 selected policy and rollback | Partial; matched final owner comparison |
| I-01 | E02/E03/E16/U08 authority fixtures | Partial; complete packaged hostile-input boundary map |
| I-02 | E10 native intent/effect and policy fixtures | Partial; complete combined fault schedule |
| I-03 | P1 command identity/backend fixtures | Component pass; final integrated recovery join open |
| I-04 | E11/E12 transport/reservation observations | Component pass; final U07 cost join open |
| I-05 | E12/U07/P7 root/child accounting | Partial; all final owner attempts and uncertainty |
| I-06 | E05/E15/U03 workspace preservation | Partial; held-out integrated parent |
| I-07 | M01/M02 native canonical recovery | Partial; broader disk/corruption faults |
| I-08 | M04/M05 native source/scope query fences | Partial; complete final workload map |
| I-09 | E04/E17 packaged full capture/history | Partial; final sensitive-surface audit |
| I-10 | E08/U06 native stop and packaged long check | Partial; integrated timing/variants |
| I-11 | E10/E16/U04/U06 native uncertain effects | Partial; combined recovery and handoff |
| I-12 | E15/U03 P2/P7 current-parent checks | Partial; final held-out generation |
| I-13 | M01/M08 both-store and local conversion | Partial; complete declared fault/backend variants |
| I-14 | E11/E17/U09 unknown/degraded diagnostics | Partial; complete final setup/support matrix |
| I-15 | M08/U04 local activation/tamper checks | Partial; broader divergence/hydration; handoff not run |
| I-16 | M03/M04/U09 CPU/network boundary fixtures | Partial; final corpus and resource targets |
| I-17 | M05/U05 protected pruning and stale recall | Partial; complete final retention/export schedule |
| I-18 | E15/U02/U03/U06 P7 visible child lifecycle | Partial; final integrated owner runs |
| I-19 | M08/U04 independent age local crypto | Partial; full intermediate-vault/sensitive-surface observations |

## U01–U09 and measured quality

| Owner suite | Supporting evidence | Final package/owner gate |
|---|---|---|
| U01 analysis | P2 context and P5 retrieval, P7 source navigation | Not run with final held-out owner fixture/rubric |
| U02 review | Luna strict review and Qwen 3.8 passing baseline/child pair | Not run as final held-out integrated owner suite |
| U03 generation | Luna and Qwen 3.8 current-parent generation checks | Not run as final held-out integrated owner suite |
| U04 encrypted handoff | Historical two-Windows run; current local crypto/restore | Owner excluded machine handoff; not run |
| U05 history/pruning | Current packaged history plus P5 retention | Partial; final integrated retention/export audit open |
| U06 interruption | P7 native terminal, both-store recovery, packaged long checks | Partial; final owner variants and latency thresholds open |
| U07 routing/optimization | P6 qualification; optional advice remains disabled/rejected | Final matched integrated comparison not run |
| U08 skills/MCP | Packaged skills/setup and fresh MCP content receipt | Partial; full declared language/toolset coverage open |
| U09 local CPU memory | Existing real inference/index/query and verified assets | Partial; final corpus/recall/resources/support envelope open |

The [Qwen follow-up](p7-qwen38-reasoning-budget-2026-09-22.md) records review
baseline/child **2/2 defects and zero false positives** each, 312.297/222.027 s,
USD 0.140654/0.105532; wording caveats remain. Generation passed 44/44 integrated
parent checks in 14/16 requests, 456.706 s, USD 0.246074. Review used 16,384 total
output tokens and a 360-second per-response ceiling within a 900-second task;
generation's recorded configuration remains separate. These small selected
fixtures do not establish final owner usefulness or population latency quantiles.
The [Luna report](p7-review-generation-quality-2026-09-22.md) retains its passing
review/generation evidence and the original Qwen endpoint failures separately.

The latest [P7 campaign accounting](p7-interruption-exploration-2026-09-22.md)
records USD **1.278161 settled / 34.127269 reserved** under the USD 100 ceiling.
This continuation adds no paid requests and releases no unresolved reservation.
Final U01–U03/U07 fixture splits, sample sizes and numeric quality/cost/latency
thresholds remain undeclared. Invariants require zero forbidden outcomes.
Historical launch-dialog interventions and interrupted campaigns remain disclosed;
the fresh two-case run required no dialog intervention. No final owner
intervention rate, p50/p95 or minimum-hardware claim is available.

## Completion boundary and next work

Local documentation validation passed: `node src/tests/contracts/repository.cjs`
checked 384 Markdown files, 2,270 relative links, 68 task owners and the exact
56-task release closure with zero errors. `git diff --check` passed. These are
documentation checks; the two native results above are the new runtime evidence.

Receipt capture and acceptance preparation are complete. P8-01–04 require the
bounded missing variants in the gap report; P8-05 requires frozen owner fixtures,
thresholds and a final supported artifact, integrated outcomes and human review.
The owner packet includes install, read-only smoke, task-use prerequisites,
rollback/uninstall and an uncompleted acceptance form. No machine handoff is
scheduled, no acceptance waiver is inferred, and signing/publication are not
authorized by preparing this packet.
