# Code layout

Status: layout convention adopted; product implementation remains planned. This file defines repository paths used by the [implementation plan](README.md). The [architecture](../architecture/vcp-what.md) remains authoritative for product behavior, and the [delivery contract](00-delivery-contract.md) defines engineering rules.

## Initial roots and current state

Use `docs/` for documentation, `src/` for source code and its test assets, and `scripts/` for build and test automation. Root-level community and agent guidance files plus `.github/` are the exceptions needed for project discovery and contribution workflows. Keep root `AGENTS.md` and `CLAUDE.md` identical.

Today `docs/architecture/` contains the product architecture, supporting implementation designs and research; `docs/plan/` contains execution instructions; `docs/adr/` contains all 20 decision records; and `docs/development/` contains implementation and qualification procedures. ADR-013 records the source-management convention; engineering qualification remains pending throughout the new records. `src/tests/registry.json`, `src/tests/support/harness.cjs`, `src/tests/contracts/`, and `scripts/test.ps1` now implement the [delivery harness](../development/delivery-harness.md). The remaining product packages, fixtures and build runners below are targets. Create directories with their first useful content; do not add empty crates or passing placeholder runners.

## Target directory tree

```text
vcp/
  README.md
  AGENTS.md                    repository guidance for AI agents
  CLAUDE.md                    identical guidance for Claude-compatible tools
  LICENSE
  NOTICE
  THIRD_PARTY_NOTICES.md
  CONTRIBUTING.md
  CODE_OF_CONDUCT.md
  SECURITY.md
  SUPPORT.md
  TRADEMARK.md
  .gitignore
  .gitattributes
  .github/
    ISSUE_TEMPLATE/
    pull_request_template.md
    workflows/                 future CI invoking the same scripts as developers
  docs/
    README.md
    architecture/              design, requirements, and supporting research
    plan/                      execution segments, this layout, and traceability
    adr/                       20 decision records; runtime qualification pending
    development/               implementation guides; concrete setup/source map later
    protocol/                  later public API and compatibility documentation
    operations/                installation, data, recovery, and release guides
    evaluations/               reviewed, redacted result summaries only
  src/
    README.md
    Cargo.toml                 proposed Rust workspace manifest; selected in P0
    Cargo.lock                 committed application dependency resolution
    rust-toolchain.toml        qualified compiler and component pins
    crates/                    VCP Rust packages or mapped cohesive modules
    tests/
      support/                 deterministic helpers and external-effect oracles
      fixtures/                synthetic repositories, events, and service inputs
      contracts/               shared backend and service behavior
      recovery/                crash, interruption, and replay cases
      platform/windows/        real native process and filesystem checks
      end-to-end/              compiled CLI scenarios with controlled providers
    evals/
      tasks/                   evaluation definitions and private fixture references
      fixtures/                synthetic held-out inputs and truth sets
      graders/                 executable scoring and artifact checks
    skills/builtin/            versioned skill content, manifests, and assets
    third_party/
      upstreams.toml           origins, exact commits, licenses, paths, and owners
      codex/                   committed copied source, with reviewed patches applied
      munarium/                selected local libraries, members of the Codex workspace
      gemini-cli/              selected extracts and licensed comparison fixtures
      munarium/                selected kernel/local-memory code and fixtures
      patches/codex/           ordered patches reproducing committed Codex changes
      licenses/                original license and notice texts for imports
    packages/                  deferred TypeScript surfaces
      protocol-ts/             generated protocol bindings
      sdk-ts/                  client connection/subscription code
      vscode/                  extension host and presentation
  scripts/
    README.md
    build.ps1                  planned native build entry point
    test.ps1                   planned suite/case/backend dispatcher
    package.ps1                planned release assembly and checksums
    evals/                     planned evaluation orchestration
    upstream/                  planned explicit import/reconstruction helpers
  artifacts/                   ignored local build, test, and evaluation output
```

The optional `artifacts/` directory is generated output, not a fourth tracked content root. Runtime history, stores, indexes, model caches, vaults, credentials, and developer recovery keys belong in configured local data locations outside the source tree. Do not use a Git checkout as a cloud vault.

## Rust responsibility map

Logical names describe ownership; they do not require a crate per row. P0-07/P0-08 map them to actual files under `src/` after selecting a working Codex baseline. Retain cohesive upstream modules and tests when possible, and record the mapping in `docs/development/` with provenance in `src/third_party/upstreams.toml`.

`src/crates/vcp-embedding/` now implements the file-only CPU qualification helper
in the retained Cargo workspace. [Its source/asset guide](../development/local-embeddings.md)
records this concrete P0 placement; the broader memory and retrieval packages
remain planned integration work.

| Proposed path under `src/crates/` | Responsibility | Owning plan segments |
|---|---|---|
| `vcp-domain/` | IDs, entities, errors, state transitions, and invariants | [02](02-engine-state-and-capture.md) |
| `vcp-protocol/` | Typed internal commands/events; public schemas later | [02](02-engine-state-and-capture.md), [17](17-deferred-api-and-sdk.md) |
| `vcp-engine/` | Session controller, scheduling, completion, recovery, delegation | [02](02-engine-state-and-capture.md), [05](05-openrouter-and-session-loop.md), [06](06-windows-tools-and-recovery.md), [14](14-visible-delegation.md) |
| `vcp-context/` | Instructions, manifests, token planning, and compaction | [04](04-context-and-instructions.md) |
| `vcp-models/` | Normalized requests, OpenRouter transport, and catalog | [05](05-openrouter-and-session-loop.md) |
| `vcp-routing/` | Group selection, profiles, escalation, and optimization | [12](12-routing-and-optimization.md) |
| `vcp-decision/` | Planned bounded Boolean/Choice/Score advice, deterministic evaluator and thin Rust OpenRouter adapter for actual Jev plus qualified conventional-LLM comparator/fallback | [12](12-routing-and-optimization.md#p6-02--deterministic-profile-policy) |
| `vcp-budget/` | Reservations, settlement, and root/child cost attribution | [03](03-storage-and-budget.md) |
| `vcp-policy/` | Authority, trust, approvals, and dispatch admission | [06](06-windows-tools-and-recovery.md) |
| `vcp-tools/` | File, search, patch, process, and Git contracts | [06](06-windows-tools-and-recovery.md) |
| `vcp-exec/` | Workers, process trees, PTY, and OS adapters | [06](06-windows-tools-and-recovery.md) |
| `vcp-repository/` | Workspace identity, file revisions, and Git context | [04](04-context-and-instructions.md), [06](06-windows-tools-and-recovery.md) |
| `vcp-store/` | Canonical backends, artifacts, migrations, snapshots, keys, vaults, restore | [02](02-engine-state-and-capture.md), [03](03-storage-and-budget.md), [10](10-history-and-pruning.md), [11](11-encrypted-portability.md) |
| `vcp-memory/` | Ingestion, claims, governance, pruning, and retrieval orchestration | [08](08-memory-and-ingestion.md), [09](09-local-search-and-generations.md), [10](10-history-and-pruning.md) |
| `vcp-search-tantivy/` | Lexical index schema, stable IDs, and generations | [09](09-local-search-and-generations.md) |
| `vcp-search-diskann/` | Vector provider, filters, and generation lifecycle | [09](09-local-search-and-generations.md) |
| `vcp-extensions/` | Skills and MCP; deferred hook/import integration | [13](13-skills-and-mcp.md), [19](19-deferred-extensions-and-platforms.md) |
| `vcp-audit/` | Projections, inspectors, redaction, and export | [02](02-engine-state-and-capture.md), [07](07-cli-and-inspection.md), [10](10-history-and-pruning.md) |
| `vcp-cli/` | Interactive CLI, JSONL presentation, and task ownership | [07](07-cli-and-inspection.md), [10](10-history-and-pruning.md), [11](11-encrypted-portability.md), [14](14-visible-delegation.md) |

A conventional Rust crate may have `Cargo.toml`, its own `src/`, and `tests/`; the nested `src` is normal. Put backend-specific migrations with the owning store package. Keep pure unit tests with their modules and use `src/tests/` for shared contracts and cross-package scenarios. The workspace must register shared test targets explicitly: merely creating `src/tests/` does not make Cargo discover them.

`vcp-decision` is a logical P6-02 destination, not an existing crate or an additional
foundation prerequisite. Its proposed `question`, `answer`, `validation`,
`evaluation` and deterministic/OpenRouter adapter modules follow
[ADR-020](../adr/020-bounded-semantic-decisions.md) and the
[decision contract](../architecture/decision-evaluation-design.md#decision-contract).
Keep pure question/answer schemas independent of gateway clients, UI and concrete
stores; orchestration uses injected existing context, ledger and model interfaces.
`vcp-routing` consumes validated advice, while policy, budget and completion
authorities remain outside the evaluator. Shared fixtures belong under the planned
`src/tests/fixtures/decision/`; labelled held-out evaluations use `src/evals/`.
Within the existing OpenRouter adapter, qualify actual Jev as the preferred
specialized candidate and keep a conventional OpenRouter LLM as a separately
qualified comparator/permitted fallback. Implement protocol and native-answer
mapping in Rust under the
[Jev gateway contract](../architecture/decision-evaluation-design.md#jev-through-openrouter-qualification),
without assuming a Chat Completions model-name substitution is sufficient. The
deterministic baseline remains a distinct implementation. Create no empty package,
direct TypeSafe gateway/credential facility, Jev SDK, LangChain dependency,
Python/JavaScript decision runtime or local model dependency from this map.

## Dependency and upstream boundaries

- Domain and internal protocol types do not depend on presentation, network clients, or concrete stores.
- The engine calls injected model, budget, policy, store, memory, and execution interfaces. Every effect follows the same authority and durability rules, including effects from retained upstream code.
- CLI and future client packages issue commands and consume projections; they do not own a second scheduler, model gateway, or cost ledger.
- Memory accepts canonical records independently of search-index visibility. Embedding and search implementations stay local.
- Optional semantic evaluation calls the existing OpenRouter boundary with root attribution and current authority. It cannot recursively select/evaluate its own model, create a separate gateway or make local memory/retrieval depend on remote advice. The deterministic baseline remains usable without it.
- Keep vendored source and patch history identifiable. For an attributed port placed directly in a VCP module, record both the original and destination paths. Preserve upstream notices, fixtures, and relevant modification markers.
- A retained upstream workspace may remain under `src/third_party/codex/` if flattening it would harm reuse. P0 chooses one authoritative build graph and documents the concrete manifest; do not maintain two independent engines to fit the diagram.

## Codex source ownership and build participation

[ADR-013](../adr/013-upstream-reuse-and-vendoring.md) fixes the repository convention: copy the selected pinned Codex source into `src/third_party/codex/` and commit it as ordinary VCP files, with reviewed patches already applied. Preserve useful upstream-relative structure such as `codex-rs/`. This is the engine/CLI foundation in the VCP build, not a reference-only checkout or a requirement to rewrite it into separate `vcp-*` crates.

No Git submodule, gitlink, nested `.git`, or Git subtree workflow is planned. Normal builds consume the committed source without fetching Codex or applying patches. Explicit maintenance checks reconstruct it in a temporary directory from the pinned selection in `src/third_party/upstreams.toml` plus the ordered series in `src/third_party/patches/codex/`, then compare the result with the committed tree. Keep source, selection/hash records, patches, and applicable notices synchronized in each change. Other retained upstreams use separately identified patch records under `src/third_party/patches/` as needed.

P0-07 owns revision selection and the unmodified native Windows baseline; P0-08 integrates VCP adapters and records retained modules versus replacements. The [current Codex source map](../development/codex-source.md) identifies the committed Cargo workspace at `src/third_party/codex/codex-rs/Cargo.toml`, its pinned toolchain, 7,937 selected files, development-only Node tools and native build/reconstruction commands. The [Munarium source map](../development/munarium-source.md) adds three separately attributed libraries to that same workspace; retained Munarium root Cargo files are provenance inputs, not a second build entry point. Logical `vcp-*` modules below remain integration destinations, not existing replacement crates.

## Scripts and generated material

Run repository automation from the root through `scripts/`. PowerShell is the initial build/test interface because native Windows is the first delivery target. Supporting automation may use another pinned tool when justified; document that prerequisite. Product logic, reusable test helpers, and graders belong in `src/`, while scripts dispatch them and collect results.

Build/test scripts must resolve paths relative to their own location, report the concrete commands, propagate nonzero exits, reject unknown options, and distinguish missing prerequisites from passing checks. The planned test interface and suites are in [segment 00](00-delivery-contract.md); packaging is owned by [segment 15](15-integration-and-release.md). Live evaluations require a configured spend cap and must not run as an implicit ordinary check.

Keep build output and raw run evidence in ignored `artifacts/` or tool-native ignored output directories. Place reviewed, redacted evidence summaries in `docs/evaluations/`. Generated protocol bindings belong in the deferred `src/packages/protocol-ts/` package with their generator inputs and regeneration command identified. Do not commit downloaded models, real owner repositories, or private task transcripts as test fixtures.

## Applying the layout to the plan

| Earlier shorthand | Repository-relative target |
|---|---|
| `crates/` | `src/crates/` |
| `tests/` | `src/tests/` |
| `evals/` task definitions, fixtures, graders | `src/evals/` |
| `evals/` runner scripts | `scripts/evals/` |
| `evals/reports/` reviewed summaries | `docs/evaluations/` |
| `skills/builtin/` | `src/skills/builtin/` |
| `third_party/` | `src/third_party/` |
| `packages/` | `src/packages/` |

Unqualified logical names such as `vcp-store/snapshot` still mean modules in the P0 source map, not an additional root directory. Paths quoted from another repository retain their original spelling and are not VCP destinations.

P0 establishes the actual source workspace, provenance map, and first runnable scripts. Feature segments add code and tests in the mapped locations. P8 adds installation/operations guides, reviewed acceptance evidence, and packaging. Deferred packages are created only when their work is scheduled.

When a concrete layout changes, update this file, the affected segment paths, the source map, script entry points, and contributor instructions together. Directory creation never counts as completing a product work item.

## Materializing the P0 source map

P0-07/P0-08 produce one row per logical responsibility with the actual manifest/package, module paths, public boundary types, retained upstream origin, patch/port identity, test target and maintenance owner. A logical responsibility implemented in an existing Codex package points to that package; do not create a redundant facade crate simply to fill this table.

Start by tracing the existing upstream dependency graph and entry points. Mark modules that perform model calls, writes, process launches or credential/network discovery. Place VCP admission and receipt boundaries around those paths, then prove through the [upstream experiments](../development/upstream-qualification.md#native-baseline-and-effect-inventory) that no competing path remains. Keep one selected build graph and record its concrete entry command.

For a new VCP module, choose the narrowest owner that can implement the behavior without importing UI/store/network details into domain types. Pure reusable helpers live with that module. Cross-backend assertions live once in shared contracts; platform helpers live under shared support when used by multiple packages. Register those test targets in the actual build graph and verify discovery with the runner, rather than counting directories.

## Documentation and contract ownership

| Document type | Authoritative purpose | Update trigger |
|---|---|---|
| [Product architecture](../architecture/vcp-what.md) | Confirmed behavior, scope and invariants | Explicit product clarification or reviewed design correction |
| [Supporting designs](../architecture/README.md#supporting-implementation-designs) | Proposed records, algorithms, failure ordering and cross-service interfaces | A contract is refined or a qualified mechanism replaces a proposal |
| [ADR records](../adr/README.md) | Decision, alternatives, evidence, consequences and unresolved gates | Named experiment selects/rejects an engineering choice |
| [Plan segments](README.md) | Implementation sequence, task ownership and exit evidence | Work is refined or a concrete source mapping becomes available |
| [Development guides](../development/README.md) | How to prepare/reproduce implementation and source selection | Real toolchain/source/runner procedure changes |
| Operations/evaluations | Tested procedures and measured results | Implementation produces current validated evidence |

Do not duplicate wire schemas or migration definitions by hand across documents and source. Once implemented, name their canonical source/generator and use examples tied to its version. Until then, label illustrative fields and command interfaces as proposed. Adding a design does not establish a supported protocol.

## Build and output placement checks

Before adding a manifest/script, identify its input roots and exact generated outputs. Builds consume committed source and declared dependencies; maintenance reconstruction is explicit under `scripts/upstream/`. Runner scripts validate options, resolve their own paths and call real registered targets. Reusable production logic and graders remain under `src/` rather than embedded in orchestration.

Verify output rules with scoped ignore checks before running or staging new output. Packaging uses an explicit file inventory; ignored status does not stop a broad archive from including local data. Runtime stores, models, keys and vaults belong outside the checkout. Test resources have distinct ownership markers and verified cleanup targets.

When a concrete mapping changes, inspect consumers as a set: module imports, workspace members, test registration, scripts, package inventories, provenance, contributor setup, affected plan paths and ADR evidence. Preserve task IDs/dependencies unless the architecture explicitly changes their contract. This is a placement and integration checklist, not a mandate to implement deferred packages early.
