# 04 — Repository context, AGENTS.md and continuity

Status: planned. Owns P2-01 and P2-08. P2-01 requires durable capture/storage; P2-08 follows the session loop, budget and projections. Consult architecture section 6 and [routing](12-routing-and-optimization.md) for the model-envelope handshake.

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

## P2-08 — Refresh and compaction

1. Refresh affected source/instruction/tool/policy versions before dispatch and after edits. Invalidate prepared work whose steering or relevant preconditions changed.
2. Pin objective, latest user constraints, current diff/base, unresolved effects/costs, decisions and acceptance work. Selectively compact conversation projection while retaining full original artifacts.
3. Route model-assisted compaction through the model gateway and budget ledger. Store summary inputs/output/evidence and version; deterministic summarization can be local where appropriate.
4. Form explicit handoff packets with scope, current state, references, remaining budget and compatible tool-call/result pairs. Record discarded provider-specific opaque fields rather than inventing missing reasoning.

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
