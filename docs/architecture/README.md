# Architecture and research

- [VCP architecture](vcp-what.md) defines intended behavior, requirements, invariants, reuse boundaries, and delivery scope. It is the product design authority.
- [Coding-agent research workbook](othertools.md) supplies background investigations and comparisons.
- [Open-source inventory](open-source.md) lists reuse candidates for revision-specific investigation.
- [Model groups](model-groups.md) records research inputs for model routing, including estimates; it is not a current supported-model or pricing catalog.
- [JEV exploration](exploring-jev.md) motivates bounded semantic judgments; the [adoption map](decision-evaluation-design.md#adoption-map) separates useful P6 proposals from unselected vendor integration and unqualified performance claims.

Implementation follows the [plan](../plan/README.md) and [code layout](../plan/code-layout.md). Research notes do not override confirmed requirements or establish that a candidate has passed integration tests.

## Supporting implementation designs

These documents expand the architecture into proposed records, interfaces, algorithms and failure contracts. They do not change task ownership or certify implementations. Library versions, concrete workspace paths and unresolved operational defaults remain subject to their task/ADR gates.

| Design | Contracts and owning plan segments |
|---|---|
| [Engine and execution](engine-execution-design.md) | State, canonical transactions, budget, artifacts, prepared effects, pause/recovery and CLI; 02, 03, 06, 07 |
| [Context and provider](context-provider-design.md) | Workspace/instruction provenance, context assembly/compaction, admitted OpenRouter attempts and completion; 04, 05 |
| [Memory and retrieval](memory-retrieval-design.md) | Governed claims, ingestion, coherent local indexes, query eligibility and pruning; 08, 09, 10 |
| [Storage and portability](storage-portability-design.md) | Neutral records, snapshots, independent keys, encrypted publication, restore and handoff; 03, 10, 11 |
| [Routing and extensions](routing-extensions-design.md) | Groups, optimizer, skills/MCP, isolated children and deferred hook/observer boundaries; 12, 13, 14, 19 |
| [Bounded semantic decisions](decision-evaluation-design.md) | Typed advisory judgments, OpenRouter admission, deterministic fallback and measured rollout; P6 in 12, fixtures in 16, downstream P7/P8 checks |
| [Deferred clients](deferred-clients-design.md) | Public protocol/SDK, editor document receipts, foreign compatibility and execution hosts; 17, 18, 19 |
| [Qualification and release](qualification-release-design.md) | Test registry, result records, independent fault oracles, quality comparisons and packaged acceptance; 00, 01, 15, 16 |

The [20 ADR records](../adr/README.md) separate confirmed directions from proposed engineering choices and evidence gates. [Development guides](../development/README.md) cover implementation packets and upstream qualification. Explicit pause while the CLI stays open is part of the [lifecycle contract](vcp-what.md#45-terminal-close-pause-and-workspace-resume), alongside pause on owner loss.
