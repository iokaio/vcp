# Architecture decision records

The [architecture ADR register](../architecture/vcp-what.md#221-adr-register) retains the original 20 decision subjects and subsequent engineering decisions, now 61 records. Confirmed product directions remain binding; proposed engineering mechanisms, versions and operational defaults require the named qualification evidence. These records do not mark implementation tasks complete.

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
| [ADR-038 — Escalation-advisory helper accounting binding](038-advisory-helper-accounting-binding.md) | P6-03/M2 canonical helper reservation/artifact binding and live charge state |
| [ADR-039 — Escalation-advisory retention and typed redaction](039-advisory-retention-redaction.md) | P6-03/M2 derived-copy closure and non-resurrectable advisory tombstones |
| [ADR-040 — Saved aggregate forecast provenance](040-saved-aggregate-forecast-provenance.md) | P6-05/M3 workspace source manifests, aggregate access checks and typed retention |
| [ADR-041 — Provider evidence and conservative routing](041-provider-evidence-and-conservative-routing.md) | P6 exact catalog association from generation receipts and full-input routed reservations |

## Subsequent owner decisions

- [ADR-042 — Owner-directed P8 closure and P9 continuation](042-owner-directed-p8-closure.md): closes the milestone with explicit qualification gaps; preserves release-evidence and runtime trust requirements.

- [ADR-043 � Public protocol schemas and reconnect identity](043-public-protocol-schema-and-identity.md): canonical public DTOs, generated schema/types and stable authenticated mutation receipts.

- [ADR-044 - Controlled local process bootstrap](044-controlled-local-process-bootstrap.md): authenticated Windows launch and attachment.

- [ADR-045 - Owned local execution](045-owned-local-execution.md): explicit resume, retained event ownership and bounded shutdown.

- [ADR-046 - Atomic metadata session forks](046-atomic-metadata-session-forks.md): historical boundaries, paired genesis and source-scoped receipts.

- [ADR-047 - Durable public run start](047-durable-public-run-start.md): atomic caller identities, immutable root selection and one-use execution admission.

- [ADR-048 - Capability-gated result fields](048-capability-gated-result-fields.md): approval source counters with strict v1.0 compatibility.

- [ADR-049 - Retained public diff evidence](049-retained-public-diff-evidence.md): typed proposal captures, historical source linkage and bounded reads.

- [ADR-050 - Governed public memory inspection](050-governed-public-memory-inspection.md): authenticated claim history, visibility and evidence states.

- [ADR-051 - Authenticated public memory query](051-authenticated-public-memory-query.md): tagged search sources, scoped capture/finish and explicit truncation.

- [ADR-052 - Explicit public memory review](052-explicit-public-memory-review.md): immutable submissions and decisions, atomic governance receipts and source retention.

- [ADR-053 - Scoped local session export](053-scoped-local-session-export.md): atomic derived captures, explicit visibility and source-dependent reads.

- [ADR-054 - Scoped public retention](054-scoped-public-retention.md): exact authenticated previews, original-command reconciliation and scoped physical cleanup.

- [ADR-055 - TypeScript local SDK](055-typescript-local-sdk.md): bounded typed calls, native attachment and explicit consumer cleanup.

- [ADR-056 - Initial editor observer connection](056-editor-observer-connection.md): SDK-backed workspace view with explicit full-item prerequisites.

- [ADR-057 - Editor trust and observer recovery](057-editor-trust-and-observer-recovery.md): controller trust commands, explicit moved-root reconciliation and authenticated observer handoff.

- [ADR-058 - Editor task presentation](058-editor-task-presentation.md): access-checked task summaries, attributed evidence and durable opaque editor actions.

- [ADR-059 - Versioned editor edits](059-versioned-editor-edits.md): connection-scoped observations, per-file version fences and durable uncertain receipts.
- [ADR-060 - Governed inspector queries](060-governed-inspector-queries.md): bounded history and memory pages with current access and retention checks.
- [ADR-061 - Policy and routing inspection](061-policy-routing-inspection.md): bounded observations distinguish stored policy, effective constraints and grant provenance.

- [ADR-062 - Reconciled editor optimizer commands](062-editor-optimizer-commands.md): actor-bound atomic report capture and policy publication, exact previews and durable reconciliation.

- [ADR-063 - Scoped encrypted publisher commands](063-editor-encrypted-publisher.md): durable export intent, public background authority and honest local publication status.

- [ADR-064 - Governed editor inspector views](064-editor-inspector-views.md): transient authorized pages, exact reviews and durable command references.

- [ADR-065 - Governed lifecycle hooks](065-governed-lifecycle-hooks.md): versioned broker execution, fresh rewrite authority and durable deduplication.

[ADR-066](066-versioned-configuration-imports.md) records versioned imports under native authority ceilings (P10-02).

## Maintaining a decision

Keep the confirmed requirement, proposed mechanism and measured evidence distinct. At the owning gate, record selected versions/source paths, alternatives actually evaluated, compatibility/migration effects, operational burden, test artifacts and conditions for reconsideration. Preserve rejected alternatives and known limits. An experiment that has not run remains unqualified.

The [subsystem designs](../architecture/README.md) define implementation contracts and failure ordering; the [plan](../plan/README.md) assigns work and acceptance. A supporting design reference is not an additional dependency or proof of completion.

Return to the [documentation index](../README.md).
