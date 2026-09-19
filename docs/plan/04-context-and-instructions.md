# 04 — Repository context, AGENTS.md and continuity

Status: P2-01 in progress with the [native repository/context increment](../evaluations/p2-context-increment.md); P2-08 in progress with [deterministic compaction](../development/p2-context-continuity.md). Owns P2-01 and P2-08. P2-01 requires durable capture/storage; P2-08 follows the session loop, budget and projections. Consult architecture section 6 and [routing](12-routing-and-optimization.md) for the model-envelope handshake.

## Implementation references

The [context requirements](../architecture/vcp-what.md#6-context-instructions-and-compaction)
and [proposed context/provider design](../architecture/context-provider-design.md)
define source manifests, instruction applicability, selection, send invalidation
and compaction. [ADR-009](../adr/009-context-and-capture.md) tracks engineering
choices. Use [scope/revision identities](../architecture/engine-execution-design.md#identity-revisions-and-ownership)
consistently with memory and retention; code names below remain proposed until
P0 source mapping and P2 implementation.

## Code organization

`vcp-repository` owns `identity`, `roots`, `discovery`, `ignore`, `file_version`, `git_state` and optional bounded `repo_map`. `vcp-context` owns `instruction_loader`, `sources`, `manifest`, `token_plan`, `assembler`, `refresh`, `compaction` and `handoff`. Inject filesystem/scope readers, tokenizer estimation, artifact storage and the model capability envelope; retain chosen C05/G05 code behind those contracts.

Define `ContextPart` with source ID/type, trust class, scope, source revision/hash, artifact ID, inclusion reason, token estimate and omitted ranges. A `ContextManifest` binds ordered parts to task/steering/policy/model/skill/tool/memory revisions. The serialized model request retains a link back to that manifest.

## P2-01 — Discovery and initial assembly

1. Register stable workspace/root identities independently of absolute paths. Observe Git staged/unstaged/untracked state without resetting it. Additional roots are explicit; a repository name alone cannot unify histories.
2. Reuse qualified ignore/file-discovery behavior with bounded directory traversal and file sizes. Resolve links/junctions against permitted roots, label binary/generated files and capture content only within policy. Expose skipped inputs and limits.
3. Load default AGENTS.md from applicable parent/nested scopes. Record exact content/revision and path applicability. Explicit user requirements retain precedence; instructions and skills cannot widen trusted execution policy. Foreign configuration imports stay deferred.
4. Assemble operating/task instructions, applicable project instructions, activated skills/tools, task state, repository/memory evidence and required conversation pairs as separate attributed parts. Untrusted source text cannot become a system instruction by serialization.
5. Fit the candidate model envelope, including output reserve, schema tokens and margin. Report selected/excluded parts. A smaller fallback model triggers reassembly and cost admission against the actual resulting request.

A repository-map experiment may borrow Aider concepts after pinning and comparison; it must justify useful context per token against simple discovery. Do not let an optional map block basic code analysis.

Build P2-01 in four independently observable increments:

1. **Observation:** return stable root/repository/worktree identity and a bounded
   staged/unstaged/untracked manifest without modifying Git state. Version dirty
   content explicitly; HEAD alone is not a workspace fingerprint. Include reasons
   for omitted/denied/oversize/generated paths.
2. **Instruction applicability:** resolve one authoritative AGENTS.md loader for
   parent/nested scopes. For a change spanning sibling directories, retain which
   instructions apply to each path. Parent reads outside the workspace need
   authorized instruction-read scope and do not grant arbitrary root access.
3. **Selection:** construct mandatory and optional `ContextPart` sets, deduplicate
   source/version/range overlap, rank optional evidence deterministically and
   retain exclusion reasons. Instructions, latest constraints, pending tool pairs
   and preconditions cannot be sacrificed to fit a smaller model.
4. **Sealing:** serialize candidate roles/schemas, count the resulting request
   under the qualified tokenizer/estimate, record revisions and artifact IDs,
   and pass the actual size to routing/admission. If mandatory content cannot
   fit, return context-capability failure or explicitly partition the work.

Use a proposed dependency manifest with source hashes, root-binding revision,
instruction/skill/tool revisions, steering/policy revisions, memory generation
and current authority/deletion revisions. Revalidate only relevant dependencies
plus scope fences; a whole-tree rehash on every chunk is unnecessary. Filesystem
watchers accelerate invalidation but never prove freshness. Resolve junctions and
identity again near actual reads; reject escaped targets before capturing bytes.

Provider role conversion must preserve the logical trust classification. Source
code saying “ignore instructions” remains quoted/attributed evidence rather than
being inserted as an operating instruction. If a model cannot express a required
role/tool distinction, mark it incompatible instead of flattening semantics.
An exact prompt inspector uses retained serialized bytes; it must not rebuild
history from the current contents of changed files.

Do not load foreign instruction formats as a side effect of reusing an upstream
loader. AGENTS.md is the initial convention; explicit compatibility imports belong
to P10-02. A repository map is a versioned navigation hint and may fall back to
lexical/path discovery for unsupported languages or stale parsers. Measure useful
evidence per token using the same fixture tasks and include failures in the map
comparison.

## P2-08 — Refresh and compaction

1. Refresh affected source/instruction/tool/policy versions before dispatch and after edits. Invalidate prepared work whose steering or relevant preconditions changed.
2. Pin objective, latest user constraints, current diff/base, unresolved effects/costs, decisions and acceptance work. Selectively compact conversation projection while retaining full original artifacts.
3. Route model-assisted compaction through the model gateway and budget ledger. Store summary inputs/output/evidence and version; deterministic summarization can be local where appropriate.
4. Form explicit handoff packets with scope, current state, references, remaining budget and compatible tool-call/result pairs. Record discarded provider-specific opaque fields rather than inventing missing reasoning.

Implement an invalidation table rather than a single “context dirty” flag:

| Change | Required action |
|---|---|
| New user steering | Re-evaluate affected prepared work; include the latest accepted constraints |
| Edited source or changed root binding | Refresh dependent ranges/preconditions and affected verification |
| AGENTS.md or activated skill revision | Re-resolve applicability before affected dispatch |
| Tool/MCP schema change | Rebuild schemas and prepared calls, then re-count the request |
| Catalog/model envelope change | Reassemble and obtain admission for the actual new request |
| Memory access revoke/prune | Exclude direct and derived affected content; invalidate cached summaries |
| New late usage/effect observation | Refresh unresolved-state/accounting fields without rewriting prior evidence |

Apply the [send fence](../architecture/context-provider-design.md#selection-and-envelope-handshake)
between preparation and transport access. If a relevant access/prune revision
commits first, do not send the stale manifest. If dispatch already passed that
boundary, cancel where possible and report that sent content cannot be recalled.
Do not hold a store transaction over the provider request to simulate remote
atomicity.

Compaction reads a coherent source range and produces a new immutable summary
artifact, input-reference manifest and summarizer/algorithm version. Validate
that required task fields remain outside the summary, then commit the projection
only if source/steering/access dependencies still apply. If summary generation
fails or races a correction, preserve the previous valid projection and full
history. Record token gain and stop repeated no-gain compaction. Model-assisted
compaction consumes an ordinary root reservation and cannot consume protected
verification/reporting funds accidentally.

Test a long trace with an early obsolete assumption, a late user correction, an
unfinished tool call and a post-cancellation unknown charge. Assert those current
facts structurally after compaction, not by comparing generated prose. Later
pruning must also invalidate summaries derived from removed evidence. Handoff
tests use deliberately incompatible role/tool formats and assert either valid
pairs or an explicitly recorded attributed-summary conversion.

## Tests to implement

| Case | Fixture/action | Assertion |
|---|---|---|
| Scoped instructions | Parent and nested AGENTS.md conflict on different files | Correct applicable instruction manifest; user constraint and trusted policy preserved |
| Hostile context | Retrieved source asks to reveal keys/change grants | Included as attributed data; attempted effect still denied by policy |
| Human edits | User changes staged/unstaged/untracked data during assembly | New fingerprint detected; no overwrite or reset |
| Discovery limits | Junction escape, large file, generated tree, missing root | Bounded traversal with explicit exclusion and no scope leak |
| Envelope change | Tool catalog grows; fallback has smaller context | Reassembled request fits; accurate omission metadata and fresh reservation |
| Forced compaction | Long trace includes a late correction and uncertain tool | Objective/correction/unknown effect retained; original transcript unchanged |
| Handoff | Model role/tool serialization differs | Valid pairs and constraints preserved; incompatible opaque fields labelled |
| Reopen | Workspace moved; files changed while VCP was closed | Identity rebind and observed-change record; actor remains unknown |

Store fixtures under `src/tests/fixtures/context/` with small exact expected manifests and version markers. Assert inclusion/authority properties rather than exact model prose. Run `context`, relevant `tools`, and E02/E04/R02/R07. Later U01/U03/U07 test the same behavior with real tasks.

Done when every dispatched model request is attributable to versioned inputs, source/steering changes prevent stale actions, and compaction/handoff preserve task correctness without discarding full retained history.
