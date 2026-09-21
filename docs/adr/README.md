# Architecture decision records

The [architecture ADR register](../architecture/vcp-what.md#221-adr-register) retains the original 20 decision subjects and subsequent engineering decisions, now 28 records. Confirmed product directions remain binding; proposed engineering mechanisms, versions and operational defaults require the named qualification evidence. These records do not mark implementation tasks complete.

ADR-013 records the committed-source repository convention and maintenance evidence. Other records distinguish confirmed scope, qualified P0 mechanisms and unresolved production gates. The [P0 handoff](../evaluations/p0-06-handoff.md) consolidates the bounded engineering evidence; it does not claim human sign-off or release qualification.

| Record | Qualification gate |
|---|---|
| [ADR-001 — Runtime and process topology](001-runtime-topology.md) | P0-02/03/05/08/06; P2-07 |
| [ADR-002 — Internal commands and deferred public protocol](002-internal-and-public-protocol.md) | P1-02; later P9-01/02/03 |
| [ADR-003 — Canonical records and artifacts](003-canonical-storage.md) | P0-04/06, P1-04, P5-09/10 |
| [ADR-004 — Prepared edits and execution receipts](004-edits-and-execution.md) | P2-04/07; later P4-03 |
| [ADR-005 — Autonomy, authority and OS isolation](005-autonomy-and-isolation.md) | P2-03, P8-01 |
| [ADR-006 — Model gateway, catalog and capability groups](006-model-gateway-and-groups.md) | P2-02, P6-01 |
| [ADR-007 — Cost profiles, selection and escalation](007-profiles-and-routing.md) | P6-02/03/04 |
| [ADR-008 — Local governed memory and retrieval](008-local-governed-memory.md) | P0-02/07, P5-01 through P5-08 |
| [ADR-009 — Versioned context and full activity capture](009-context-and-capture.md) | P1-03, P2-08, P3-03 |
| [ADR-010 — Visible bounded delegation and integration](010-visible-delegation.md) | P7-04/05/06 |
| [ADR-011 — Instructions, bundled skills and MCP](011-extension-scope.md) | P7-01/02/03; later P10-01/02 |
| [ADR-012 — Client sequencing and native distribution](012-clients-and-distribution.md) | P8; later P9/P4/P10 |
| [ADR-013 — Upstream reuse and committed vendoring](013-upstream-reuse-and-vendoring.md) | P0-07/08/09; P8-06 |
| [ADR-014 — Explicit foreign compatibility subsets](014-foreign-compatibility.md) | Later P9 and P10-02 |
| [ADR-015 — Portable snapshots, backend choice and handoff](015-portability-and-storage-choice.md) | P0-04, P5-09/10, U04 |
| [ADR-016 — Full history, retention and pause lifecycle](016-history-and-pause.md) | P3-04/05, P5-07, U05/U06 |
| [ADR-017 — Interactive project optimization](017-project-optimization.md) | P6-05/04, U07 |
| [ADR-018 — Complete usable-release acceptance](018-release-acceptance.md) | P8-05 |
| [ADR-019 — Cloud encryption and developer-controlled keys](019-cloud-encryption-and-keys.md) | P0-04/06, P3-06, P5-09/10, P8-03 |
| [ADR-020 — Bounded semantic decisions](020-bounded-semantic-decisions.md) | Proposed P6-02/03/04/05; P7/P8 integration checks under existing dependencies |
| [ADR-021 — Retention replay bases](021-retention-replay-bases.md) | P5-07 sealed replay and physical cleanup |
| [ADR-022 — Retention selection and cleanup](022-retention-selection-and-cleanup.md) | P5-07 exact selection, protection and automatic policy |
| [ADR-023 — Canonical selection activation](023-canonical-selection-activation.md) | P3-06/P5-10 leased descriptor selection and revision-checked activation |
| [ADR-024 — Native skill packages](024-native-skill-packages.md) | P7-01 descriptor discovery, activation and authority |
| [ADR-025 — Built-in skill asset identity](025-builtin-skill-assets.md) | P7-02 lazy packaged assets, integrity and coverage evidence |
| [ADR-026 — Governed sequential MCP stdio](026-governed-mcp-stdio.md) | P7-03 bounded protocol, current source fences and durable call outcomes |
| [ADR-027 — Owned HTTP send boundary](027-owned-http-send-boundary.md) | P7-03 socket admission, scoped credentials and bounded HTTP prerequisites |
| [ADR-028 — Exact MCP schema profile](028-exact-mcp-schema-profile.md) | P7-03 exact numeric values, bounded schema semantics and profile identity |
| [ADR-029 — Retained task-transition evidence](029-retained-transition-evidence.md) | P6-02/M1 scoped causal observations, gaps and rebuild-on-read retention |
| [ADR-030 — Causal action, effect and attempt observations](030-causal-action-observations.md) | P6-02/M1 separate turn/effect/attempt traces, exact cohorts and failure identities |
| [ADR-031 — Exact attempt charge attribution](031-exact-attempt-charge-attribution.md) | P6-02/M1 per-attempt settlements, liabilities and terminal cost rewards |
| [ADR-032 — Rebuildable unqualified Markov fit artifacts](032-rebuildable-markov-fit-artifacts.md) | P6-02/M1 source-bound first-order candidates and explicit abstention |
| [ADR-033 — Task-separated held-out Markov order comparison](033-heldout-markov-order-comparison.md) | P6-02/M1 deterministic held-out partition, complexity penalty and multi-step check |
| [ADR-034 — Exact attempt-visit reward mapping](034-exact-attempt-reward-mapping.md) | P6-02/M1 exact cost samples and unknown-liability mean abstention |
| [ADR-035 — Consumed statistical value retention and replay](035-consumed-statistical-value-replay.md) | P6-02/M1 immutable selected scalar, input identity and historical replay |
| [ADR-036 — Canonical escalation-advisory request and result records](036-canonical-escalation-advisory-records.md) | P6-03/M2 immutable deduplicated inputs, outputs and stale disposition |
| [ADR-037 — Caller-owned escalation-advisory scheduling lease](037-caller-owned-advisory-scheduling-lease.md) | P6-03/M2 single claim, dispatch revalidation and interruption-safe reopen |

## Maintaining a decision

Keep the confirmed requirement, proposed mechanism and measured evidence distinct. At the owning gate, record selected versions/source paths, alternatives actually evaluated, compatibility/migration effects, operational burden, test artifacts and conditions for reconsideration. Preserve rejected alternatives and known limits. An experiment that has not run remains unqualified.

The [subsystem designs](../architecture/README.md) define implementation contracts and failure ordering; the [plan](../plan/README.md) assigns work and acceptance. A supporting design reference is not an additional dependency or proof of completion.

Return to the [documentation index](../README.md).
