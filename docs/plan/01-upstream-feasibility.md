# 01 — Native Windows foundation and upstream feasibility

Status: planned. Owns P0-01 through P0-09. Read [delivery conventions](00-delivery-contract.md), architecture sections 0–3 and 19.5, and [the upstream inventory](../architecture/open-source.md). Exact prerequisites are preserved in [the ledger](20-traceability.md).

## Outcome and code surface

Produce a reproducible Codex-derived Windows baseline and evidence that local Munarium/Tantivy/DiskANN plus encrypted portable storage can fit it. Follow [the code layout](code-layout.md). Proposed outputs: pinned workspace/toolchain configuration under `src/`, `src/third_party/upstreams.toml`, component/patch notes, prototype source under the chosen workspace, `src/tests/fixtures/`, `scripts/build.ps1`, `scripts/test.ps1`, and ADR-001/003/008/013/015/019. Build outputs remain ignored; the concrete source map and setup instructions go in `docs/development/`. These are prototypes, not completed VCP release capabilities.

## P0-01 — Requirements and experiment harness

1. Record A01–A17, FR-01–FR-17, I-01–I-19 and the 19 ADRs. Link each to the owner segment and initial test IDs.
2. Create the minimal runner/result manifest from segment 00, plus fake clock, scripted model responses and isolated temporary-root helpers. The initial runner can invoke upstream commands; it need not bootstrap a new engine.
3. Specify disposable analysis/review/generation fixtures and known expected findings. Propose workload sizes and hardware measurements without inventing a minimum machine or dollar cap.

Test unknown suite rejection, nonzero child exit propagation, isolated fixture roots and exclusion of synthetic credentials from reports. Done when experiments can be reproduced and none of the owner requirements is left as an optional release feature.

## P0-07 — Immutable upstream selection

1. Pin Codex, Gemini CLI and Munarium to exact commits; record selected paths, dependencies, licenses, notices, patch origin and local owner. Treat mutable URLs as discovery inputs.
2. Build selected upstream components on native Windows before modifications, including the unmodified Codex engine/CLI baseline. Use an isolated temporary checkout; record compiler/native dependencies, commands, resource use and known upstream failures.
3. Classify each selected module's I/O, model/network helpers and ambient credential discovery. Establish the VCP replacements required for each effectful seam.
4. Follow [ADR-013](../adr/013-upstream-reuse-and-vendoring.md): import the selected Codex source as ordinary tracked files under `src/third_party/codex/`, retaining useful upstream-relative workspace structure. Include original license/NOTICE material and root attribution with the import. Do not introduce a submodule, gitlink, nested Git repository, or subtree workflow.
5. Define the concrete provenance schema, source-selection/path mapping, original/resulting hashes, and explicit reconstruction procedure. Record any exclusions or normalization. Build from the committed source; keep import/verification helpers under planned `scripts/upstream/`, separate from ordinary compilation. Commit only when that implementation task authorizes it.

Test both a clean VCP clone containing the imported files and an independent reconstruction from the pinned upstream selection and ordered patches, comparing paths, file hashes, and notices. Once patches are added, the reconstructed result must equal the committed, already-patched source. A build must not fetch Codex or apply those patches again; ordinary dependency provisioning remains documented separately. Include selected embedding assets and crypto dependencies in provenance as they are chosen. Done when another Windows environment can build the committed inputs and independently reconstruct them without an unrecorded developer cache.

## P0-02 — Local Munarium and search spike

1. Locate the pinned kernel/gates, in-memory backend, conformance scenarios and actual local embedding implementation. Extract a minimal record/evidence/governance example behind candidate VCP interfaces.
2. Build a small local corpus into Tantivy and DiskANN; query exact symbols and semantic intent, close the process and reopen on disk.
3. Measure CPU inference, embedding dimensions/model identity, index build peak RAM, startup and query time. Exercise missing model assets and network-disabled inference.

Run M03/M04/U09 prototype cases and an exact-vector oracle on the small corpus. No remote embedding fallback, PostgreSQL dependency or hosted Munarium process is acceptable. If a selected module does not fit, document a bounded adapter/replacement while preserving the required libraries and local-compute contract.

## P0-03 — Codex lifecycle seam

1. Trace CLI input to controller, context, model request, tools and completion in the pinned code. Identify injectable persistence/model/policy/execution boundaries.
2. Wrap a tiny internal command/event path with workspace/task IDs, deterministic responses, cancellation and visible outcomes.
3. Exercise owner-connection loss and checkpoint hooks. Keep useful internal protocol machinery; defer public API/schema support.

Record a request-to-effect trace and close/reopen experiment. Done when VCP can own lifecycle without an independent upstream task loop continuing after cancellation.

## P0-04 — Storage and encrypted portability comparison

1. Store identical event, claim, artifact-reference and reservation fixtures in SQLite and a framed-file prototype. Compare transaction/reopen semantics before optimizing size.
2. Snapshot a canonical view with matching artifacts/index inputs while writes continue. Encrypt the entire archive outside the sync root; evaluate the Rust age candidate and dedicated developer identity from architecture section 12.11.
3. Restore into a second Windows environment with a separately supplied recovery copy. Measure bundle size, ciphertext transfer churn, CPU/peak disk, restore and backend conversion.
4. Resolve ADR-019's authorized-writer authentication separately from recipient encryption. Prototype the selected trust/signing contract using an existing implementation; test that a stranger knowing a public recipient cannot inject an accepted writer identity. Record key provisioning, rotation and first-machine trust enrollment.

Test both backends, incomplete uploads, wrong key, altered header/payload, missing final chunk, missing artifact and unsigned/untrusted writer substitution. Observe the sync folder during failures for plaintext markers, then verify the cryptographic format independently; marker absence alone is insufficient. Select a qualified default candidate and identify work needed to advertise the alternative. Keys, plaintext manifests and work files stay out of the vault.

## P0-05 — Windows execution spike

1. Reuse candidate Codex process/job/PTY and policy code for argument-safe process launch with an explicit working root and environment.
2. Exercise Unicode/spaces, CRLF, long paths within supported limits, file locks, junctions and process trees.
3. Measure terminate/close behavior and report which filesystem/network restrictions are actually enforced. Do not silently substitute WSL.

Run E07/E08/R04 against real processes, including a grandchild, an output flood and a termination-resistant fixture. Record residual-process and unknown-effect behavior. P0-06 cannot advertise a sandbox capability backed only by a mock test.

## P0-08 — Codex integration baseline

1. Keep the functioning CLI/loop structure and insert candidate OpenRouter, budget, store and memory adapters. Use deterministic provider responses initially.
2. Exercise read, prepared patch, verification and task summary through the retained engine. Trace every helper/model call and remove bypasses.
3. Compare preserving a cohesive module with extraction where coupling prevents VCP contracts. Rehearse importing one representative upstream fix and record effort/patch size.
4. Record C01–C06 destinations and C07 retained tests, supporting libraries, concrete build entry point, and kept/replaced modules in the source map and ADR-013. These responsibility names do not limit reuse to small extracts. Keep one authoritative engine/build graph and explicitly account for credential/network effects.
5. Store every local difference from the pinned Codex selection in the ordered `src/third_party/patches/codex/` series. Update it with the committed patched source and manifest hashes; prove reconstruction in a disposable directory. Remove/disable upstream telemetry and implicit credential discovery, routing needed operations through VCP authority rather than discarding all credential-library primitives.

Pass R02–R05/R08 prototype cases, with user edits preserved and one authoritative store/ledger/controller. A clean-looking fork without working seams is not the deliverable.

## P0-09 — Gemini fixture and port boundaries

1. Pin G01 tool invocation, G02 scheduler and G03 policy/confirmation behavior; locate G06 skill/MCP seams.
2. Express language-neutral input/result/event fixtures and retain original expected outcomes. Mark intentional differences for VCP permission ceilings, durable dispatch and accounting.
3. Implement only the small adapters/ports needed to prove those boundaries. Node may run comparison fixtures without becoming an engine runtime dependency.

Test out-of-order tool results, argument rewrites, stale confirmations, resource conflicts and cancellation. G04 hooks and G07 editor utilities remain research for deferred segments.

## P0-06 — Qualification and handoff

Collect every P0 prerequisite result. Accept concrete source/toolchain mappings, local model/runtime, Windows capability scope, storage recommendation, encrypted format and writer/key trust contract. Capture failed alternatives and replacement scope; do not label unrun prototypes qualified.

Handoff includes a reproducible native build, component map, ADR decisions, fixture/report locations and a concrete list of packages/modules segment 02 will edit. All later source paths may be adjusted within the `src/` layout to this map without changing ownership or contracts in the plan. Update the layout document, affected segments, and scripts together when the concrete mapping changes.
