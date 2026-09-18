# 01 — Native Windows foundation and upstream feasibility

Status: all nine P0 tasks complete for bounded feasibility. These results do not qualify production integration or a release. Owns P0-01 through P0-09. Read [delivery conventions](00-delivery-contract.md), architecture sections 0–3 and 19.5, and [the upstream inventory](../architecture/open-source.md). Exact prerequisites are preserved in [the ledger](20-traceability.md).

Use the [upstream experiment procedure](../development/upstream-qualification.md) for manifest/reconstruction details and the [qualification harness design](../architecture/qualification-release-design.md) for result records and fault supervision. [ADR-001](../adr/001-runtime-topology.md), [ADR-003](../adr/003-canonical-storage.md), [ADR-008](../adr/008-local-governed-memory.md), [ADR-015](../adr/015-portability-and-storage-choice.md) and [ADR-019](../adr/019-cloud-encryption-and-keys.md) now record the confirmed directions and pending experiments separately.

## Outcome and code surface

Produce a reproducible Codex-derived Windows baseline and evidence that local Munarium/Tantivy/DiskANN plus encrypted portable storage can fit it. Follow [the code layout](code-layout.md). Proposed outputs: pinned workspace/toolchain configuration under `src/`, `src/third_party/upstreams.toml`, component/patch notes, prototype source under the chosen workspace, `src/tests/fixtures/`, `scripts/build.ps1`, `scripts/test.ps1`, and ADR-001/003/008/013/015/019. Build outputs remain ignored; the concrete source map and setup instructions go in `docs/development/`. These are prototypes, not completed VCP release capabilities.

## P0-01 — Requirements and experiment harness

Implementation: the [delivery harness](../development/delivery-harness.md) provides registry validation, process execution, result manifests and regression checks. [Experiment helpers and fixtures](../development/experiment-fixtures.md) add the scripted provider, fake clock, owned roots, observation log, analysis/review/generation truth sets and measurement proposals. [Executed evidence](../evaluations/p0-01-experiment-harness.md) qualifies P0-01 only; no product acceptance result is implied.

1. Record A01–A17, FR-01–FR-17, I-01–I-19 and the ADR register. The P0-01 baseline covered ADR-001–019; later [ADR-020](../adr/020-bounded-semantic-decisions.md) adds P6 planning without changing that historical qualification. Link decisions to their owner segments and initial test IDs.
2. Create the minimal runner/result manifest from segment 00, plus fake clock, scripted model responses and isolated temporary-root helpers. The initial runner can invoke upstream commands; it need not bootstrap a new engine.
3. Specify disposable analysis/review/generation fixtures and known expected findings. Propose workload sizes and hardware measurements without inventing a minimum machine or dollar cap.

Test unknown suite rejection, nonzero child exit propagation, isolated fixture roots and exclusion of synthetic credentials from reports. Done when experiments can be reproduced and none of the owner requirements is left as an optional release feature.

### Build the experiment skeleton

Create a machine-readable registry only when the first real experiment is wired to it. Each entry binds suite/case, owning task, concrete command, fixture version, required host/assets and expected artifact categories. Dispatch by validated registry entry; do not treat arbitrary suite names as shell fragments. Record `not_run` before execution when native Windows or a pinned asset is missing.

Implement the result recorder before costly experiments: source commit/dirty identity, actual tool versions, candidate pin, configuration, fixture/seed, commands, outcomes, repetitions and raw artifact references. Keep acquisition, baseline and modified runs distinct. Add a supervisor-owned effect log and explicit fault barriers before building a large test framework. [Harness contracts](../architecture/qualification-release-design.md#harness-architecture) define the boundary.

Propose separate quality thresholds for analysis findings, review precision/recall and generated behavior/architecture fit; leave final values unselected until the owner task set is known. Do not expose held-out answers in model-visible fixtures. Classify every ADR as confirmed direction, proposed mechanism or unresolved engineering choice.

## P0-07 — Immutable upstream selection

Completed feasibility: the [consolidated selection gate](../evaluations/p0-07-selection-gate.md)
supersedes the partial statuses in the historical progression below. Current
patches and concrete retained/replaced destinations are in the
[handoff map](../development/p0-handoff.md).

Implementation progress: [candidate pins and commands](../development/upstream-candidates.md) provide original-byte inventories and an executable native Windows baseline experiment. [Native results](../evaluations/p0-07-native-candidates.md) record Codex's CLI build and 102 patch/policy tests, Munarium's 79 kernel/store tests, and Gemini's 213 policy/scheduler/tool tests. The [committed Codex selection](../development/codex-source.md) adds 7,937 files, license/closure records, independent reconstruction and a direct native build command. Complete effect classification and other selected component closure remain outstanding; P0-07 stays in progress.

The [static boundary inventory](../evaluations/p0-07-boundary-inventory.md) checks all 154 Codex workspace packages, their proposed ownership and 25 source anchors. The [adapter guide](../development/codex-boundaries.md) and [Gemini candidate record](../../src/third_party/components/gemini-cli.md) identify concrete next inputs. These checks do not qualify runtime effects or complete the remaining P0-07 gate.

The [compiler compatibility experiment](../evaluations/p0-07-common-rust.md)
records the failed unmodified Rust 1.98.0 build, a minimal attributed compatibility
patch, independent reconstruction and follow-up native checks. It does not yet
by itself qualify a combined workspace; the subsequent evidence below covers
shared-workspace builds and the earlier hosted Codex environment.

The [hosted Windows results](../evaluations/p0-07-hosted-windows.md) establish
committed-source build and independent reconstruction on a second Windows
environment. The `win8core` job passed 40 regressions, 102 native patch/policy
tests and five CLI traces. It also exposed and verified a fix for short-path
output containment. Other component selections and effect qualification remain
open; this result does not complete P0-07 or the later VCP lifecycle work.

The [committed Munarium selection](../development/munarium-source.md) adds 70
files and three libraries to the same Cargo workspace. [Import evidence](../evaluations/p0-07-munarium-import.md)
records 200 native tests, unchanged Codex package pins and exact reconstruction.
At that import, the static inventory covered 157 packages, 22 groups and 30 anchors.
Gemini selection, remaining effect qualification and later runtime/asset gates
remain open; P0-07 stays `in_progress`.

The [Gemini comparison runner](../development/gemini-baseline.md) adds explicit
preparation, isolated profiles and complete-result checks for 402 selected native
tests. [Evidence and retained failures](../evaluations/p0-07-gemini-baseline.md)
cover argument encoding, skills and MCP alongside policy/scheduler/tools. The
candidate remains external; P0-09 owns ports and intentional semantic divergences.

The [local embedding helper](../development/local-embeddings.md) adds pinned
Candle/Tokenizers CPU inference, an immutable external MiniLM asset inventory and
independent numerical fixtures to the shared workspace. [Executed evidence](../evaluations/p0-07-local-embeddings.md)
distinguishes this seam from P0-02's corpus/index, OS network-denial and resource
gates. The static inventory now covers 158 packages, 23 groups and 32 anchors.

The [helper effect map](../development/helper-effect-traces.md) and
[native follow-up](../evaluations/p0-07-helper-traces.md) extend the trace to
nine cases and the catalog to 36 anchors. Review and Responses-based compaction
are independently observed, including helper rejection, parent usage gaps and
prompt-history replacement. P0-03/P0-08 still own lifecycle/accounting adapters;
remaining module effect qualification keeps P0-07 in progress.

The subsequent [classified effect inventory](../development/upstream-effect-classes.md)
assigns all 158 packages conservative effect ceilings and primary VCP owners,
with 43 named entries covering update/maintenance routes as well as model helpers.
The [selection acceptance map](../evaluations/p0-07-selection-gate.md) consolidates
each P0-07 obligation and its evidence. P0-07 is complete after the final
classification PR passed both hosted runners and merged. Runtime enforcement, real local
corpus/reopen and in-app pause remain the separate tasks below; source
qualification does not satisfy their acceptance contracts.

1. Pin Codex, Gemini CLI and Munarium to exact commits; record selected paths, dependencies, licenses, notices, patch origin and local owner. Treat mutable URLs as discovery inputs.
2. Build selected upstream components on native Windows before modifications, including the unmodified Codex engine/CLI baseline. Use an isolated temporary checkout; record compiler/native dependencies, commands, resource use and known upstream failures.
3. Classify each selected module's I/O, model/network helpers and ambient credential discovery. Establish the VCP replacements required for each effectful seam.
4. Follow [ADR-013](../adr/013-upstream-reuse-and-vendoring.md): import the selected Codex source as ordinary tracked files under `src/third_party/codex/`, retaining useful upstream-relative workspace structure. Include original license/NOTICE material and root attribution with the import. Do not introduce a submodule, gitlink, nested Git repository, or subtree workflow.
5. Define the concrete provenance schema, source-selection/path mapping, original/resulting hashes, and explicit reconstruction procedure. Record any exclusions or normalization. Build from the committed source; keep import/verification helpers under planned `scripts/upstream/`, separate from ordinary compilation. Commit only when that implementation task authorizes it.

Test both a clean VCP clone containing the imported files and an independent reconstruction from the pinned upstream selection and ordered patches, comparing paths, file hashes, and notices. Once patches are added, the reconstructed result must equal the committed, already-patched source. A build must not fetch Codex or apply those patches again; ordinary dependency provisioning remains documented separately. Include selected embedding assets and crypto dependencies in provenance as they are chosen. Done when another Windows environment can build the committed inputs and independently reconstruct them without an unrecorded developer cache.

### Pin, inventory and reconstruct

Implement the [source-selection schema](../development/upstream-qualification.md#proposed-source-selection-manifest) with immutable repository identity, exact selected paths, destination mapping, original/result bytes and ordered patches. Reject duplicate/case-colliding paths, missing internal dependencies and branch-only revisions. Record source text normalization explicitly; otherwise preserve bytes. Never infer selected-file rights from the root license alone.

Build the unmodified selection in a disposable root and record its package targets, compiler/native dependencies and tests before integration. Produce an effect inventory covering CLI entry, retries, review, compaction, credentials, telemetry, updates and maintenance. Each retained path must identify whether it is pure, read-only or effectful and which VCP seam will own its effects.

Keep import/reconstruction as explicit maintenance commands. The comparison fails for added, missing or changed paths as well as mismatched hashes. A clean VCP build and a reconstruction comparison are independent results. Preserve actual upstream failures so P0-08 cannot count a pre-existing red test as a newly passing adaptation.

## P0-02 — Local Munarium and search spike

Prerequisite evidence: [Munarium's native datastore baseline](../evaluations/p0-07-munarium-datastore.md)
passes 200 upstream tests with Tantivy and DiskANN enabled. The [source/closure map](../development/munarium-baseline.md)
identifies usable libraries and filesystem effects. It has no embedding runtime,
VCP workspace policy or measured production envelope in that earlier run. The
[subsequent CPU helper](../evaluations/p0-07-local-embeddings.md) qualifies local
model loading and numerical behavior separately; corpus/index integration and
offline enforcement remained outstanding at that gate.

The [local corpus prototype](../development/local-memory-spike.md) now connects
the real CPU model, Tantivy and DiskANN in a single qualification executable.
Its public fixture includes two workspaces, semantic intent, repeated identifiers
and an obsolete version; the observer checks fresh-process reopen and corrupted
manifest rejection. [Native evidence](../evaluations/p0-02-local-corpus.md)
records seven passing queries and repeated process reopen. The subsequent
[governance adapter](../development/local-governance-spike.md) uses scoped
Munarium gates and volatile ledgers, preserving rejected proposals and historical
versions while selecting current results. Durable canonical-store integration
remains owned by P0-04/P1/P5; it is not completed by this volatile prototype.
[Offline CPU qualification](../development/offline-embeddings.md) now observes
zero-capability Windows containment, real inference, live network controls and
missing/corrupt asset rejection. See [executed evidence and limits](../evaluations/p0-02-offline-embeddings.md).

The [declared resource gate](../development/local-memory-resources.md) adds
100/1,000/10,000-record fixtures, native resident/private/mapped observations,
sampled logical disk peaks and five warm repetitions in two fresh query
processes. [Resource evidence](../evaluations/p0-02-local-resources.md) separates
governance replay and independent-oracle work from model and lookup timing.
All nine native resource phases and 324 query rows passed. Together these gates
complete P0-02's feasibility acceptance and select the bounded integration
candidate in [ADR-008](../adr/008-local-governed-memory.md). Do not treat these
synthetic measurements as an installed-product limit. Publication still requires
the final PR head to pass CI on the configured runners.

[Governance qualification evidence](../evaluations/p0-02-local-governance.md)
records eight native tests, seven real corpus queries and repeated process
reopen. Replaying fixture claims into memory is not durable governance recovery.

1. Locate the pinned kernel/gates, in-memory backend, conformance scenarios and actual local embedding implementation. Extract a minimal record/evidence/governance example behind candidate VCP interfaces.
2. Build a small local corpus into Tantivy and DiskANN; query exact symbols and semantic intent, close the process and reopen on disk.
3. Measure CPU inference, embedding dimensions/model identity, index build peak RAM, startup and query time. Exercise missing model assets and network-disabled inference.

Run M03/M04/U09 prototype cases and an exact-vector oracle on the small corpus. No remote embedding fallback, PostgreSQL dependency or hosted Munarium process is acceptable. If a selected module does not fit, document a bounded adapter/replacement while preserving the required libraries and local-compute contract.

### Qualify the full local path

Create a corpus with exact identifiers, semantic paraphrases, conflicting versions and two workspaces containing the same symbol. Define relevant/forbidden result IDs independently of the index. Run ingestion, lexical lookup, local embedding, ANN lookup, canonical scope filtering and reopen using real components. Record the embedding model digest, preprocessing, dimension, metric and index-library/provider identity with each run.

Measure cold load, warm inference/query, batch throughput, resident/mapped memory, peak build disk and reopen time separately. Repeat at declared corpus sizes; a tiny success establishes API feasibility only. Disable embedding network access and observe attempted traffic; a configuration flag by itself is insufficient evidence of local execution. Inject missing/corrupt assets and dimension mismatch before selecting the baseline in [ADR-008](../adr/008-local-governed-memory.md).

## P0-03 — Codex lifecycle seam

Completed feasibility: the [recovery implementation](../development/lifecycle-recovery.md) and [native acceptance evidence](../evaluations/p0-03-recovery-execution.md) qualify durable owner checkpoints, startup/model/tool gates, same-process private CLI pause/resume, fresh-process restoration and native process quiescence. Earlier increments below describe the progression; their unfinished obligations are superseded by this result. The [P0-08 host](../development/p0-integration.md) adds bounded coding/accounting integration; global production wiring remains P1/P2/P3.

The [native CLI trace](../evaluations/p0-07-cli-trace.md) now observes completion,
tool receipt/file effects, retry and rejection through the unmodified retained
loop. The [continuation admission increment](../development/continuation-admission.md)
now gates delegated child, review and mailbox starts through an injected host
hook, with [ten native regressions](../evaluations/p0-03-continuation-admission.md).
The [scoped lifecycle host](../development/scoped-lifecycle.md) adds registered
thread admission, independent/inherited holds, retained interruption, owner-loss
denial and revision-checked readmission. See its [local qualification](../evaluations/p0-03-scoped-lifecycle.md).
That earlier scoped-host increment left startup/effect authority, durable
checkpoint/reopen, process-tree quiescence and in-app root/child pause unqualified;
the completed recovery milestone above supplies those results.

Delivery now groups connected work into the [foundation milestones](README.md#pr-milestones-and-local-validation).
The scoped-control milestone combines thread-scoped admission with a host lifecycle
fence, independent/inherited root-child holds, active retained-loop cancellation,
explicit readmission and owner-loss denial, including negative/race cases and
source reconstruction. Do not publish separate PRs for each hook or observer.
Checkpoint/reopen, process quiescence and private CLI integration passed together
in the connected recovery milestone. Local native tests remain the primary
evidence during development.

1. Trace CLI input to controller, context, model request, tools and completion in the pinned code. Identify injectable persistence/model/policy/execution boundaries.
2. Wrap a tiny internal command/event path with workspace/task IDs, deterministic responses, cancellation and visible outcomes.
3. Exercise owner-connection loss and checkpoint hooks. Keep useful internal protocol machinery; defer public API/schema support.

Record a request-to-effect trace and close/reopen experiment. Done when VCP can own lifecycle without an independent upstream task loop continuing after cancellation.

### Locate controller ownership

Trace one scripted turn from CLI input through model/tool scheduling to completion. Annotate concrete function/module names, state mutation owners and asynchronous boundaries in the P0 source map. Prototype injected model/store/budget/policy/executor handles in the smallest retained loop that exercises the trace.

Use commands with stable IDs and steering revisions and worker completions tagged with their originating revision. Pause during a model stream and before tool dispatch, keeping the CLI open for status/inspection; resume through explicit revalidation. Then close the owner and reopen in a fresh process. Observe request/effect counts independently. Determine how private CLI control delivery reaches the owner without a second writer; public attachment remains deferred under [ADR-002](../adr/002-internal-and-public-protocol.md).

## P0-04 — Storage and encrypted portability comparison

Implementation: [the portable-storage prototype](../development/portable-storage-spike.md) compares SQLite WAL/FULL and framed commits, signed age full/incremental snapshots, independent Go/Node cryptographic interoperability, crash barriers and real cache rebuilds. [Qualification evidence](../evaluations/p0-04-portable-storage.md) records local results and the required second-Windows-environment gate. Production schemas, activation and key UX remain P1/P5/P3/P8.

1. Store identical event, claim, artifact-reference and reservation fixtures in SQLite and a framed-file prototype. Compare transaction/reopen semantics before optimizing size.
2. Snapshot a canonical view with matching artifacts/index inputs while writes continue. Encrypt the entire archive outside the sync root; evaluate the Rust age candidate and dedicated developer identity from architecture section 12.11.
3. Restore into a second Windows environment with a separately supplied recovery copy. Measure bundle size, ciphertext transfer churn, CPU/peak disk, restore and backend conversion.
4. Resolve ADR-019's authorized-writer authentication separately from recipient encryption. Prototype the selected trust/signing contract using an existing implementation; test that a stranger knowing a public recipient cannot inject an accepted writer identity. Record key provisioning, rotation and first-machine trust enrollment.

Test both backends, incomplete uploads, wrong key, altered header/payload, missing final chunk, missing artifact and unsigned/untrusted writer substitution. Observe the sync folder during failures for plaintext markers, then verify the cryptographic format independently; marker absence alone is insufficient. Select a qualified default candidate and identify work needed to advertise the alternative. Keys, plaintext manifests and work files stay out of the vault.

### Hold the comparison constant

Serialize the same neutral record IDs, disputes, event sequence, unresolved reservation, artifact references, deletion epochs and generation inputs into each backend. Compare their logical export after commit/reopen; binary file equality is not the parity oracle. Interrupt before/after acknowledgement and validate exactly which acknowledged facts survived.

Snapshot a declared watermark while additional writes occur. Compare full and incremental packaging with encryption enabled for both: encrypted bytes changed per logical change, peak local disk, transfer volume, validation time and time to usable search. Include conversion in both directions and an incompatible binary index rebuilt from retained vectors.

Resolve writer enrollment and anti-replay assumptions explicitly. Knowing a recipient public key must not create writer authority; a fresh offline machine without a trusted latest checkpoint cannot prove global freshness. Record that limitation rather than inventing cloud consensus. See [storage and portability design](../architecture/storage-portability-design.md) and [ADR-019](../adr/019-cloud-encryption-and-keys.md).

## P0-05 — Windows execution spike

Completed after P0-03: [native evidence](../evaluations/p0-03-recovery-execution.md) qualifies the retained Job Object candidate, explicit argv/environment, Unicode/CRLF/long paths, bounded output draining, non-cooperative grandchild termination and exclusive-lock observations. Independent AppContainer/control canaries qualify a single-process filesystem/network candidate. This does not advertise a composed production sandbox; P2/P8 own that integration.

1. Reuse candidate Codex process/job/PTY and policy code for argument-safe process launch with an explicit working root and environment.
2. Exercise Unicode/spaces, CRLF, long paths within supported limits, file locks, junctions and process trees.
3. Measure terminate/close behavior and report which filesystem/network restrictions are actually enforced. Do not silently substitute WSL.

Run E07/E08/R04 against real processes, including a grandchild, an output flood and a termination-resistant fixture. Record residual-process and unknown-effect behavior. P0-06 cannot advertise a sandbox capability backed only by a mock test.

### Observe the operating-system boundary

Use a purpose-built helper accepting structured fixture arguments to spawn a grandchild, hold a file lock, emit arbitrary bytes and create a scoped marker. Launch with an explicit executable/argv/environment rather than shell interpolation. Record process ownership tokens and handle inheritance; PID alone is insufficient after restart.

For each claimed restriction, place independently observed canaries inside and outside allowed roots and record the attempted and actual access. Test junction/path replacement between prepare and use and state any remaining race. Measure scheduling stop separately from process-tree termination. Preserve evidence for residual processes without claiming that a shell exit reverted its filesystem effects.

## P0-08 — Codex integration baseline

Completed for bounded feasibility: [integration evidence](../evaluations/p0-08-09-integration.md)
records the retained read/prepare/patch/native-check/memory loop, shared durable
reservations and receipts, preserved user edits, denied helper/tool bypasses,
ordered source reconstruction and the representative fix-import rehearsal.
Retained lifecycle and native recovery regressions passed on the common Rust
1.98.0 candidate. The [handoff map](../development/p0-handoff.md) identifies
retained/replaced modules and the production responsibilities still owned by P1–P8.

1. Keep the functioning CLI/loop structure and insert candidate OpenRouter, budget, store and memory adapters. Use deterministic provider responses initially.
2. Exercise read, prepared patch, verification and task summary through the retained engine. Trace every helper/model call and remove bypasses.
3. Compare preserving a cohesive module with extraction where coupling prevents VCP contracts. Rehearse importing one representative upstream fix and record effort/patch size.
4. Record C01–C06 destinations and C07 retained tests, supporting libraries, concrete build entry point, and kept/replaced modules in the source map and ADR-013. These responsibility names do not limit reuse to small extracts. Keep one authoritative engine/build graph and explicitly account for credential/network effects.
5. Store every local difference from the pinned Codex selection in the ordered `src/third_party/patches/codex/` series. Update it with the committed patched source and manifest hashes; prove reconstruction in a disposable directory. Remove/disable upstream telemetry and implicit credential discovery, routing needed operations through VCP authority rather than discarding all credential-library primitives.

Pass R02–R05/R08 prototype cases, with user edits preserved and one authoritative store/ledger/controller. A clean-looking fork without working seams is not the deliverable.

### Assemble a proof of integration

Insert one VCP boundary at a time into the functioning baseline: normalized model admission, prepared tools, canonical receipts, then local memory. For each insertion rerun the same deterministic read/patch/check trace and compare observed requests, effects and retained upstream behavior. Keep temporary scaffold interfaces visibly unqualified until the owning production task lands.

Maintain a retained/replaced map containing upstream path, VCP responsibility, reason for changes, hidden effects removed, patch IDs and applicable R tests. Include supporting libraries, not only C01–C06 labels. Use the [engine design](../architecture/engine-execution-design.md) to verify that no upstream scheduler or store still exercises competing authority.

Choose a representative immutable upstream fix whose affected tests are known; reconstruct the updated source and record conflict resolution, regression results, changed lines and engineering effort. This is a maintenance experiment, not permission to advance dependencies automatically or to commit during a documentation task.

## P0-09 — Gemini fixture and port boundaries

Completed for bounded feasibility: [the pinned comparison and Rust port evidence](../evaluations/p0-08-09-integration.md#gemini-comparison)
preserves canonical argument encoding and out-of-order results while explicitly
testing VCP's fresh approval, resource ownership and cancellation receipt rules.
The [component record](../../src/third_party/components/gemini-cli.md#p0-09-attributed-adaptation)
binds the source, license, destination files and comparison-only Node closure.

1. Pin G01 tool invocation, G02 scheduler and G03 policy/confirmation behavior; locate G06 skill/MCP seams.
2. Express language-neutral input/result/event fixtures and retain original expected outcomes. Mark intentional differences for VCP permission ceilings, durable dispatch and accounting.
3. Implement only the small adapters/ports needed to prove those boundaries. Node may run comparison fixtures without becoming an engine runtime dependency.

Test out-of-order tool results, argument rewrites, stale confirmations, resource conflicts and cancellation. G04 hooks and G07 editor utilities remain research for deferred segments.

### Compare port behavior precisely

Define neutral fixtures with canonical arguments, resource declarations, arrival/cancellation schedule and expected semantic events. Normalize only incidental timestamps/IDs in comparisons. Preserve meaningful order, argument hashes, cancellation reasons and partial outcomes.

For each difference record the original outcome, VCP outcome, governing invariant and why an adapter rather than a copied expectation is needed. Remove provider SDK types from the production boundary and route every helper request through the VCP model gateway. A Node comparison fixture is test infrastructure; it does not establish a second runtime dependency for the shipped engine.

## P0-06 — Qualification and handoff

Completed for bounded feasibility: [the consolidated dossier](../evaluations/p0-06-handoff.md)
binds every prerequisite, candidate decision, failed alternative and remaining
production risk. The common Rust 1.98.0 retained CLI build, integration,
lifecycle/recovery and source checks passed. [The setup/source map](../development/p0-handoff.md)
gives reproducible commands and concrete P1 module destinations. All P0 ledger
dependencies are complete; P1 implementation and product acceptance remain planned.

Collect every P0 prerequisite result. Accept concrete source/toolchain mappings, local model/runtime, Windows capability scope, storage recommendation, encrypted format and writer/key trust contract. Capture failed alternatives and replacement scope; do not label unrun prototypes qualified.

Handoff includes a reproducible native build, component map, ADR decisions, fixture/report locations and a concrete list of packages/modules segment 02 will edit. All later source paths may be adjusted within the `src/` layout to this map without changing ownership or contracts in the plan. Update the layout document, affected segments, and scripts together when the concrete mapping changes.

### Close the feasibility gate

Produce one row per candidate/contract with qualified envelope, rejected alternative or not-run status and exact evidence IDs. List remaining risks by the next owning task instead of burying them in a summary. Native Windows process control, local inference, storage parity, encrypted format/writer trust and source maintenance each need their own evidence.

Update the relevant ADRs with selected versions/mechanisms only after those results exist. Publish a setup guide and concrete source map that another clean Windows environment can follow, plus actual build/test commands and required assets. Check all P0 prerequisites in the ledger before marking P0-06 complete; writing this dossier without running the experiments is still planning.
